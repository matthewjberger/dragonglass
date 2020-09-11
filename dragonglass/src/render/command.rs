use crate::{
    core::LogicalDevice,
    render::{Buffer, Fence},
};
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use std::{mem, sync::Arc};
use vk_mem::Allocator;

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
            .level(level)
            .command_buffer_count(count);
        let command_buffers =
            unsafe { self.device.handle.allocate_command_buffers(&allocate_info) }?;
        Ok(command_buffers)
    }

    pub fn copy_buffer_to_buffer(
        &self,
        graphics_queue: vk::Queue,
        source: vk::Buffer,
        destination: vk::Buffer,
        regions: &[vk::BufferCopy],
    ) -> Result<()> {
        let device = self.device.handle.clone();
        self.execute_once(graphics_queue, |command_buffer| {
            unsafe { device.cmd_copy_buffer(command_buffer, source, destination, &regions) };
            Ok(())
        })
    }

    pub fn execute_once<T>(&self, queue: vk::Queue, mut executor: T) -> Result<()>
    where
        T: FnMut(vk::CommandBuffer) -> Result<()>,
    {
        let command_buffer = self.allocate_command_buffers(1, vk::CommandBufferLevel::PRIMARY)?[0];
        let command_buffers = [command_buffer];

        self.device.record_command_buffer(
            command_buffer,
            vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
            || executor(command_buffer),
        );

        let submit_info = vk::SubmitInfo::builder()
            .command_buffers(&command_buffers)
            .build();
        let submit_info_arr = [submit_info];

        let fence = Fence::new(self.device.clone(), vk::FenceCreateFlags::empty())?;

        let device = self.device.handle.clone();
        unsafe {
            device.queue_submit(queue, &submit_info_arr, fence.handle)?;
            device.wait_for_fences(&[fence.handle], true, 100_000_000_000)?;
            device.queue_wait_idle(queue)?;
            device.free_command_buffers(self.handle, &command_buffers);
        }

        Ok(())
    }
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_command_pool(self.handle, None);
        }
    }
}

impl crate::core::Context {
    pub fn create_buffer<T: Copy>(
        &self,
        pool: &CommandPool,
        data: &[T],
        usage: vk::BufferUsageFlags,
    ) -> Result<Buffer> {
        let staging_buffer = Buffer::staging_buffer(self.allocator.clone(), data)?;
        let device_local_buffer =
            Buffer::device_local_buffer(self.allocator.clone(), &staging_buffer, usage)?;
        let size = data.len() * mem::size_of::<T>();
        let region = vk::BufferCopy::builder().size(size as _);
        pool.copy_buffer_to_buffer(
            self.graphics_queue(),
            staging_buffer.handle,
            device_local_buffer.handle,
            &[region.build()],
        )?;
        Ok(device_local_buffer)
    }
}
