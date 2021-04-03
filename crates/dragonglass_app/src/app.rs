use crate::{
    gui::Gui,
    logger::create_logger,
    state::{Input, System},
};
use anyhow::Result;
use dragonglass_physics::PhysicsWorld;
use dragonglass_render::{Backend, Render};
use dragonglass_world::{load_gltf, BoxCollider, Ecs, Entity, Mesh, Transform, World};
use image::io::Reader;
use imgui::{im_str, DrawData, Ui};
use log::error;
use nalgebra_glm as glm;
use ncollide3d::{
    na::Point3, pipeline::CollisionGroups, pipeline::GeometricQueryType, query::Ray, shape::Cuboid,
    shape::ShapeHandle, world::CollisionWorld,
};
use std::{collections::HashMap, path::PathBuf};
use winit::{
    dpi::PhysicalPosition,
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyboardInput, MouseButton, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Icon, Window, WindowBuilder},
};

pub struct AppConfig {
    pub width: u32,
    pub height: u32,
    pub is_fullscreen: bool, // TODO: This isn't respected yet
    pub title: String,
    pub icon: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            is_fullscreen: false,
            title: "Dragonglass Application".to_string(),
            icon: None,
        }
    }
}

impl AppConfig {
    pub fn create_window(&self) -> Result<(EventLoop<()>, Window)> {
        let event_loop = EventLoop::new();

        let mut window_builder = WindowBuilder::new()
            .with_title(self.title.to_string())
            .with_inner_size(PhysicalSize::new(self.width, self.height));

        if let Some(icon_path) = self.icon.as_ref() {
            let image = Reader::open(icon_path)?.decode()?.into_rgba8();
            let (width, height) = image.dimensions();
            let icon = Icon::from_rgba(image.into_raw(), width, height)?;
            window_builder = window_builder.with_window_icon(Some(icon));
        }

        let window = window_builder.build(&event_loop)?;
        Ok((event_loop, window))
    }
}

pub struct Application {
    pub ecs: Ecs,
    pub world: World,
    pub collision_world: CollisionWorld<f32, ()>,
    pub physics_world: PhysicsWorld,
    pub input: Input,
    pub system: System,
    pub renderer: Box<dyn Render>,
    pub window: Window,
}

impl Application {
    pub fn set_cursor_grab(&mut self, grab: bool) -> Result<()> {
        Ok(self.window.set_cursor_grab(grab)?)
    }

    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.window.set_cursor_visible(visible)
    }

    pub fn center_cursor(&mut self) -> Result<()> {
        Ok(self.set_cursor_position(&self.system.window_center())?)
    }

    pub fn set_cursor_position(&mut self, position: &glm::Vec2) -> Result<()> {
        Ok(self
            .window
            .set_cursor_position(PhysicalPosition::new(position.x, position.y))?)
    }

    pub fn load_asset(&mut self, path: &str) -> Result<()> {
        load_gltf(path, &mut self.world, &mut self.ecs)?;
        Ok(())
    }

    pub fn reload_world(&mut self) -> Result<()> {
        self.renderer.load_world(&self.world)
    }

    pub fn pick_object(&mut self, interact_distance: f32) -> Result<Option<Entity>> {
        let ray = self.mouse_ray()?;

        let collision_group = CollisionGroups::new();
        let raycast_result = self.collision_world.first_interference_with_ray(
            &ray,
            interact_distance,
            &collision_group,
        );

        match raycast_result {
            Some(result) => {
                let handle = result.handle;
                let mut picked_entity = None;
                for (entity, collider) in self.ecs.query::<&BoxCollider>().iter() {
                    if collider.handle == handle {
                        picked_entity = Some(entity);
                        break;
                    }
                }
                Ok(picked_entity)
            }
            None => Ok(None),
        }
    }

    pub fn mouse_ray(&mut self) -> Result<Ray<f32>> {
        let (width, height) = (
            self.system.window_dimensions[0] as f32,
            self.system.window_dimensions[1] as f32,
        );
        let aspect_ratio = self.system.aspect_ratio();

        let (projection, view) = self
            .world
            .active_camera_matrices(&mut self.ecs, aspect_ratio)?;

        let mut position = self.input.mouse.position;
        position.y = height - position.y;
        let near_point = glm::vec2_to_vec3(&position);
        let mut far_point = near_point;
        far_point.z = 1.0;
        let p_near = glm::unproject_zo(
            &near_point,
            &view,
            &projection,
            glm::vec4(0.0, 0.0, width, height),
        );
        let p_far = glm::unproject_zo(
            &far_point,
            &view,
            &projection,
            glm::vec4(0.0, 0.0, width, height),
        );
        let direction = (p_far - p_near).normalize();
        let ray = Ray::new(Point3::from(p_near), direction);
        Ok(ray)
    }

    pub fn update(&mut self) -> Result<()> {
        self.update_colliders()?;
        self.collision_world.update();
        self.physics_world.update(self.system.delta_time as f32);
        Ok(())
    }

    pub fn render(&mut self, draw_data: &DrawData) -> Result<()> {
        self.renderer.render(
            &self.system.window_dimensions,
            &mut self.ecs,
            &self.world,
            &self.physics_world,
            &self.collision_world,
            draw_data,
        )?;
        Ok(())
    }

    /// Add/Syncs basic cuboid colliders for all meshes that do not have one yet
    /// This is meant to allow for basic 3D picking
    fn update_colliders(&mut self) -> Result<()> {
        let collision_group = CollisionGroups::new();
        let query_type = GeometricQueryType::Contacts(0.0, 0.0);
        let mut entity_map = HashMap::new();
        let mut entries = Vec::new();
        for (entity, mesh) in self.ecs.query::<&Mesh>().iter() {
            entries.push((entity, mesh.bounding_box()));
        }
        for (entity, bounding_box) in entries.into_iter() {
            let translation = glm::translation(&bounding_box.center());
            let transform_matrix = self
                .world
                .entity_global_transform_matrix(&mut self.ecs, entity)?
                * translation;
            let transform = Transform::from(transform_matrix);
            let half_extents = bounding_box.half_extents().component_mul(&transform.scale);
            let collider_shape = Cuboid::new(half_extents);
            let shape_handle = ShapeHandle::new(collider_shape);

            match self.ecs.entity(entity) {
                Ok(entity_ref) => match entity_ref.get::<BoxCollider>() {
                    // collider exists already, sync it
                    Some(collider) => {
                        if let Some(collision_object) =
                            self.collision_world.get_mut(collider.handle)
                        {
                            collision_object.set_position(transform.as_isometry());
                            collision_object.set_shape(shape_handle);
                        }
                    }
                    None => {
                        let (handle, _collision_object) = self.collision_world.add(
                            transform.as_isometry(),
                            shape_handle,
                            collision_group,
                            query_type,
                            (),
                        );
                        entity_map.insert(entity, handle);
                    }
                },
                Err(_) => continue,
            }
        }
        for (entity, handle) in entity_map.into_iter() {
            let _ = self.ecs.insert_one(entity, BoxCollider { handle });
        }
        Ok(())
    }
}

pub trait ApplicationRunner {
    fn initialize(&mut self, _application: &mut Application) -> Result<()> {
        Ok(())
    }

    fn create_ui(&mut self, _application: &mut Application, ui: &Ui) -> Result<()> {
        ui.text(im_str!("Hello!"));
        Ok(())
    }

    fn update(&mut self, _application: &mut Application) -> Result<()> {
        Ok(())
    }

    fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }

    fn on_key(
        &mut self,
        _application: &mut Application,
        _keystate: ElementState,
        _keycode: VirtualKeyCode,
    ) -> Result<()> {
        Ok(())
    }

    fn on_file_dropped(&mut self, _application: &mut Application, _path: &PathBuf) -> Result<()> {
        Ok(())
    }

    fn on_mouse(
        &mut self,
        _application: &mut Application,
        _button: MouseButton,
        _state: ElementState,
    ) -> Result<()> {
        Ok(())
    }

    fn handle_events(
        &mut self,
        _application: &mut Application,
        _event: winit::event::Event<()>,
    ) -> Result<()> {
        Ok(())
    }
}

pub fn run_application(
    mut runner: impl ApplicationRunner + 'static,
    configuration: AppConfig,
) -> Result<()> {
    create_logger()?;

    let (event_loop, window) = configuration.create_window()?;
    let mut gui = Gui::new(&window);

    let logical_size = window.inner_size();
    let window_dimensions = [logical_size.width, logical_size.height];
    let renderer = Box::new(Render::create_backend(
        &Backend::Vulkan,
        &window,
        &window_dimensions,
        gui.context_mut(),
    )?);

    let mut ecs = Ecs::new();
    let world = World::new(&mut ecs)?;

    let mut state = Application {
        ecs,
        world,
        collision_world: CollisionWorld::new(0.02f32),
        physics_world: PhysicsWorld::new(),
        input: Input::default(),
        system: System::new(window_dimensions),
        renderer,
        window,
    };

    runner.initialize(&mut state)?;

    event_loop.run(move |event, _, control_flow| {
        if let Err(error) = run_loop(&mut runner, &mut state, &mut gui, event, control_flow) {
            error!("Application Error: {}", error);
        }
    });
}

fn run_loop(
    runner: &mut impl ApplicationRunner,
    application: &mut Application,
    gui: &mut Gui,
    event: Event<()>,
    control_flow: &mut ControlFlow,
) -> Result<()> {
    *control_flow = ControlFlow::Poll;

    application.system.handle_event(&event);
    gui.handle_event(&event, &application.window);
    application
        .input
        .handle_event(&event, application.system.window_center());
    application.input.allowed = !gui.capturing_input();

    match event {
        Event::NewEvents(_cause) => {
            if application.system.exit_requested {
                *control_flow = ControlFlow::Exit;
            }
        }
        Event::MainEventsCleared => {
            gui.prepare_frame(&application.window)?;
            let ui = gui.context.frame();
            runner.create_ui(application, &ui)?;
            gui.platform.prepare_render(&ui, &application.window);
            let draw_data = ui.render();

            runner.update(application)?;
            application.update()?;
            application.render(draw_data)?;
        }
        Event::WindowEvent {
            event: WindowEvent::DroppedFile(ref path),
            ..
        } => {
            runner.on_file_dropped(application, path)?;
        }
        Event::WindowEvent {
            event:
                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            state: keystate,
                            virtual_keycode: Some(keycode),
                            ..
                        },
                    ..
                },
            ..
        } => {
            runner.on_key(application, keystate, keycode)?;
        }
        Event::WindowEvent {
            event: WindowEvent::MouseInput { button, state, .. },
            ..
        } => {
            runner.on_mouse(application, button, state)?;
        }
        Event::LoopDestroyed => {
            runner.cleanup()?;
        }
        _ => {}
    }

    runner.handle_events(application, event)?;
    Ok(())
}
