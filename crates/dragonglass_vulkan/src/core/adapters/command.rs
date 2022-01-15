use crate::core::{Device, Fence};
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use derive_builder::Builder;
use std::sync::Arc;

pub struct CommandPool {
    pub handle: vk::CommandPool,
    queue: vk::Queue,
    device: Arc<Device>,
}

impl CommandPool {
    pub fn new(
        device: Arc<Device>,
        queue: vk::Queue,
        create_info: vk::CommandPoolCreateInfoBuilder,
    ) -> Result<Self> {
        // TODO: Construct create info in this method using just a Queue wrapper that has an index and the queue handle
        let handle = unsafe { device.handle.create_command_pool(&create_info, None)? };
        let command_pool = Self {
            handle,
            queue,
            device,
        };
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
        self.execute_once(|command_buffer| {
            unsafe {
                device.cmd_copy_buffer(command_buffer, info.source, info.destination, &info.regions)
            };
            Ok(())
        })
    }

    pub fn copy_buffer_to_image(&self, info: &BufferToImageCopy) -> Result<()> {
        let device = self.device.handle.clone();
        self.execute_once(|command_buffer| {
            unsafe {
                device.cmd_copy_buffer_to_image(
                    command_buffer,
                    info.source,
                    info.destination,
                    info.dst_image_layout,
                    &info.regions,
                )
            };
            Ok(())
        })
    }

    pub fn copy_image_to_image(&self, info: &ImageToImageCopy) -> Result<()> {
        let device = self.device.handle.clone();
        self.execute_once(|command_buffer| {
            unsafe {
                device.cmd_copy_image(
                    command_buffer,
                    info.source,
                    info.source_layout,
                    info.destination,
                    info.destination_layout,
                    &info.regions,
                )
            };
            Ok(())
        })
    }

    pub fn transition_image_layout(&self, info: &PipelineBarrier) -> Result<()> {
        let device = self.device.handle.clone();
        self.execute_once(|command_buffer| {
            unsafe {
                device.cmd_pipeline_barrier(
                    command_buffer,
                    info.src_stage_mask,
                    info.dst_stage_mask,
                    info.dependency_flags,
                    &info.memory_barriers,
                    &info.buffer_memory_barriers,
                    &info.image_memory_barriers,
                )
            };
            Ok(())
        })
    }

    pub fn blit_image(&self, info: &BlitImage) -> Result<()> {
        self.execute_once(|command_buffer| {
            unsafe {
                self.device.handle.cmd_blit_image(
                    command_buffer,
                    info.src_image,
                    info.src_image_layout,
                    info.dst_image,
                    info.dst_image_layout,
                    &info.regions,
                    info.filter,
                )
            }
            Ok(())
        })
    }

    pub fn execute_once(
        &self,
        executor: impl FnMut(vk::CommandBuffer) -> Result<()>,
    ) -> Result<()> {
        let command_buffer = self.allocate_command_buffers(1, vk::CommandBufferLevel::PRIMARY)?[0];
        let command_buffers = [command_buffer];

        self.device.record_command_buffer(
            command_buffer,
            vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
            executor,
        )?;

        let submit_info = vk::SubmitInfo::builder()
            .command_buffers(&command_buffers)
            .build();
        let submit_info_arr = [submit_info];

        let fence = Fence::new(self.device.clone(), vk::FenceCreateFlags::empty())?;

        let device = self.device.handle.clone();
        unsafe {
            device.queue_submit(self.queue, &submit_info_arr, fence.handle)?;
            device.wait_for_fences(
                &[fence.handle],
                true,
                std::time::Duration::from_secs(100).as_nanos() as _,
            )?;
            device.queue_wait_idle(self.queue)?;
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
    pub source: vk::Buffer,
    pub destination: vk::Buffer,
    pub regions: Vec<vk::BufferCopy>,
}

#[derive(Builder)]
pub struct BufferToImageCopy {
    pub source: vk::Buffer,
    pub destination: vk::Image,
    pub regions: Vec<vk::BufferImageCopy>,
    #[builder(default = "vk::ImageLayout::TRANSFER_DST_OPTIMAL")]
    pub dst_image_layout: vk::ImageLayout,
}

#[derive(Builder)]
pub struct ImageToImageCopy {
    pub source: vk::Image,
    pub source_layout: vk::ImageLayout,
    pub destination: vk::Image,
    pub destination_layout: vk::ImageLayout,
    pub regions: Vec<vk::ImageCopy>,
}

#[derive(Builder)]
pub struct PipelineBarrier {
    pub src_stage_mask: vk::PipelineStageFlags,
    pub dst_stage_mask: vk::PipelineStageFlags,
    #[builder(default)]
    pub dependency_flags: vk::DependencyFlags,
    #[builder(default)]
    pub memory_barriers: Vec<vk::MemoryBarrier>,
    #[builder(default)]
    pub buffer_memory_barriers: Vec<vk::BufferMemoryBarrier>,
    pub image_memory_barriers: Vec<vk::ImageMemoryBarrier>,
}

#[derive(Builder)]
pub struct BlitImage {
    pub src_image: vk::Image,
    pub src_image_layout: vk::ImageLayout,
    pub dst_image: vk::Image,
    pub dst_image_layout: vk::ImageLayout,
    pub regions: Vec<vk::ImageBlit>,
    pub filter: vk::Filter,
}
