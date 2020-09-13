use super::Fence;
use crate::core::LogicalDevice;
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use derive_builder::Builder;
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

    pub fn copy_buffer_to_buffer(&self, info: &BufferToBufferCopy) -> Result<()> {
        let device = self.device.handle.clone();
        self.execute_once(info.graphics_queue, |command_buffer| {
            unsafe {
                device.cmd_copy_buffer(command_buffer, info.source, info.destination, &info.regions)
            };
            Ok(())
        })
    }

    pub fn copy_buffer_to_image(&self, info: &BufferToImageCopy) -> Result<()> {
        let device = self.device.handle.clone();
        self.execute_once(info.graphics_queue, |command_buffer| {
            unsafe {
                device.cmd_copy_buffer_to_image(
                    command_buffer,
                    info.source,
                    info.destination,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &info.regions,
                )
            };
            Ok(())
        })
    }

    // pub fn transition_image_layout(
    //     &self,
    //     image_transition_info: ImageTransition,
    // ) -> Result<()> {
    //     self.execute_command_once(, |command_buffer| {
    //         unsafe {
    //             self.context
    //                 .logical_device()
    //                 .logical_device()
    //                 .cmd_pipeline_barrier(
    //                     command_buffer,
    //                     src_stage_mask,
    //                     dst_stage_mask,
    //                     vk::DependencyFlags::empty(),
    //                     &[],
    //                     &[],
    //                     &barriers,
    //                 )
    //         };
    //     })?;
    //     Ok(())
    // }

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
        )?;

        let submit_info = vk::SubmitInfo::builder()
            .command_buffers(&command_buffers)
            .build();
        let submit_info_arr = [submit_info];

        let fence = Fence::new(self.device.clone(), vk::FenceCreateFlags::empty())?;

        let device = self.device.handle.clone();
        unsafe {
            device.queue_submit(queue, &submit_info_arr, fence.handle)?;
            device.wait_for_fences(
                &[fence.handle],
                true,
                std::time::Duration::from_secs(100).as_nanos() as _,
            )?;
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

#[derive(Builder)]
pub struct BufferToBufferCopy {
    pub graphics_queue: vk::Queue,
    pub source: vk::Buffer,
    pub destination: vk::Buffer,
    pub regions: Vec<vk::BufferCopy>,
}

#[derive(Builder)]
pub struct BufferToImageCopy {
    pub graphics_queue: vk::Queue,
    pub source: vk::Buffer,
    pub destination: vk::Image,
    pub regions: Vec<vk::BufferImageCopy>,
}
