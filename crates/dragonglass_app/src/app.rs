use crate::{gui::Gui, input::Input, logger::create_logger, system::System, universe::Universe};
use anyhow::Result;
use dragonglass_physics::PhysicsWorld;
use dragonglass_render::{Backend, Renderer};
use dragonglass_world::{BoxCollider, Ecs, Entity, Mesh, Transform, World};
use image::io::Reader;
use imgui::{im_str, DrawData, Ui};
use legion::{IntoQuery, Registry};
use log::error;
use nalgebra_glm as glm;
use rapier3d::geometry::Ray;
use std::{collections::HashMap, fmt, path::PathBuf};
use winit::{
    dpi::PhysicalSize,
    event::MouseButton,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
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
    pub universe: Universe,
    pub input: Input,
    pub system: System,
    pub renderer: Box<dyn Renderer>,
}

impl Application {
    pub fn pick_object(&mut self, interact_distance: f32) -> Result<Option<Entity>> {
        // FIXME collision
        todo!();
        // let ray = self.mouse_ray()?;

        // let collision_group = CollisionGroups::new();
        // let raycast_result = self.universe.collision_world.first_interference_with_ray(
        //     &ray,
        //     interact_distance,
        //     &collision_group,
        // );

        // match raycast_result {
        //     Some(result) => {
        //         let handle = result.handle;
        //         let mut picked_entity = None;
        //         for (entity, collider) in <(Entity, &BoxCollider)>::query().iter(&self.universe.ecs)
        //         {
        //             if collider.handle == handle {
        //                 picked_entity = Some(*entity);
        //                 break;
        //             }
        //         }
        //         Ok(picked_entity)
        //     }
        //     None => Ok(None),
        // }
    }

    pub fn mouse_ray(&mut self) -> Result<Ray> {
        // FIXME collision
        todo!()
        // let (width, height) = (
        //     self.system.window_dimensions[0] as f32,
        //     self.system.window_dimensions[1] as f32,
        // );
        // let aspect_ratio = self.system.aspect_ratio();

        // let (projection, view) = self
        //     .universe
        //     .world
        //     .active_camera_matrices(&mut self.universe.ecs, aspect_ratio)?;

        // let mut position = self.input.mouse.position;
        // position.y = height - position.y;
        // let near_point = glm::vec2_to_vec3(&position);
        // let mut far_point = near_point;
        // far_point.z = 1.0;
        // let p_near = glm::unproject_zo(
        //     &near_point,
        //     &view,
        //     &projection,
        //     glm::vec4(0.0, 0.0, width, height),
        // );
        // let p_far = glm::unproject_zo(
        //     &far_point,
        //     &view,
        //     &projection,
        //     glm::vec4(0.0, 0.0, width, height),
        // );
        // let direction = (p_far - p_near).normalize();
        // let ray = Ray::new(Point3::from(p_near), direction);
        // Ok(ray)
    }

    pub fn update(&mut self) -> Result<()> {
        // FIXME collision
        // self.update_colliders()?;
        self.universe.physics_world.update();
        Ok(())
    }

    pub fn render(&mut self, draw_data: &DrawData) -> Result<()> {
        self.renderer.render(
            &self.system.window_dimensions,
            &mut self.universe.ecs,
            &self.universe.world,
            &self.universe.physics_world,
            draw_data,
        )?;
        Ok(())
    }

    /// Add/Syncs basic cuboid colliders for all meshes that do not have one yet
    /// This is meant to allow for basic 3D picking
    fn update_colliders(&mut self) -> Result<()> {
        // FIXME collision
        todo!()
        // let collision_group = CollisionGroups::new();
        // let query_type = GeometricQueryType::Contacts(0.0, 0.0);

        // let mut entity_map = HashMap::new();

        // let entities = <(Entity, &Mesh)>::query()
        //     .iter(&self.universe.ecs)
        //     .map(|(entity, mesh)| (*entity, mesh.bounding_box()))
        //     .collect::<Vec<_>>();

        // for (entity, bounding_box) in entities.into_iter() {
        //     let translation = glm::translation(&bounding_box.center());
        //     let transform_matrix = self
        //         .universe
        //         .world
        //         .entity_global_transform_matrix(&mut self.universe.ecs, entity)?
        //         * translation;
        //     let transform = Transform::from(transform_matrix);
        //     let half_extents = bounding_box.half_extents().component_mul(&transform.scale);
        //     let collider_shape = Cuboid::new(half_extents);
        //     let shape_handle = ShapeHandle::new(collider_shape);

        //     match self.universe.ecs.entry(entity) {
        //         Some(entry) => match entry.get_component::<BoxCollider>() {
        //             // collider exists already, sync it
        //             Ok(collider) => {
        //                 if let Some(collision_object) =
        //                     self.universe.collision_world.get_mut(collider.handle)
        //                 {
        //                     collision_object.set_position(transform.as_isometry());
        //                     collision_object.set_shape(shape_handle);
        //                 }
        //             }
        //             // collider doesn't exist already, create and add it
        //             Err(_) => {
        //                 let (handle, _collision_object) = self.universe.collision_world.add(
        //                     transform.as_isometry(),
        //                     shape_handle,
        //                     collision_group,
        //                     query_type,
        //                     (),
        //                 );
        //                 entity_map.insert(entity, handle);
        //             }
        //         },
        //         None => continue,
        //     }
        // }

        // for (entity, handle) in entity_map.into_iter() {
        //     if let Some(mut entry) = self.universe.ecs.entry(entity) {
        //         entry.add_component(BoxCollider {
        //             handle,
        //             visible: true,
        //         });
        //     }
        // }
        // Ok(())
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
    let renderer = Box::new(Renderer::create_backend(
        &Backend::Vulkan,
        &window,
        &window_dimensions,
        gui.context_mut(),
    )?);

    let mut state = Application {
        universe: Universe::new(),
        input: Input::default(),
        system: System::new(window_dimensions),
        renderer,
    };

    runner.initialize(&mut state)?;

    event_loop.run(move |event, _, control_flow| {
        if let Err(error) = run_loop(
            &mut runner,
            &window,
            &mut state,
            &mut gui,
            event,
            control_flow,
        ) {
            error!("Application Error: {}", error);
        }
    });
}

fn run_loop(
    runner: &mut impl ApplicationRunner,
    window: &Window,
    application: &mut Application,
    gui: &mut Gui,
    event: Event<()>,
    control_flow: &mut ControlFlow,
) -> Result<()> {
    *control_flow = ControlFlow::Poll;

    application.system.handle_event(&event);
    gui.handle_event(&event, &window);
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
            let draw_data = gui.render_frame(&window, |ui| runner.create_ui(application, ui))?;
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
