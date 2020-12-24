use crate::{camera::OrbitalCamera, gui::Gui, input::Input, logger::create_logger, system::System};
use anyhow::Result;
use dragonglass_render::{Backend, Renderer};
use dragonglass_world::{load_gltf, BoundingBoxVisible, Mesh, World};
use image::io::Reader;
use imgui::{im_str, Condition, DrawData, Ui};
use log::{error, info, warn};
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Icon, Window, WindowBuilder},
};

pub struct AppConfiguration {
    width: u32,
    height: u32,
    is_fullscreen: bool,
    title: String,
    icon: Option<String>,
}

impl Default for AppConfiguration {
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

impl AppConfiguration {
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

pub trait App {
    fn initialize(&mut self, _window: &mut Window, _world: &mut World) {}
    fn create_ui(&mut self, ui: &Ui, _world: &mut World) {
        ui.text(im_str!("Hello!"));
    }
    fn update(&mut self, _world: &mut World) {}
    fn cleanup(&mut self) {}
    fn on_key(&mut self, _state: ElementState, _keycode: VirtualKeyCode) {}
    fn handle_events(&mut self, _event: winit::event::Event<()>) {}
}

pub fn run_app(mut app: impl App + 'static, configuration: AppConfiguration) -> Result<()> {
    create_logger()?;

    let (event_loop, mut window) = configuration.create_window()?;
    let mut gui = Gui::new(&window);

    let logical_size = window.inner_size();
    let window_dimensions = [logical_size.width, logical_size.height];
    let mut renderer = Box::new(Renderer::create_backend(
        &Backend::Vulkan,
        &window,
        &window_dimensions,
        gui.context_mut(),
    )?);

    let mut input = Input::default();
    let mut system = System::new(window_dimensions);
    let mut world = World::new();

    app.initialize(&mut window, &mut world);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        system.handle_event(&event);
        gui.handle_event(&event, &window);
        input.handle_event(&event, system.window_center());
        input.allowed = !gui.capturing_input();

        match event {
            Event::MainEventsCleared => {
                if input.is_key_pressed(VirtualKeyCode::Escape) || system.exit_requested {
                    *control_flow = ControlFlow::Exit;
                }

                let draw_data = gui
                    .render_frame(&window, |ui| {
                        app.create_ui(ui, &mut world);
                    })
                    .expect("Failed to render gui frame!");

                app.update(&mut world);

                if let Err(error) = renderer.render(&system.window_dimensions, &world, draw_data) {
                    error!("{}", error);
                }
            }
            Event::WindowEvent {
                event:
                    WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                state,
                                virtual_keycode: Some(keycode),
                                ..
                            },
                        ..
                    },
                ..
            } => {
                app.on_key(state, keycode);
            }
            Event::LoopDestroyed => {
                app.cleanup();
            }
            _ => {}
        }

        app.handle_events(event);
    });
}
