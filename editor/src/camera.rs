use dragonglass::app::{Input, System};
use nalgebra_glm as glm;

#[derive(Clone, Copy)]
pub struct OrbitalCamera {
    direction: glm::Vec2,
    r: f32,
    min: f32,
    max: f32,
    offset: glm::Vec3,
    world_up: glm::Vec3,
    pub scroll: f32,
    pub rotation: f32,
    pub drag: f32,
}

impl OrbitalCamera {
    pub fn pan(&mut self, offset: &glm::Vec2) {
        self.offset += self.right().normalize() * offset.x;
        self.offset += self.up().normalize() * offset.y;
    }

    pub fn up(&self) -> glm::Vec3 {
        self.right().cross(&self.direction())
    }

    pub fn right(&self) -> glm::Vec3 {
        self.direction().cross(&self.world_up).normalize()
    }

    pub fn direction(&self) -> glm::Vec3 {
        glm::vec3(
            self.direction.y.sin() * self.direction.x.sin(),
            self.direction.y.cos(),
            self.direction.y.sin() * self.direction.x.cos(),
        )
    }

    pub fn position(&self) -> glm::Vec3 {
        (self.direction() * self.r) + self.offset
    }

    pub fn rotate(&mut self, position_delta: &glm::Vec2) {
        self.direction.x -= position_delta.x;
        self.direction.y = glm::clamp_scalar(
            self.direction.y - position_delta.y,
            10.0_f32.to_radians(),
            170.0_f32.to_radians(),
        );
    }

    pub fn forward(&mut self, r: f32) {
        self.r -= r;
        if self.r < self.min {
            self.r = self.min;
        }
        if self.r > self.max {
            self.r = self.max;
        }
    }

    pub fn view_matrix(&self) -> glm::Mat4 {
        glm::look_at(&self.position(), &self.offset, &self.world_up)
    }

    pub fn update(&mut self, input: &Input, system: &System) {
        if !input.allowed {
            return;
        }

        self.forward(input.mouse.wheel_delta.y * self.scroll);

        if input.mouse.is_left_clicked {
            let delta = input.mouse.position_delta;
            let rotation = delta * self.rotation * system.delta_time as f32;
            self.rotate(&rotation);
        } else if input.mouse.is_right_clicked {
            let delta = input.mouse.position_delta;
            let pan = delta * self.drag;
            self.pan(&pan);
        }
    }
}

impl Default for OrbitalCamera {
    fn default() -> Self {
        Self {
            direction: glm::vec2(0_f32.to_radians(), 45_f32.to_radians()),
            r: 5.0,
            min: 1.0,
            max: 100.0,
            offset: glm::vec3(0.0, 0.0, 0.0),
            world_up: glm::vec3(0.0, 1.0, 0.0),
            scroll: 1.0,
            rotation: 0.05,
            drag: 0.001,
        }
    }
}