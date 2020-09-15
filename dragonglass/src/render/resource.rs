use crate::core::LogicalDevice;
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use std::sync::Arc;

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

    pub fn record<T>(
        device: Arc<LogicalDevice>,
        buffer: vk::CommandBuffer,
        begin_info: vk::RenderPassBeginInfoBuilder,
        mut action: T,
    ) -> Result<()>
    where
        T: FnMut() -> Result<()>,
    {
        unsafe {
            device
                .handle
                .cmd_begin_render_pass(buffer, &begin_info, vk::SubpassContents::INLINE)
        };

        action()?;

        unsafe {
            device.handle.cmd_end_render_pass(buffer);
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

pub struct Fence {
    pub handle: vk::Fence,
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

pub struct DescriptorSetLayout {
    pub handle: vk::DescriptorSetLayout,
    device: Arc<LogicalDevice>,
}

impl DescriptorSetLayout {
    pub fn new(
        device: Arc<LogicalDevice>,
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
    device: Arc<LogicalDevice>,
}

impl DescriptorPool {
    pub fn new(
        device: Arc<LogicalDevice>,
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
