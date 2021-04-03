use crate::app::Application;
use anyhow::Result;
use dragonglass_world::{Entity, Transform};
use nalgebra_glm as glm;

// FIXME: Separate this into two cameras
pub struct Camera {
    pub direction: glm::Vec2,
    pub r: f32,
    pub min: f32,
    pub max: f32,
    pub offset: glm::Vec3,
    pub world_up: glm::Vec3,
    pub use_fps: bool,
    pub sensitivity: glm::Vec2,
}

impl Camera {
    pub fn update(&mut self, application: &mut Application, entity: Entity) -> Result<()> {
        self.forward(application.input.mouse.wheel_delta.y * 0.3);

        let mouse_delta = if self.use_fps {
            application.input.mouse.offset_from_center
        } else {
            application.input.mouse.position_delta
        };
        let delta_time = application.system.delta_time as f32;
        let mouse_delta = mouse_delta.component_mul(&self.sensitivity) * delta_time;

        if application.input.mouse.is_left_clicked || self.use_fps {
            self.rotate(&mouse_delta);
        }

        {
            let mut transform = application.ecs.get_mut::<Transform>(entity)?;
            if application.input.mouse.is_right_clicked && !self.use_fps {
                self.pan(&mouse_delta)
            }

            transform.translation = self.position();
            if self.use_fps {
                transform.rotation = glm::quat_conjugate(&glm::quat_look_at(
                    &(self.position() - self.direction()),
                    &self.world_up,
                ));
            } else {
                transform.rotation = glm::quat_conjugate(&glm::quat_look_at(
                    &(self.offset - self.position()),
                    &self.world_up,
                ));
            }
        }

        application.set_cursor_grab(self.use_fps)?;
        application.set_cursor_visible(!self.use_fps);
        if self.use_fps {
            application.center_cursor()?;
        }

        Ok(())
    }

    pub fn pan(&mut self, offset: &glm::Vec2) {
        self.offset += self.right() * offset.x;
        self.offset += self.up() * offset.y;
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
        self.direction.x += position_delta.x;
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

impl Default for Camera {
    fn default() -> Self {
        Self {
            direction: glm::vec2(0_f32.to_radians(), 45_f32.to_radians()),
            r: 5.0,
            min: 1.0,
            max: 100.0,
            offset: glm::vec3(0.0, 0.0, 0.0),
            world_up: glm::Vec3::y(),
            use_fps: false,
            sensitivity: glm::vec2(1.0, 1.0),
        }
    }
}
