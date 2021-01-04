use anyhow::Result;
use dragonglass::{app::Application, world::Transform};
use nalgebra_glm as glm;

#[derive(Default)]
pub(crate) struct Arcball {
    pub offset: glm::Vec3,
}

impl Arcball {
    pub fn update(&mut self, application: &mut Application) -> Result<()> {
        let delta_time = application.system.delta_time as f32;
        let mouse_delta = application.input.mouse.position_delta;
        let mousewheel_delta = application.input.mouse.wheel_delta;

        let camera_entity = application.world.active_camera(&mut application.ecs)?;
        let mut transform = application.ecs.get_mut::<Transform>(camera_entity)?;
        let forward = transform.forward();
        let up = transform.up();
        let right = transform.right();

        if application.input.mouse.scrolled {
            let scroll_multiplier = 100.0;
            transform.translation += forward * scroll_multiplier * mousewheel_delta.y * delta_time;
        }

        if application.input.mouse.is_right_clicked {
            transform.translation -= right * mouse_delta.x * delta_time;
            transform.translation += up * mouse_delta.y * delta_time;
            self.offset -= right * mouse_delta.x * delta_time;
            self.offset += up * mouse_delta.y * delta_time;
        }

        if application.input.mouse.is_left_clicked {
            // Pitch
            let pitch_bound = 80_f32.to_radians();
            let pitch = glm::quat_euler_angles(&transform.rotation).z;
            let mut pitch_delta = -mouse_delta.y * delta_time;
            if pitch + pitch_delta > pitch_bound {
                pitch_delta = pitch_bound - pitch;
            }
            if pitch + pitch_delta < -pitch_bound {
                pitch_delta = -pitch_bound - pitch;
            }
            transform.translation =
                glm::rotate_vec3(&transform.translation, pitch_delta, &transform.right());

            // Yaw
            let yaw_bound = 80_f32.to_radians();
            let yaw = glm::quat_euler_angles(&transform.rotation).y;
            let mut yaw_delta = -mouse_delta.x * delta_time;
            if yaw + yaw_delta > yaw_bound {
                yaw_delta = yaw_bound - yaw;
            }
            if yaw + yaw_delta < -yaw_bound {
                yaw_delta = -yaw_bound - yaw;
            }
            transform.translation =
                glm::rotate_vec3(&transform.translation, yaw_delta, &glm::Vec3::y());

            // Foxus on target
            let target = -(transform.translation - self.offset);
            transform.look_at(&target, &glm::Vec3::y());
        }

        Ok(())
    }
}
