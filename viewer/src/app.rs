use anyhow::Result;
use winit::{
    dpi::PhysicalSize,
    event::Event,
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

#[derive(Default)]
pub struct App;

impl App {
    pub const TITLE: &'static str = "Dragonglass - GLTF Model Viewer";

    pub fn run() -> Result<()> {
        let event_loop = EventLoop::new();
        let _window = WindowBuilder::new()
            .with_title(Self::TITLE)
            .with_inner_size(PhysicalSize::new(1920, 1080))
            .build(&event_loop)?;

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            match event {
                Event::NewEvents { .. } => {
                }
                Event::MainEventsCleared => {
                }
                _ => {}
            }
        });
    }
}