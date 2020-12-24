use crate::vulkan::core::{
    create_swapchain, CommandPool, Context, Device, Fence, Semaphore, Swapchain,
    SwapchainProperties,
};
use anyhow::{bail, Context as AnyhowContext, Result};
use ash::{prelude::VkResult, version::DeviceV1_0, vk};
use std::sync::Arc;

pub struct Frame {
    index: usize,
    locks: Vec<FrameLock>,
    command_buffers: Vec<vk::CommandBuffer>,
    _command_pool: CommandPool,
    frames_in_flight: usize,
    swapchain: Option<Swapchain>,
    pub swapchain_properties: SwapchainProperties,
    pub recreated_swapchain: bool,
    context: Arc<Context>,
}

impl Frame {
    pub fn new(
        context: Arc<Context>,
        dimensions: &[u32; 2],
        frames_in_flight: usize,
    ) -> Result<Self> {
        let frame_locks = (0..frames_in_flight)
            .map(|_| FrameLock::new(context.device.clone()))
            .collect::<Result<Vec<_>>>()?;

        let graphics_queue_index = context.physical_device.graphics_queue_family_index;
        let command_pool = CommandPool::new(
            context.device.clone(),
            context.graphics_queue(),
            vk::CommandPoolCreateInfo::builder()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(graphics_queue_index),
        )?;

        let (swapchain, properties) = create_swapchain(&context, dimensions)?;
        let number_of_framebuffers = swapchain.images()?.len() as _;
        let command_buffers = command_pool
            .allocate_command_buffers(number_of_framebuffers, vk::CommandBufferLevel::PRIMARY)?;

        Ok(Self {
            index: 0,
            locks: frame_locks,
            command_buffers,
            _command_pool: command_pool,
            frames_in_flight,
            swapchain: Some(swapchain),
            recreated_swapchain: false,
            swapchain_properties: properties,
            context,
        })
    }

    pub fn swapchain(&self) -> Result<&Swapchain> {
        self.swapchain.as_ref().context("Failed to get swapchain!")
    }

    pub fn render(
        &mut self,
        dimensions: &[u32; 2],
        mut action: impl FnMut(vk::CommandBuffer, usize) -> Result<()>,
    ) -> Result<()> {
        self.recreated_swapchain = false;
        self.wait_for_in_flight_fence()?;
        if let Some(image_index) = self.acquire_next_frame(dimensions)? {
            self.reset_in_flight_fence()?;
            self.context.device.record_command_buffer(
                self.command_buffer_at(image_index)?,
                vk::CommandBufferUsageFlags::empty(),
                |command_buffer| action(command_buffer, image_index),
            )?;
            self.submit_command_buffer(image_index)?;
            let result = self.present_next_frame(image_index)?;
            self.check_presentation_result(result, dimensions)?;
            self.increment_frame_counter();
        }
        Ok(())
    }

    fn increment_frame_counter(&mut self) {
        self.index = (self.index + 1) % self.frames_in_flight;
    }

    fn reset_in_flight_fence(&self) -> Result<()> {
        let in_flight_fence = self.frame_lock()?.in_flight.handle;
        unsafe { self.context.device.handle.reset_fences(&[in_flight_fence]) }?;
        Ok(())
    }

    fn wait_for_in_flight_fence(&self) -> Result<()> {
        let fence = self.frame_lock()?.in_flight.handle;
        unsafe {
            self.context
                .device
                .handle
                .wait_for_fences(&[fence], true, std::u64::MAX)
        }?;
        Ok(())
    }

    fn acquire_next_frame(&mut self, dimensions: &[u32; 2]) -> Result<Option<usize>> {
        let result = self
            .swapchain()?
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
        let swapchains = [self.swapchain()?.handle_khr];
        let image_indices = [image_index as u32];

        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        let presentation_result = unsafe {
            self.swapchain()?
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
        if dimensions[0] == 0 || dimensions[1] == 0 {
            return Ok(());
        }

        unsafe { self.context.device.handle.device_wait_idle() }?;

        self.swapchain = None;
        let (swapchain, properties) = create_swapchain(&self.context, dimensions)?;
        self.swapchain = Some(swapchain);
        self.swapchain_properties = properties;

        self.recreated_swapchain = true;

        Ok(())
    }

    fn command_buffer_at(&self, image_index: usize) -> Result<vk::CommandBuffer> {
        let command_buffer = *self.command_buffers.get(image_index).context(format!(
            "No command buffer was found at image index: {}",
            image_index
        ))?;
        Ok(command_buffer)
    }

    fn frame_lock(&self) -> Result<&FrameLock> {
        let lock = &self.locks.get(self.index).context(format!(
            "No frame lock was found at frame index: {}",
            self.index
        ))?;
        Ok(lock)
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
            self.context.device.handle.queue_submit(
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
    pub fn new(device: Arc<Device>) -> Result<Self> {
        let handles = Self {
            image_available: Semaphore::new(device.clone())?,
            render_finished: Semaphore::new(device.clone())?,
            in_flight: Fence::new(device, vk::FenceCreateFlags::SIGNALED)?,
        };
        Ok(handles)
    }
}
