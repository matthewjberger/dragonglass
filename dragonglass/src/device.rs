use super::{
    core::{Context, LogicalDevice},
    forward::ForwardSwapchain,
    render::{CommandPool, Fence, RenderPass, Semaphore},
};
use anyhow::{anyhow, bail, Result};
use ash::{prelude::VkResult, version::DeviceV1_0, vk};
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;

pub struct RenderingDevice {
    frame: usize,
    frame_locks: Vec<FrameLock>,
    command_buffers: Vec<vk::CommandBuffer>,
    command_pool: CommandPool,
    forward_swapchain: Option<ForwardSwapchain>,
    context: Arc<Context>,
}

impl RenderingDevice {
    const MAX_FRAMES_IN_FLIGHT: usize = 2;

    pub fn new<T: HasRawWindowHandle>(window_handle: &T, dimensions: &[u32; 2]) -> Result<Self> {
        let context = Arc::new(Context::new(window_handle)?);
        let frame_locks = Self::create_frame_locks(context.logical_device.clone())?;
        let command_pool = Self::create_command_pool(
            context.logical_device.clone(),
            context.physical_device.graphics_queue_index,
        )?;
        let forward_swapchain = ForwardSwapchain::new(context.clone(), dimensions)?;
        let mut renderer = Self {
            frame: 0,
            frame_locks,
            command_buffers: Vec::new(),
            command_pool,
            forward_swapchain: Some(forward_swapchain),
            context,
        };
        renderer.allocate_command_buffers()?;
        Ok(renderer)
    }

    fn create_frame_locks(device: Arc<LogicalDevice>) -> Result<Vec<FrameLock>> {
        (0..Self::MAX_FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|_| FrameLock::new(device.clone()))
            .collect()
    }

    fn create_command_pool(device: Arc<LogicalDevice>, queue_index: u32) -> Result<CommandPool> {
        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_index)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = CommandPool::new(device, create_info)?;
        Ok(command_pool)
    }

    fn allocate_command_buffers(&mut self) -> Result<()> {
        let allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.command_pool.handle)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(self.forward_swapchain()?.framebuffers.len() as _);
        self.command_buffers = unsafe { self.device().allocate_command_buffers(&allocate_info) }?;
        Ok(())
    }

    fn forward_swapchain(&self) -> Result<&ForwardSwapchain> {
        self.forward_swapchain
            .as_ref()
            .ok_or(anyhow!("No forward swapchain was available for rendering!"))
    }

    fn device(&self) -> ash::Device {
        self.context.logical_device.handle.clone()
    }

    pub fn render(&mut self, dimensions: &[u32; 2]) -> Result<()> {
        self.wait_for_in_flight_fence()?;
        if let Some(image_index) = self.acquire_next_frame(dimensions)? {
            self.reset_in_flight_fence()?;
            self.record_command_buffer(image_index)?;
            self.submit_command_buffer(image_index)?;
            let result = self.present_next_frame(image_index)?;
            self.check_presentation_result(result, dimensions)?;
            self.frame = (1 + self.frame) % Self::MAX_FRAMES_IN_FLIGHT;
        }
        Ok(())
    }

    fn reset_in_flight_fence(&self) -> Result<()> {
        let in_flight_fence = self.frame_lock()?.in_flight.handle;
        unsafe { self.device().reset_fences(&[in_flight_fence]) }?;
        Ok(())
    }

    fn wait_for_in_flight_fence(&self) -> Result<()> {
        let fence = self.frame_lock()?.in_flight.handle;
        unsafe { self.device().wait_for_fences(&[fence], true, std::u64::MAX) }?;
        Ok(())
    }

    fn acquire_next_frame(&mut self, dimensions: &[u32; 2]) -> Result<Option<usize>> {
        let result = self
            .forward_swapchain()?
            .swapchain
            .acquire_next_image(self.frame_lock()?.image_available.handle, vk::Fence::null());

        match result {
            Ok((image_index, _)) => Ok(Some(image_index as usize)),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.create_swapchain(dimensions)?;
                Ok(None)
            }
            Err(error) => bail!(error),
        }
    }

    fn present_next_frame(&mut self, image_index: usize) -> Result<VkResult<bool>> {
        let wait_semaphores = [self.frame_lock()?.render_finished.handle];
        let swapchains = [self.forward_swapchain()?.swapchain.handle_khr];
        let image_indices = [image_index as u32];

        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        let presentation_result = unsafe {
            self.forward_swapchain()?
                .swapchain
                .handle_ash
                .queue_present(self.context.presentation_queue(), &present_info)
        };

        Ok(presentation_result)
    }

    fn check_presentation_result(
        &mut self,
        presentation_result: VkResult<bool>,
        dimensions: &[u32; 2],
    ) -> Result<()> {
        match presentation_result {
            Ok(is_suboptimal) if is_suboptimal => {
                self.create_swapchain(dimensions)?;
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.create_swapchain(dimensions)?;
            }
            Err(error) => bail!(error),
            _ => {}
        };
        Ok(())
    }

    fn create_swapchain(&mut self, dimensions: &[u32; 2]) -> Result<()> {
        if dimensions[0] <= 0 || dimensions[1] <= 0 {
            return Ok(());
        }

        unsafe { self.context.logical_device.handle.device_wait_idle() }?;

        self.forward_swapchain = None;
        self.forward_swapchain = Some(ForwardSwapchain::new(self.context.clone(), dimensions)?);

        Ok(())
    }

    fn record_command_buffer(&mut self, image_index: usize) -> Result<()> {
        let command_buffer = *self.command_buffers.get(image_index).ok_or(anyhow!(
            "No command buffer was found for the forward swapchain at image index: {}",
            image_index
        ))?;

        self.context.logical_device.clone().record_command_buffer(
            command_buffer,
            vk::CommandBufferUsageFlags::SIMULTANEOUS_USE,
            || {
                self.record_render_pass(command_buffer, image_index)?;
                Ok(())
            },
        )?;

        Ok(())
    }

    fn record_render_pass(
        &self,
        command_buffer: vk::CommandBuffer,
        image_index: usize,
    ) -> Result<()> {
        let clear_values = Self::clear_values();
        let begin_info = vk::RenderPassBeginInfo::builder()
            .render_pass(self.forward_swapchain()?.render_pass.handle)
            .framebuffer(self.framebuffer_at(image_index)?)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.forward_swapchain()?.swapchain_properties.extent,
            })
            .clear_values(&clear_values);

        RenderPass::record(
            self.context.logical_device.clone(),
            command_buffer,
            begin_info,
            || {
                // TODO: render stuff
                Ok(())
            },
        )?;

        Ok(())
    }

    fn framebuffer_at(&self, image_index: usize) -> Result<vk::Framebuffer> {
        let framebuffer = self
            .forward_swapchain()?
            .framebuffers
            .get(image_index)
            .ok_or(anyhow!(
                "No framebuffer was found for the forward swapchain at image index: {}",
                image_index
            ))?
            .handle;
        Ok(framebuffer)
    }

    fn command_buffer_at(&self, image_index: usize) -> Result<vk::CommandBuffer> {
        let command_buffer = *self.command_buffers.get(image_index).ok_or(anyhow!(
            "No command buffer was found at image index: {}",
            image_index
        ))?;
        Ok(command_buffer)
    }

    fn frame_lock(&self) -> Result<&FrameLock> {
        let lock = &self.frame_locks.get(self.frame).ok_or(anyhow!(
            "No frame lock was found at frame index: {}",
            self.frame,
        ))?;
        Ok(lock)
    }

    fn clear_values() -> Vec<vk::ClearValue> {
        let color = vk::ClearColorValue {
            float32: [0.39, 0.58, 0.93, 1.0], // Cornflower blue
        };
        let depth_stencil = vk::ClearDepthStencilValue {
            depth: 1.0,
            stencil: 0,
        };
        vec![vk::ClearValue { color }, vk::ClearValue { depth_stencil }]
    }

    fn submit_command_buffer(&self, image_index: usize) -> Result<()> {
        let lock = self.frame_lock()?;
        let image_available_semaphores = [lock.image_available.handle];
        let wait_semaphores = [lock.render_finished.handle];
        let command_buffers = [self.command_buffer_at(image_index)?];

        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(&image_available_semaphores)
            .wait_dst_stage_mask(&[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT])
            .command_buffers(&command_buffers)
            .signal_semaphores(&wait_semaphores);

        unsafe {
            self.context.logical_device.handle.queue_submit(
                self.context.graphics_queue(),
                &[submit_info.build()],
                lock.in_flight.handle,
            )
        }?;

        Ok(())
    }
}

pub struct FrameLock {
    pub image_available: Semaphore,
    pub render_finished: Semaphore,
    pub in_flight: Fence,
}

impl FrameLock {
    pub fn new(device: Arc<LogicalDevice>) -> Result<Self> {
        let handles = Self {
            image_available: Semaphore::new(device.clone())?,
            render_finished: Semaphore::new(device.clone())?,
            in_flight: Fence::new(device.clone(), vk::FenceCreateFlags::SIGNALED)?,
        };
        Ok(handles)
    }
}
