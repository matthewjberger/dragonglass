use super::core::LogicalDevice;
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use std::sync::Arc;

pub struct Fence {
    handle: vk::Fence,
    device: Arc<LogicalDevice>,
}

impl Fence {
    pub fn new(device: Arc<LogicalDevice>, flags: vk::FenceCreateFlags) -> Result<Self> {
        let create_info = vk::FenceCreateInfo::builder().flags(flags);
        let handle = unsafe { device.handle.create_fence(&create_info, None) }?;
        let fence = Self { handle, device };
        Ok(fence)
    }
}

impl Drop for Fence {
    fn drop(&mut self) {
        unsafe { self.device.handle.destroy_fence(self.handle, None) }
    }
}

pub struct Semaphore {
    pub handle: vk::Semaphore,
    device: Arc<LogicalDevice>,
}

impl Semaphore {
    pub fn new(device: Arc<LogicalDevice>) -> Result<Self> {
        let create_info = vk::SemaphoreCreateInfo::builder();
        let handle = unsafe { device.handle.create_semaphore(&create_info, None) }?;
        let semaphore = Self { handle, device };
        Ok(semaphore)
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        unsafe { self.device.handle.destroy_semaphore(self.handle, None) }
    }
}
