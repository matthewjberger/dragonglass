use anyhow::{Context as AnyhowContext, Result};
use dragonglass::{app::Application, world::Transform};
use nalgebra_glm as glm;

#[derive(Default)]
pub(crate) struct Arcball {
    pub offset: glm::Vec3, // TODO: this needs to track the arcball target
}

impl Arcball {
    pub fn update(&mut self, application: &mut Application) -> Result<()> {
        let delta_time = application.system.delta_time as f32;
        let mouse_delta = application.input.mouse.position_delta;
        let mousewheel_delta = application.input.mouse.wheel_delta;

        let camera_entity = application.world.active_camera(&mut application.ecs)?;

        let mut entry = application
            .ecs
            .entry(camera_entity)
            .context("Failed to lookup an entity!")?;
        let mut transform = entry.get_component_mut::<Transform>()?;

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
        }

        if application.input.mouse.is_left_clicked {
            let yaw_delta = -mouse_delta.x * delta_time;
            transform.translation =
                glm::rotate_vec3(&transform.translation, yaw_delta, &glm::Vec3::y());

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

            let target = -transform.translation;
            transform.look_at(&target, &glm::Vec3::y());
        }

        Ok(())
    }
}
