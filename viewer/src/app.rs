use crate::{camera::OrbitalCamera, gui::Gui, input::Input, settings::Settings, system::System};
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
    gui: Gui,
    camera: OrbitalCamera,
    _settings: Settings,
    input: Input,
    system: System,
    window: Window,
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

        let mut gui = Gui::new(&window);

        let logical_size = window.inner_size();
        let window_dimensions = [logical_size.width, logical_size.height];
        let rendering_device =
            RenderingDevice::new(&window, &window_dimensions, gui.context_mut())?;

        let app = Self {
            gui,
            camera: OrbitalCamera::default(),
            _settings: settings,
            input: Input::default(),
            system: System::new(window_dimensions),
            window,
            rendering_device,
            event_loop,
        };

        Ok(app)
    }

    pub fn run(self) -> Result<()> {
        let Self {
            mut gui,
            mut camera,
            mut input,
            mut system,
            mut rendering_device,
            window,
            event_loop,
            ..
        } = self;

        info!("Running viewer");
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            gui.handle_event(&event, &window);
            system.handle_event(&event);
            input.handle_event(&event, system.window_center());
            input.allowed = !gui.capturing_input();

            if input.is_key_pressed(VirtualKeyCode::Escape) || system.exit_requested {
                *control_flow = ControlFlow::Exit;
            }

            Self::update_camera(&mut camera, &input, &system);

            if let Event::MainEventsCleared = event {
                if let Err(error) = gui.render_frame(&window) {
                    error!("{}", error);
                }

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
        let scroll_multiplier = 0.01;
        camera.forward(input.mouse.wheel_delta.y * scroll_multiplier);
        if input.mouse.is_left_clicked {
            let rotation = input.mouse.position_delta * system.delta_time as f32;
            camera.rotate(&rotation);
        }
    }
}
