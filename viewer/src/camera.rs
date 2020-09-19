use nalgebra_glm as glm;

#[derive(Clone, Copy)]
pub struct OrbitalCamera {
    direction: glm::Vec2,
    r: f32,
}

impl OrbitalCamera {
    pub fn position(&self) -> glm::Vec3 {
        let direction = glm::vec3(
            self.direction.y.sin() * self.direction.x.sin(),
            self.direction.y.cos(),
            self.direction.y.sin() * self.direction.x.cos(),
        );
        direction * self.r
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
    }

    pub fn view_matrix(&self) -> glm::Mat4 {
        glm::look_at(
            &self.position(),
            &glm::vec3(0.0, 0.0, 0.0),
            &glm::vec3(0.0, 1.0, 0.0),
        )
    }
}

impl Default for OrbitalCamera {
    fn default() -> Self {
        Self {
            direction: glm::vec2(0_f32.to_radians(), 45_f32.to_radians()),
            r: 5.0,
        }
    }
}
