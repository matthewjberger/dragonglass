use nalgebra_glm as glm;
use std::{cmp, collections::HashMap, time::Instant};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{
        ElementState, Event, KeyboardInput, MouseButton, MouseScrollDelta, VirtualKeyCode,
        WindowEvent,
    },
};

pub type KeyMap = HashMap<VirtualKeyCode, ElementState>;

pub struct Input {
    pub keystates: KeyMap,
    pub mouse: Mouse,
    pub allowed: bool,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            keystates: KeyMap::default(),
            mouse: Mouse::default(),
            allowed: true,
        }
    }
}

impl Input {
    pub fn is_key_pressed(&self, keycode: VirtualKeyCode) -> bool {
        self.keystates.contains_key(&keycode) && self.keystates[&keycode] == ElementState::Pressed
    }

    pub fn handle_event<T>(&mut self, event: &Event<T>, window_center: glm::Vec2) {
        if let Event::WindowEvent { event, .. } = event {
            if let WindowEvent::KeyboardInput {
                input:
                    KeyboardInput {
                        virtual_keycode: Some(keycode),
                        state,
                        ..
                    },
                ..
            } = *event
            {
                *self.keystates.entry(keycode).or_insert(state) = state;
            }
        }

        self.mouse.handle_event(event, window_center);
    }
}

#[derive(Default)]
pub struct Mouse {
    pub is_left_clicked: bool,
    pub is_right_clicked: bool,
    pub position: glm::Vec2,
    pub position_delta: glm::Vec2,
    pub offset_from_center: glm::Vec2,
    pub wheel_delta: glm::Vec2,
    pub moved: bool,
    pub scrolled: bool,
}

impl Mouse {
    pub fn handle_event<T>(&mut self, event: &Event<T>, window_center: glm::Vec2) {
        match event {
            Event::NewEvents { .. } => self.new_events(),
            Event::WindowEvent { event, .. } => match *event {
                WindowEvent::MouseInput { button, state, .. } => self.mouse_input(button, state),
                WindowEvent::CursorMoved { position, .. } => {
                    self.cursor_moved(position, window_center)
                }
                WindowEvent::MouseWheel {
                    delta: MouseScrollDelta::LineDelta(h_lines, v_lines),
                    ..
                } => self.mouse_wheel(h_lines, v_lines),
                _ => {}
            },
            _ => {}
        }
    }

    fn new_events(&mut self) {
        if !self.scrolled {
            self.wheel_delta = glm::vec2(0.0, 0.0);
        }
        self.scrolled = false;

        if !self.moved {
            self.position_delta = glm::vec2(0.0, 0.0);
        }
        self.moved = false;
    }

    fn cursor_moved(&mut self, position: PhysicalPosition<f64>, window_center: glm::Vec2) {
        let last_position = self.position;
        let current_position = glm::vec2(position.x as _, position.y as _);
        self.position = current_position;
        self.position_delta = current_position - last_position;
        self.offset_from_center = window_center - glm::vec2(position.x as _, position.y as _);
        self.moved = true;
    }

    fn mouse_wheel(&mut self, h_lines: f32, v_lines: f32) {
        self.wheel_delta = glm::vec2(h_lines, v_lines);
        self.scrolled = true;
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState) {
        let clicked = state == ElementState::Pressed;
        match button {
            MouseButton::Left => self.is_left_clicked = clicked,
            MouseButton::Right => self.is_right_clicked = clicked,
            _ => {}
        }
    }
}

pub struct System {
    pub window_dimensions: PhysicalSize<u32>,
    pub delta_time: f64,
    pub last_frame: Instant,
    pub exit_requested: bool,
}

impl System {
    pub fn new(window_dimensions: PhysicalSize<u32>) -> Self {
        Self {
            last_frame: Instant::now(),
            window_dimensions,
            delta_time: 0.01,
            exit_requested: false,
        }
    }

    pub fn aspect_ratio(&self) -> f32 {
        let width = self.window_dimensions.width;
        let height = cmp::max(self.window_dimensions.height as u32, 0);
        width as f32 / height as f32
    }

    pub fn window_center(&self) -> glm::Vec2 {
        glm::vec2(
            self.window_dimensions.width as f32 / 2.0,
            self.window_dimensions.height as f32 / 2.0,
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
                WindowEvent::Resized(dimensions) => {
                    self.window_dimensions = dimensions;
                }
                _ => {}
            },
            _ => {}
        }
    }
}
