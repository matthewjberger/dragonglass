use crate::{input::Input, settings::Settings, system::System};
use anyhow::Result;
use log::debug;
use nalgebra_glm as glm;
use winit::{
    dpi::PhysicalSize,
    event::VirtualKeyCode,
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

pub struct App {
    _settings: Settings,
    input: Input,
    system: System,
    _window: Window,
    event_loop: EventLoop<()>,
}

impl App {
    pub const TITLE: &'static str = "Dragonglass - GLTF Model Viewer";

    pub fn new() -> Result<Self> {
        let settings = Settings::load_current_settings()?;

        debug!("Running viewer");
        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_title(Self::TITLE)
            .with_inner_size(PhysicalSize::new(settings.width, settings.height))
            .build(&event_loop)?;

        let window_dimensions = glm::vec2(
            window.inner_size().width as _,
            window.inner_size().height as _,
        );
        let system = System::new(window_dimensions);
        let input = Input::default();

        let app = Self {
            _settings: settings,
            input,
            system,
            _window: window,
            event_loop,
        };

        Ok(app)
    }

    pub fn run(self) -> Result<()> {
        let Self {
            mut input,
            mut system,
            event_loop,
            ..
        } = self;

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            system.handle_event(&event);
            input.handle_event(&event, system.window_center());

            if input.is_key_pressed(VirtualKeyCode::Escape) {
                *control_flow = ControlFlow::Exit;
            }
        });
    }
}
