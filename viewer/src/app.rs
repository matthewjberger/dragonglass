use crate::{input::Input, settings::Settings, system::System};
use anyhow::Result;
use dragonglass::Renderer;
use log::info;
use nalgebra_glm as glm;
use raw_window_handle::HasRawWindowHandle;
use winit::{
    dpi::PhysicalSize,
    event::{Event, VirtualKeyCode},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

pub struct App {
    _settings: Settings,
    input: Input,
    system: System,
    _window: Window,
    renderer: Renderer,
    event_loop: EventLoop<()>,
}

impl App {
    pub const TITLE: &'static str = "Dragonglass - GLTF Model Viewer";

    pub fn new() -> Result<Self> {
        let settings = Settings::load_current_settings()?;

        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_title(Self::TITLE)
            .with_inner_size(PhysicalSize::new(settings.width, settings.height))
            .build(&event_loop)?;

        let window_dimensions = glm::vec2(
            window.inner_size().width as _,
            window.inner_size().height as _,
        );

        let renderer = Renderer::new(&window.raw_window_handle())?;

        let app = Self {
            _settings: settings,
            input: Input::default(),
            system: System::new(window_dimensions),
            _window: window,
            renderer,
            event_loop,
        };

        Ok(app)
    }

    pub fn run(self) -> Result<()> {
        let Self {
            mut input,
            mut system,
            mut renderer,
            event_loop,
            ..
        } = self;

        renderer.initialize()?;

        info!("Running viewer");
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            system.handle_event(&event);
            input.handle_event(&event, system.window_center());

            if input.is_key_pressed(VirtualKeyCode::Escape) {
                *control_flow = ControlFlow::Exit;
            }

            if let Event::MainEventsCleared = event {
                renderer.render().expect("Failed to render a frame!");
            }
        });
    }
}
