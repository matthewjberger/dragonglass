use crate::core::LogicalDevice;
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use std::sync::Arc;

pub struct RenderPass {
    pub handle: vk::RenderPass,
    device: Arc<LogicalDevice>,
}

impl RenderPass {
    pub fn new(device: Arc<LogicalDevice>, create_info: &vk::RenderPassCreateInfo) -> Result<Self> {
        let handle = unsafe { device.handle.create_render_pass(&create_info, None) }?;
        let render_pass = Self { handle, device };
        Ok(render_pass)
    }

    pub fn record<T>(
        &self,
        command_buffer: vk::CommandBuffer,
        create_info: vk::RenderPassBeginInfoBuilder,
        mut action: T,
    ) where
        T: FnMut(),
    {
        let create_info = create_info.render_pass(self.handle);

        unsafe {
            self.device.handle.cmd_begin_render_pass(
                command_buffer,
                &create_info,
                vk::SubpassContents::INLINE,
            )
        };

        action();

        unsafe {
            self.device.handle.cmd_end_render_pass(command_buffer);
        }
    }
}

impl Drop for RenderPass {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_render_pass(self.handle, None);
        }
    }
}
