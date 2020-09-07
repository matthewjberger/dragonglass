use crate::{input::Input, settings::Settings, system::System};
use anyhow::Result;
use dragonglass::RenderingDevice;
use log::{error, info};
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
    rendering_device: RenderingDevice,
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

        let logical_size = window.inner_size();
        let window_dimensions = [logical_size.width, logical_size.height];
        let rendering_device = RenderingDevice::new(&window, &window_dimensions)?;

        let app = Self {
            _settings: settings,
            input: Input::default(),
            system: System::new(window_dimensions),
            _window: window,
            rendering_device,
            event_loop,
        };

        Ok(app)
    }

    pub fn run(self) -> Result<()> {
        let Self {
            mut input,
            mut system,
            mut rendering_device,
            event_loop,
            ..
        } = self;

        info!("Running viewer");
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            system.handle_event(&event);
            input.handle_event(&event, system.window_center());

            if input.is_key_pressed(VirtualKeyCode::Escape) || system.exit_requested {
                *control_flow = ControlFlow::Exit;
            }

            if let Event::MainEventsCleared = event {
                if let Err(error) = rendering_device.render(&system.window_dimensions) {
                    error!("{}", error);
                }
            }
        });
    }
}
