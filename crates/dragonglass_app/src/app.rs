use crate::{
    logger::create_logger,
    state::{Input, System},
};
use anyhow::Result;
use dragonglass_gui::{
    egui::{ClippedMesh, CtxRef},
    Gui, ScreenDescriptor,
};
use dragonglass_render::Renderer;
use dragonglass_world::{
    load_gltf, rapier3d::prelude::InteractionGroups, Entity, MouseRayConfiguration, SdfFont, World,
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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
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
    pub world: World,
    pub input: Input,
    pub system: System,
    pub renderer: Renderer,
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

    pub fn update(&mut self) -> Result<()> {
        self.world.tick(self.system.delta_time as f32)
    }

    pub fn render(&mut self, ui_meshes: &[ClippedMesh]) -> Result<()> {
        let physical_size = self.window.outer_size();
        let screen_descriptor = ScreenDescriptor {
            physical_width: physical_size.width,
            physical_height: physical_size.height,
            scale_factor: self.window.scale_factor() as _,
        };
        self.renderer.render(
            &self.system.window_dimensions,
            &self.world,
            ui_meshes,
            screen_descriptor,
        )
    }

    pub fn pick_object(
        &mut self,
        interact_distance: f32,
        groups: InteractionGroups,
    ) -> Result<Option<Entity>> {
        let aspect_ratio = self.system.aspect_ratio();

        let (projection, view) = self.world.active_camera_matrices(aspect_ratio)?;

        let mouse_ray_configuration = MouseRayConfiguration {
            viewport_width: self.system.window_dimensions[0] as _,
            viewport_height: self.system.window_dimensions[1] as _,
            projection_matrix: projection,
            view_matrix: view,
            mouse_position: self.input.mouse.position,
            invert_y: false,
        };

        self.world
            .pick_object(&mouse_ray_configuration, interact_distance, groups)
    }
}

pub trait ApplicationRunner {
    fn initialize(&mut self, _application: &mut Application) -> Result<()> {
        Ok(())
    }

    fn update_gui(&mut self, _context: CtxRef, _application: &mut Application) -> Result<()> {
        Ok(())
    }

    fn update_before_app(&mut self, _application: &mut Application) -> Result<()> {
        Ok(())
    }

    fn update_after_app(&mut self, _application: &mut Application) -> Result<()> {
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

    let screen_descriptor = ScreenDescriptor {
        physical_width: window_dimensions[0],
        physical_height: window_dimensions[1],
        scale_factor: window.scale_factor() as _,
    };
    let mut gui = Gui::new(screen_descriptor);

    let renderer = pollster::block_on(Renderer::new(&window, &window_dimensions, gui.context()))?;

    let mut world = World::new()?;

    world.fonts.insert(
        "default".to_string(),
        SdfFont::new("assets/fonts/font.fnt", "assets/fonts/font_sdf_rgba.png")?,
    );

    let mut application = Application {
        world,
        input: Input::default(),
        system: System::new(window_dimensions),
        renderer,
        window,
    };

    runner.initialize(&mut application)?;

    event_loop.run(move |event, _, control_flow| {
        if let Err(error) = run_loop(&mut runner, &mut application, &mut gui, event, control_flow) {
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

    gui.handle_event(&event);
    application.system.handle_event(&event);

    application.input.allowed = {
        let context = gui.context();
        let using_gui = context.wants_pointer_input()
            || context.wants_keyboard_input()
            || context.is_using_pointer();
        !using_gui
    };

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
            runner.update_before_app(application)?;
            application.update()?;
            runner.update_after_app(application)?;

            let _frame_data = gui.start_frame(application.window.scale_factor() as _);
            runner.update_gui(gui.context(), application)?;
            let ui_meshes = gui.end_frame(&application.window);

            application.render(&ui_meshes)?;
        }
        // FIXME window events can be grouped
        Event::WindowEvent {
            event: WindowEvent::Resized(physical_size),
            window_id,
        } if window_id == application.window.id() => {
            application
                .renderer
                .resize([physical_size.width, physical_size.height]);
        }
        Event::WindowEvent {
            event:
                WindowEvent::ScaleFactorChanged {
                    ref new_inner_size, ..
                },
            window_id,
        } if window_id == application.window.id() => {
            let size = **new_inner_size;
            application.renderer.resize([size.width, size.height]);
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
