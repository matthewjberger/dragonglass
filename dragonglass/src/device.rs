use crate::{context::Context, forward::RenderPath, swapchain::Swapchain};
use anyhow::Result;
use ash::version::DeviceV1_0;
use log::error;
use nalgebra_glm as glm;
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;

pub struct RenderingDevice {
    swapchain: Swapchain,
    render_path: Option<RenderPath>,
    context: Arc<Context>,
}

impl RenderingDevice {
    const MAX_FRAMES_IN_FLIGHT: usize = 2;

    pub fn new<T: HasRawWindowHandle>(window_handle: &T, dimensions: &[u32; 2]) -> Result<Self> {
        let context = Arc::new(Context::new(window_handle)?);
        let swapchain = Swapchain::new(context.clone(), dimensions, Self::MAX_FRAMES_IN_FLIGHT)?;
        let render_path = RenderPath::new(&context, &swapchain)?;
        let renderer = Self {
            swapchain,
            render_path: Some(render_path),
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
        let Self {
            swapchain,
            render_path,
            ..
        } = self;

        let device = self.context.logical_device.clone();
        swapchain.render_frame(
            dimensions,
            |properties| {
                if let Some(render_path) = render_path.as_ref() {
                    let aspect_ratio = properties.aspect_ratio();
                    render_path.scene.borrow().update_ubo(aspect_ratio, *view)?;
                }
                Ok(())
            },
            |command_buffer, image_index| {
                if let Some(render_path) = render_path.as_ref() {
                    render_path.rendergraph.execute_at_index(
                        device.clone(),
                        command_buffer,
                        image_index,
                    )?;
                }
                Ok(())
            },
        )?;

        if swapchain.recreated_swapchain {
            self.render_path = None;
            self.render_path = Some(RenderPath::new(&self.context, swapchain)?);
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
