use crate::{
    camera::{fps_camera_controls_system, orbital_camera_controls_system, OrbitalCamera},
    input::Input,
    settings::Settings,
    system::System,
};
use anyhow::Result;
use dragonglass::RenderingDevice;
use legion::*;
use log::{error, info};
use winit::{
    dpi::PhysicalSize,
    event::{Event, VirtualKeyCode},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

pub struct App {
    _settings: Settings,
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

        let logical_size = window.inner_size();
        let window_dimensions = [logical_size.width, logical_size.height];
        let rendering_device = RenderingDevice::new(&window, &window_dimensions)?;

        let app = Self {
            _settings: settings,
            window,
            rendering_device,
            event_loop,
        };

        Ok(app)
    }

    pub fn run(self) -> Result<()> {
        let Self {
            mut rendering_device,
            event_loop,
            ..
        } = self;

        let logical_size = rendering_device.window.inner_size();

        let mut resources = Resources::default();
        resources.insert(Input::default());
        resources.insert(System::new([logical_size.width, logical_size.height]));

        let mut world = World::default();
        world.push((OrbitalCamera::default(),));
        let mut update_schedule = Schedule::builder()
            .add_system(fps_camera_controls_system())
            .add_system(orbital_camera_controls_system())
            .flush()
            .build();

        info!("Running viewer");
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            system.handle_event(&event);
            input.handle_event(&event, system.window_center());

            if input.is_key_pressed(VirtualKeyCode::Escape) || system.exit_requested {
                *control_flow = ControlFlow::Exit;
            }

            match event {
                Event::NewEvents { .. } => update_schedule.execute(&mut world, &mut resources),
                Event::MainEventsCleared => {
                    if let Err(error) = rendering_device.render(&system.window_dimensions) {
                        error!("{}", error);
                    }
                }
                _ => {}
            }
        });
    }
}
