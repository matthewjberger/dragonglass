use crate::context::LogicalDevice;
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use std::sync::Arc;

pub struct RenderPass {
    pub handle: vk::RenderPass,
    device: Arc<LogicalDevice>,
}

impl RenderPass {
    pub fn new(device: Arc<LogicalDevice>, create_info: &vk::RenderPassCreateInfo) -> Result<Self> {
        let handle = unsafe { device.handle.create_render_pass(create_info, None) }?;
        let render_pass = Self { handle, device };
        Ok(render_pass)
    }

    pub fn record(
        &self,
        buffer: vk::CommandBuffer,
        begin_info: vk::RenderPassBeginInfoBuilder,
        mut action: impl Fn(vk::CommandBuffer) -> Result<()>,
    ) -> Result<()> {
        unsafe {
            self.device.handle.cmd_begin_render_pass(
                buffer,
                &begin_info,
                vk::SubpassContents::INLINE,
            )
        };

        action(buffer)?;

        unsafe {
            self.device.handle.cmd_end_render_pass(buffer);
        }

        Ok(())
    }
}

impl Drop for RenderPass {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_render_pass(self.handle, None);
        }
    }
}

pub struct Framebuffer {
    pub handle: vk::Framebuffer,
    device: Arc<LogicalDevice>,
}

impl Framebuffer {
    pub fn new(
        device: Arc<LogicalDevice>,
        create_info: vk::FramebufferCreateInfoBuilder,
    ) -> Result<Self> {
        let handle = unsafe { device.handle.create_framebuffer(&create_info, None) }?;
        let framebuffer = Self { handle, device };
        Ok(framebuffer)
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_framebuffer(self.handle, None);
        }
    }
}
