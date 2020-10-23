use super::forward::RenderPath;
use crate::{
    adapters::{CommandPool, Fence, Semaphore},
    context::{Context, LogicalDevice},
    swapchain::{create_swapchain, Swapchain, SwapchainProperties},
};
use anyhow::{bail, Context as AnyhowContext, Result};
use ash::{prelude::VkResult, version::DeviceV1_0, vk};
use log::error;
use nalgebra_glm as glm;
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;

pub struct RenderingDevice {
    frame: usize,
    frame_locks: Vec<FrameLock>,
    command_buffers: Vec<vk::CommandBuffer>,
    _command_pool: CommandPool,
    render_path: Option<RenderPath>,
    context: Context,
}

impl RenderingDevice {
    const MAX_FRAMES_IN_FLIGHT: usize = 2;

    pub fn new<T: HasRawWindowHandle>(window_handle: &T, dimensions: &[u32; 2]) -> Result<Self> {
        let context = Context::new(window_handle)?;
        let frame_locks = Self::frame_locks(context.logical_device.clone())?;
        let graphics_queue_index = context.physical_device.graphics_queue_index;
        let command_pool =
            Self::command_pool(context.logical_device.clone(), graphics_queue_index)?;
        let render_path = RenderPath::new(&context, dimensions)?;
        let number_of_framebuffers = render_path.swapchain.images()?.len() as _;
        let command_buffers = command_pool
            .allocate_command_buffers(number_of_framebuffers, vk::CommandBufferLevel::PRIMARY)?;

        let renderer = Self {
            frame: 0,
            frame_locks,
            command_buffers,
            _command_pool: command_pool,
            render_path: Some(render_path),
            context,
        };
        Ok(renderer)
    }

    fn frame_locks(device: Arc<LogicalDevice>) -> Result<Vec<FrameLock>> {
        (0..Self::MAX_FRAMES_IN_FLIGHT)
            .map(|_| FrameLock::new(device.clone()))
            .collect()
    }

    fn command_pool(device: Arc<LogicalDevice>, queue_index: u32) -> Result<CommandPool> {
        let create_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_index);
        let command_pool = CommandPool::new(device, create_info)?;
        Ok(command_pool)
    }

    fn render_path(&self) -> Result<&RenderPath> {
        self.render_path
            .as_ref()
            .context("No render path was available!")
    }

    fn device(&self) -> ash::Device {
        self.context.logical_device.handle.clone()
    }

    pub fn render(
        &mut self,
        dimensions: &[u32; 2],
        // TODO: Turn these into a camera trait
        view: &glm::Mat4,
        _camera_position: &glm::Vec3,
    ) -> Result<()> {
        self.wait_for_in_flight_fence()?;
        if let Some(image_index) = self.acquire_next_frame(dimensions)? {
            self.reset_in_flight_fence()?;
            if let Some(render_path) = self.render_path.as_ref() {
                let aspect_ratio = self.render_path()?.swapchain_properties.aspect_ratio();
                render_path.scene.borrow().update_ubo(aspect_ratio, *view)?;
            }
            self.record_command_buffer(image_index)?;
            self.submit_command_buffer(image_index)?;
            let result = self.present_next_frame(image_index)?;
            self.check_presentation_result(result, dimensions)?;
            self.increment_frame_counter();
        }
        Ok(())
    }

    fn increment_frame_counter(&mut self) {
        self.frame = (self.frame + 1) % Self::MAX_FRAMES_IN_FLIGHT;
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
            .render_path()?
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
        let swapchains = [self.render_path()?.swapchain.handle_khr];
        let image_indices = [image_index as u32];

        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        let presentation_result = unsafe {
            self.render_path()?
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
        if dimensions[0] == 0 || dimensions[1] == 0 {
            return Ok(());
        }

        unsafe { self.context.logical_device.handle.device_wait_idle() }?;

        self.render_path = None;
        self.render_path = Some(RenderPath::new(&self.context, dimensions)?);
        Ok(())
    }

    fn record_command_buffer(&mut self, image_index: usize) -> Result<()> {
        let command_buffer = self.command_buffer_at(image_index)?;
        self.context.logical_device.record_command_buffer(
            command_buffer,
            vk::CommandBufferUsageFlags::empty(),
            || {
                self.render_path()?.rendergraph.execute_at_index(
                    self.context.logical_device.clone(),
                    command_buffer,
                    image_index,
                )
            },
        )?;
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
        let lock = &self.frame_locks.get(self.frame).context(format!(
            "No frame lock was found at frame index: {}",
            self.frame
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
            self.context.logical_device.handle.queue_submit(
                self.context.graphics_queue(),
                &[submit_info.build()],
                lock.in_flight.handle,
            )
        }?;

        Ok(())
    }
}

impl Drop for RenderingDevice {
    fn drop(&mut self) {
        unsafe {
            if let Err(error) = self.context.logical_device.handle.device_wait_idle() {
                error!("{}", error);
            }
        }
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
            in_flight: Fence::new(device, vk::FenceCreateFlags::SIGNALED)?,
        };
        Ok(handles)
    }
}

pub struct SwapchainCycle {
    frame: usize,
    frame_locks: Vec<FrameLock>,
    command_buffers: Vec<vk::CommandBuffer>,
    command_pool: CommandPool,
    frames_in_flight: usize,
    swapchain: Swapchain,
    swapchain_properties: SwapchainProperties,
    device: Arc<LogicalDevice>,
    presentation_queue: vk::Queue,
    graphics_queue: vk::Queue,
}

impl SwapchainCycle {
    pub fn new(context: &Context, dimensions: &[u32; 2], frames_in_flight: usize) -> Result<Self> {
        let frame_locks = Self::frame_locks(context.logical_device.clone(), frames_in_flight)?;

        let graphics_queue_index = context.physical_device.graphics_queue_index;
        let command_pool =
            Self::command_pool(context.logical_device.clone(), graphics_queue_index)?;

        let (swapchain, swapchain_properties) = create_swapchain(context, dimensions)?;
        let number_of_framebuffers = swapchain.images()?.len() as _;

        let command_buffers = command_pool
            .allocate_command_buffers(number_of_framebuffers, vk::CommandBufferLevel::PRIMARY)?;

        Ok(Self {
            frame: 0,
            frame_locks,
            command_buffers,
            command_pool,
            frames_in_flight,
            swapchain,
            swapchain_properties,
            device: context.logical_device.clone(),
            presentation_queue: context.presentation_queue(),
            graphics_queue: context.graphics_queue(),
        })
    }

    fn frame_locks(device: Arc<LogicalDevice>, frames_in_flight: usize) -> Result<Vec<FrameLock>> {
        (0..frames_in_flight)
            .map(|_| FrameLock::new(device.clone()))
            .collect()
    }

    fn command_pool(device: Arc<LogicalDevice>, queue_index: u32) -> Result<CommandPool> {
        let create_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_index);
        let command_pool = CommandPool::new(device, create_info)?;
        Ok(command_pool)
    }

    // TODO: Add callback that will record the command buffer
    pub fn render_frame(&mut self, dimensions: &[u32; 2]) -> Result<()> {
        self.wait_for_in_flight_fence()?;
        if let Some(image_index) = self.acquire_next_frame(dimensions)? {
            self.reset_in_flight_fence()?;
            // TODO: Update ubos and stuff
            self.record_command_buffer(image_index)?;
            self.submit_command_buffer(image_index)?;
            let result = self.present_next_frame(image_index)?;
            self.check_presentation_result(result, dimensions)?;
            self.increment_frame_counter();
        }
        Ok(())
    }

    fn increment_frame_counter(&mut self) {
        self.frame = (self.frame + 1) % self.frames_in_flight;
    }

    fn reset_in_flight_fence(&self) -> Result<()> {
        let in_flight_fence = self.frame_lock()?.in_flight.handle;
        unsafe { self.device.handle.reset_fences(&[in_flight_fence]) }?;
        Ok(())
    }

    fn wait_for_in_flight_fence(&self) -> Result<()> {
        let fence = self.frame_lock()?.in_flight.handle;
        unsafe {
            self.device
                .handle
                .wait_for_fences(&[fence], true, std::u64::MAX)
        }?;
        Ok(())
    }

    fn acquire_next_frame(&mut self, dimensions: &[u32; 2]) -> Result<Option<usize>> {
        let result = self
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
        let swapchains = [self.swapchain.handle_khr];
        let image_indices = [image_index as u32];

        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        let presentation_result = unsafe {
            self.swapchain
                .handle_ash
                .queue_present(self.presentation_queue, &present_info)
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

        unsafe { self.device.handle.device_wait_idle() }?;

        // self.render_path = None;
        // self.render_path = Some(RenderPath::new(&self.context, dimensions)?);
        Ok(())
    }

    fn record_command_buffer(&mut self, image_index: usize) -> Result<()> {
        let command_buffer = self.command_buffer_at(image_index)?;
        self.device.record_command_buffer(
            command_buffer,
            vk::CommandBufferUsageFlags::empty(),
            || Ok(()),
        )?;
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
        let lock = &self.frame_locks.get(self.frame).context(format!(
            "No frame lock was found at frame index: {}",
            self.frame
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
            self.device.handle.queue_submit(
                self.graphics_queue,
                &[submit_info.build()],
                lock.in_flight.handle,
            )
        }?;

        Ok(())
    }
}

impl Drop for SwapchainCycle {
    fn drop(&mut self) {
        unsafe {
            if let Err(error) = self.device.handle.device_wait_idle() {
                error!("{}", error);
            }
        }
    }
}
