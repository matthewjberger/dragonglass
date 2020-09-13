use crate::core::LogicalDevice;
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use log::error;
use std::sync::Arc;
use vk_mem::Allocator;

pub struct Image {
    pub handle: vk::Image,
    allocation: vk_mem::Allocation,
    _allocation_info: vk_mem::AllocationInfo,
    allocator: Arc<Allocator>,
}

impl Image {
    pub fn new(
        allocator: Arc<Allocator>,
        allocation_create_info: &vk_mem::AllocationCreateInfo,
        image_create_info: &vk::ImageCreateInfoBuilder,
    ) -> Result<Self> {
        let (handle, allocation, _allocation_info) =
            allocator.create_image(&image_create_info, &allocation_create_info)?;

        let texture = Self {
            handle,
            allocation,
            _allocation_info,
            allocator,
        };

        Ok(texture)
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        if let Err(error) = self.allocator.destroy_image(self.handle, &self.allocation) {
            error!("{}", error);
        }
    }
}

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
