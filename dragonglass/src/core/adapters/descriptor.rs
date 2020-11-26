use crate::core::Device;
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use std::sync::Arc;

pub struct DescriptorSetLayout {
    pub handle: vk::DescriptorSetLayout,
    device: Arc<Device>,
}

impl DescriptorSetLayout {
    pub fn new(
        device: Arc<Device>,
        create_info: vk::DescriptorSetLayoutCreateInfoBuilder,
    ) -> Result<Self> {
        let handle = unsafe {
            device
                .handle
                .create_descriptor_set_layout(&create_info, None)
        }?;
        let descriptor_set_layout = Self { handle, device };
        Ok(descriptor_set_layout)
    }
}

impl Drop for DescriptorSetLayout {
    fn drop(&mut self) {
        unsafe {
            self.device
                .handle
                .destroy_descriptor_set_layout(self.handle, None);
        }
    }
}

pub struct DescriptorPool {
    pub handle: vk::DescriptorPool,
    device: Arc<Device>,
}

impl DescriptorPool {
    pub fn new(
        device: Arc<Device>,
        create_info: vk::DescriptorPoolCreateInfoBuilder,
    ) -> Result<Self> {
        let handle = unsafe { device.handle.create_descriptor_pool(&create_info, None) }?;
        let descriptor_pool = Self { handle, device };
        Ok(descriptor_pool)
    }

    pub fn allocate_descriptor_sets(
        &self,
        layout: vk::DescriptorSetLayout,
        number_of_sets: u32,
    ) -> Result<Vec<vk::DescriptorSet>> {
        let layouts = (0..number_of_sets).map(|_| layout).collect::<Vec<_>>();
        let allocation_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(self.handle)
            .set_layouts(&layouts)
            .build();
        let descriptor_sets = unsafe {
            self.device
                .handle
                .allocate_descriptor_sets(&allocation_info)?
        };
        Ok(descriptor_sets)
    }
}

impl Drop for DescriptorPool {
    fn drop(&mut self) {
        unsafe {
            self.device
                .handle
                .destroy_descriptor_pool(self.handle, None);
        }
    }
}
