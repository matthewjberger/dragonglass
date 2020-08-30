use super::core::LogicalDevice;
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use std::sync::Arc;

pub struct CommandPool {
    pub handle: vk::CommandPool,
    device: Arc<LogicalDevice>,
}

impl CommandPool {
    pub fn new(
        device: Arc<LogicalDevice>,
        create_info: vk::CommandPoolCreateInfoBuilder,
    ) -> Result<Self> {
        let handle = unsafe { device.handle.create_command_pool(&create_info, None)? };
        let command_pool = Self { handle, device };
        Ok(command_pool)
    }
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_command_pool(self.handle, None);
        }
    }
}
