use anyhow::Result;
use dragonglass::{app::Application, world::Transform};
use nalgebra_glm as glm;

pub(crate) struct Arcball {
    direction: glm::Vec2,
    r: f32,
    min: f32,
    max: f32,
    pub offset: glm::Vec3,
    pub world_up: glm::Vec3,
}

impl Arcball {
    pub fn update(&mut self, application: &mut Application) -> Result<()> {
        let delta_time = application.system.delta_time as f32;
        self.forward(application.input.mouse.wheel_delta.y * 0.3);
        if application.input.mouse.is_left_clicked {
            self.rotate(&(application.input.mouse.position_delta * delta_time));
        }

        let camera_entity = application.world.active_camera(&mut application.ecs)?;
        let mut transform = application.ecs.get_mut::<Transform>(camera_entity)?;
        if application.input.mouse.is_right_clicked {
            self.pan(&(application.input.mouse.position_delta * delta_time))
        }

        transform.translation = self.position();
        transform.rotation = glm::quat_conjugate(&glm::quat_look_at(
            &(self.offset - self.position()),
            &self.world_up,
        ));

        Ok(())
    }

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
}

impl Default for Arcball {
    fn default() -> Self {
        Self {
            direction: glm::vec2(0_f32.to_radians(), 45_f32.to_radians()),
            r: 5.0,
            min: 1.0,
            max: 100.0,
            offset: glm::vec3(0.0, 0.0, 0.0),
            world_up: glm::vec3(0.0, 1.0, 0.0),
        }
    }
}
