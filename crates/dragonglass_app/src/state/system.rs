use nalgebra_glm as glm;
use std::{cmp, time::Instant};
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
};

pub struct System {
    pub window_dimensions: [u32; 2], // TODO: Change this to a glm::Vec2
    pub start_time: Instant,
    pub delta_time: f64,
    pub last_frame: Instant,
    pub exit_requested: bool,
}

impl System {
    pub fn new(window_dimensions: [u32; 2]) -> Self {
        let now = Instant::now();
        Self {
            start_time: now,
            last_frame: now,
            window_dimensions,
            delta_time: 0.01,
            exit_requested: false,
        }
    }

    pub fn milliseconds_since_start(&self) -> u32 {
        Instant::now().duration_since(self.start_time).as_millis() as u32
    }

    pub fn aspect_ratio(&self) -> f32 {
        let width = self.window_dimensions[0];
        let height = cmp::max(self.window_dimensions[1], 0);
        width as f32 / height as f32
    }

    pub fn window_center(&self) -> glm::Vec2 {
        glm::vec2(
            self.window_dimensions[0] as f32 / 2.0,
            self.window_dimensions[1] as f32 / 2.0,
        )
    }

    pub fn handle_event<T>(&mut self, event: &Event<T>) {
        match event {
            Event::NewEvents { .. } => {
                self.delta_time = (Instant::now().duration_since(self.last_frame).as_micros()
                    as f64)
                    / 1_000_000_f64;
                self.last_frame = Instant::now();
            }
            Event::WindowEvent { event, .. } => match *event {
                WindowEvent::CloseRequested => self.exit_requested = true,
                WindowEvent::Resized(PhysicalSize { width, height }) => {
                    self.window_dimensions = [width, height];
                }
                _ => {}
            },
            _ => {}
        }
    }
}
