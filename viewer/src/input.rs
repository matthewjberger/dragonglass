use nalgebra_glm as glm;
use std::collections::HashMap;
use winit::event::{
    ElementState, Event, KeyboardInput, MouseButton, MouseScrollDelta, VirtualKeyCode, WindowEvent,
};

pub type KeyMap = HashMap<VirtualKeyCode, ElementState>;

#[derive(Default)]
pub struct Input {
    pub keystates: KeyMap,
    pub mouse: Mouse,
    pub allowed: bool,
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

        self.mouse.handle_event(&event, window_center);
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
            Event::NewEvents { .. } => {
                if !self.scrolled {
                    self.wheel_delta = glm::vec2(0.0, 0.0);
                }
                self.scrolled = false;

                if !self.moved {
                    self.position_delta = glm::vec2(0.0, 0.0);
                }
                self.moved = false;
            }
            Event::WindowEvent { event, .. } => match *event {
                WindowEvent::MouseInput { button, state, .. } => {
                    let clicked = state == ElementState::Pressed;
                    match button {
                        MouseButton::Left => self.is_left_clicked = clicked,
                        MouseButton::Right => self.is_right_clicked = clicked,
                        _ => {}
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    let last_position = self.position;
                    let current_position = glm::vec2(position.x as _, position.y as _);
                    self.position = current_position;
                    self.position_delta = current_position - last_position;
                    self.offset_from_center =
                        window_center - glm::vec2(position.x as _, position.y as _);
                    self.moved = true;
                }
                WindowEvent::MouseWheel {
                    delta: MouseScrollDelta::LineDelta(h_lines, v_lines),
                    ..
                } => {
                    self.wheel_delta = glm::vec2(h_lines, v_lines);
                    self.scrolled = true;
                }
                _ => {}
            },
            _ => {}
        }
    }
}
