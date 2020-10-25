use crate::{context::Context, frame::Frame, scene::Scene};
use anyhow::Result;
use ash::version::DeviceV1_0;
use log::error;
use nalgebra_glm as glm;
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;

pub struct RenderingDevice {
    frame: Frame,
    scene: Option<Scene>,
    context: Arc<Context>,
}

impl RenderingDevice {
    const MAX_FRAMES_IN_FLIGHT: usize = 2;

    pub fn new<T: HasRawWindowHandle>(window_handle: &T, dimensions: &[u32; 2]) -> Result<Self> {
        let context = Arc::new(Context::new(window_handle)?);
        let frame = Frame::new(context.clone(), dimensions, Self::MAX_FRAMES_IN_FLIGHT)?;
        let scene = Some(Scene::new(
            &context,
            frame.swapchain()?,
            &frame.swapchain_properties,
        )?);
        let renderer = Self {
            frame,
            scene,
            context,
        };
        Ok(renderer)
    }

    pub fn render(
        &mut self,
        dimensions: &[u32; 2],
        view: &glm::Mat4, // TODO: Turn these into a camera trait
        _camera_position: &glm::Vec3,
    ) -> Result<()> {
        let Self { frame, scene, .. } = self;

        let aspect_ratio = frame.swapchain_properties.aspect_ratio();
        let device = self.context.logical_device.clone();

        frame.render(dimensions, |command_buffer, image_index| {
            if let Some(scene) = scene.as_ref() {
                scene.object.borrow().update_ubo(aspect_ratio, *view)?;
                scene
                    .rendergraph
                    .execute_at_index(device.clone(), command_buffer, image_index)?;
            }
            Ok(())
        })?;

        if frame.recreated_swapchain {
            self.scene = None;
            self.scene = Some(Scene::new(
                &self.context,
                frame.swapchain()?,
                &frame.swapchain_properties,
            )?);
        }

        Ok(())
    }
}

impl Drop for RenderingDevice {
    fn drop(&mut self) {
        unsafe {
            if let Err(error) = self.context.logical_device.handle.device_wait_idle() {
                error!("{}", error);
            }
        }
    }
}
