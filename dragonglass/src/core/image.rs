use super::LogicalDevice;
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use std::sync::Arc;

pub struct ImageView {
    pub handle: vk::ImageView,
    device: Arc<LogicalDevice>,
}

impl ImageView {
    pub fn new(
        device: Arc<LogicalDevice>,
        create_info: vk::ImageViewCreateInfoBuilder,
    ) -> Result<Self> {
        let handle = unsafe { device.handle.create_image_view(&create_info, None) }?;
        let image_view = Self { handle, device };
        Ok(image_view)
    }
}

impl Drop for ImageView {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_image_view(self.handle, None);
        }
    }
}

pub struct Sampler {
    pub handle: vk::Sampler,
    device: Arc<LogicalDevice>,
}

impl Sampler {
    pub fn new(
        device: Arc<LogicalDevice>,
        create_info: vk::SamplerCreateInfoBuilder,
    ) -> Result<Self> {
        let handle = unsafe { device.handle.create_sampler(&create_info, None) }?;
        let sampler = Self { handle, device };
        Ok(sampler)
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        unsafe { self.device.handle.destroy_sampler(self.handle, None) };
    }
}

pub struct Framebuffer {
    pub handle: vk::Framebuffer,
    device: Arc<LogicalDevice>,
}

impl Framebuffer {
    pub fn new(device: Arc<LogicalDevice>, create_info: vk::FramebufferCreateInfo) -> Result<Self> {
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
