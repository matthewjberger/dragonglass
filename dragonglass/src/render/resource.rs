use crate::core::LogicalDevice;
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use log::error;
use std::sync::Arc;
use vk_mem::Allocator;

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

    pub fn allocate_command_buffers(
        &self,
        count: u32,
        level: vk::CommandBufferLevel,
    ) -> Result<Vec<vk::CommandBuffer>> {
        let allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.handle)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(count);
        let command_buffers =
            unsafe { self.device.handle.allocate_command_buffers(&allocate_info) }?;
        Ok(command_buffers)
    }
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_command_pool(self.handle, None);
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
