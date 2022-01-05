use crate::{
    logger::create_logger,
    state::{Input, System},
};
use anyhow::Result;
use dragonglass_render::{create_render_backend, Backend, Render};
use dragonglass_world::{
    legion::IntoQuery,
    load_gltf,
    rapier3d::{geometry::InteractionGroups, geometry::Ray, na::Point3},
    Entity, RigidBody, SdfFont, World,
};
use image::io::Reader;
use log::error;
use nalgebra_glm as glm;
use std::path::PathBuf;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
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
    pub backend: Backend,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            is_fullscreen: false,
            title: "Dragonglass Application".to_string(),
            backend: Backend::Vulkan,
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
    pub world: World,
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

    pub fn set_fullscreen(&mut self) {
        self.window
            .set_fullscreen(Some(winit::window::Fullscreen::Borderless(
                self.window.primary_monitor(),
            )));
    }

    pub fn load_asset(&mut self, path: &str) -> Result<()> {
        load_gltf(path, &mut self.world)?;
        Ok(())
    }

    pub fn reload_world(&mut self) -> Result<()> {
        self.renderer.load_world(&self.world)
    }

    pub fn mouse_ray(&mut self) -> Result<Ray> {
        let (width, height) = (
            self.system.window_dimensions[0] as f32,
            self.system.window_dimensions[1] as f32,
        );
        let aspect_ratio = self.system.aspect_ratio();

        let (projection, view) = self.world.active_camera_matrices(aspect_ratio)?;

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

    // FIXME: Move picking stuff to world struct
    pub fn pick_object(
        &mut self,
        interact_distance: f32,
        groups: InteractionGroups,
    ) -> Result<Option<Entity>> {
        let ray = self.mouse_ray()?;

        let hit = self.world.physics.query_pipeline.cast_ray(
            &self.world.physics.colliders,
            &ray,
            interact_distance,
            true,
            groups,
            None,
        );

        let mut picked_entity = None;
        if let Some((handle, _)) = hit {
            let collider = &self.world.physics.colliders[handle];
            let rigid_body_handle = collider.parent();
            let mut query = <(Entity, &RigidBody)>::query();
            for (entity, rigid_body) in query.iter(&self.world.ecs) {
                if rigid_body.handle == rigid_body_handle {
                    picked_entity = Some(*entity);
                    break;
                }
            }
        }

        Ok(picked_entity)
    }

    // FIXME: Give world an update method
    pub fn update(&mut self) -> Result<()> {
        self.world.physics.update(self.system.delta_time as f32);
        Ok(())
    }

    pub fn render(&mut self) -> Result<()> {
        self.renderer
            .render(&self.system.window_dimensions, &self.world)?;
        Ok(())
    }
}

pub trait ApplicationRunner {
    fn initialize(&mut self, _application: &mut Application) -> Result<()> {
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

    let logical_size = window.inner_size();
    let window_dimensions = [logical_size.width, logical_size.height];
    let renderer = create_render_backend(&configuration.backend, &window, &window_dimensions)?;

    let mut world = World::new()?;
    world.fonts.insert(
        "default".to_string(),
        SdfFont::new("assets/fonts/font.fnt", "assets/fonts/font_sdf_rgba.png")?,
    );

    let mut state = Application {
        world,
        input: Input::default(),
        system: System::new(window_dimensions),
        renderer,
        window,
    };

    runner.initialize(&mut state)?;

    event_loop.run(move |event, _, control_flow| {
        if let Err(error) = run_loop(&mut runner, &mut state, event, control_flow) {
            error!("Application Error: {}", error);
        }
    });
}

fn run_loop(
    runner: &mut impl ApplicationRunner,
    application: &mut Application,
    event: Event<()>,
    control_flow: &mut ControlFlow,
) -> Result<()> {
    *control_flow = ControlFlow::Poll;

    application.system.handle_event(&event);
    application
        .input
        .handle_event(&event, application.system.window_center());

    match event {
        Event::NewEvents(_cause) => {
            if application.system.exit_requested {
                *control_flow = ControlFlow::Exit;
            }
        }
        Event::MainEventsCleared => {
            runner.update(application)?;
            application.update()?;
            application.render()?;
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
