use crate::{camera::OrbitalCamera, input::Input, settings::Settings, system::System};
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
    camera: OrbitalCamera,
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
            camera: OrbitalCamera::default(),
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
            mut camera,
            mut input,
            mut system,
            mut rendering_device,
            event_loop,
            ..
        } = self;

        input.allowed = true;

        info!("Running viewer");
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            system.handle_event(&event);
            input.handle_event(&event, system.window_center());

            if input.is_key_pressed(VirtualKeyCode::Escape) || system.exit_requested {
                *control_flow = ControlFlow::Exit;
            }

            Self::update_camera(&mut camera, &input, &system);

            if let Event::MainEventsCleared = event {
                if let Err(error) = rendering_device.render(
                    &system.window_dimensions,
                    &camera.view_matrix(),
                    &camera.position(),
                ) {
                    error!("{}", error);
                }
            }
        });
    }

    fn update_camera(camera: &mut OrbitalCamera, input: &Input, system: &System) {
        if !input.allowed {
            return;
        }
        camera.forward(input.mouse.wheel_delta.y * 0.3);
        if input.mouse.is_left_clicked {
            let rotation = input.mouse.position_delta * system.delta_time as f32;
            camera.rotate(&rotation);
        }
    }
}
