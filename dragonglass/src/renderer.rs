use super::{
    core::{Context, LogicalDevice},
    forward::ForwardSwapchain,
    resource::{CommandPool, Fence, Semaphore},
};
use anyhow::{anyhow, bail, Result};
use ash::{version::DeviceV1_0, vk};
use raw_window_handle::RawWindowHandle;
use std::sync::Arc;

pub struct Renderer {
    current_frame: usize,
    frame_locks: Vec<FrameLock>,
    command_buffers: Vec<vk::CommandBuffer>,
    _command_pool: CommandPool,
    forward_swapchain: Option<ForwardSwapchain>,
    context: Arc<Context>,
}

impl Renderer {
    const MAX_FRAMES_IN_FLIGHT: usize = 2;

    pub fn new(raw_window_handle: &RawWindowHandle, dimensions: &[u32; 2]) -> Result<Self> {
        let context = Arc::new(Context::new(&raw_window_handle)?);

        let frame_locks = (0..Self::MAX_FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|_| FrameLock::new(context.logical_device.clone()))
            .collect::<Result<Vec<FrameLock>, anyhow::Error>>()?;

        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(context.physical_device.graphics_queue_index)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = CommandPool::new(context.logical_device.clone(), create_info)?;

        let forward_swapchain = ForwardSwapchain::new(context.clone(), dimensions)?;

        let allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool.handle)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(forward_swapchain.framebuffers.len() as _);

        let command_buffers = unsafe {
            context
                .logical_device
                .handle
                .allocate_command_buffers(&allocate_info)
        }?;

        let renderer = Self {
            current_frame: 0,
            frame_locks,
            command_buffers,
            _command_pool: command_pool,
            forward_swapchain: Some(forward_swapchain),
            context,
        };

        Ok(renderer)
    }

    fn forward_swapchain(&self) -> Result<&ForwardSwapchain> {
        self.forward_swapchain
            .as_ref()
            .ok_or(anyhow!("No forward swapchain was available for rendering!"))
    }

    pub fn render(&mut self, dimensions: &[u32; 2]) -> Result<()> {
        self.wait_for_in_flight_fence()?;
        if let Some(image_index) = self.acquire_next_frame(dimensions)? {
            self.reset_in_flight_fence()?;
            self.record_command_buffer(image_index)?;
            self.submit_command_buffer(image_index)?;
            self.present_next_frame(image_index, dimensions)?;
            self.current_frame += (1 + self.current_frame) % Self::MAX_FRAMES_IN_FLIGHT;
        }
        Ok(())
    }

    fn reset_in_flight_fence(&self) -> Result<()> {
        let device = self.context.logical_device.handle.clone();
        let in_flight_fence = self.frame_locks[self.current_frame].in_flight.handle;
        unsafe { device.reset_fences(&[in_flight_fence]) }?;
        Ok(())
    }

    fn wait_for_in_flight_fence(&self) -> Result<()> {
        let device = self.context.logical_device.handle.clone();
        let in_flight_fence = self.frame_locks[self.current_frame].in_flight.handle;
        unsafe { device.wait_for_fences(&[in_flight_fence], true, std::u64::MAX) }?;
        Ok(())
    }

    fn acquire_next_frame(&mut self, dimensions: &[u32; 2]) -> Result<Option<usize>> {
        let frame_lock = &self.frame_locks[self.current_frame];

        let result = self
            .forward_swapchain()?
            .swapchain
            .acquire_next_image(frame_lock.image_available.handle, vk::Fence::null());

        match result {
            Ok((image_index, _)) => Ok(Some(image_index as usize)),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.create_swapchain(dimensions)?;
                Ok(None)
            }
            Err(error) => bail!(error),
        }
    }

    fn present_next_frame(&mut self, image_index: usize, dimensions: &[u32; 2]) -> Result<()> {
        let frame_lock = &self.frame_locks[self.current_frame];

        let device = self.context.logical_device.handle.clone();

        let wait_semaphores = [frame_lock.render_finished.handle];
        let swapchains = [self.forward_swapchain()?.swapchain.handle_khr];
        let image_indices = [image_index as u32];

        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        let presentation_queue_index = self.context.physical_device.presentation_queue_index;
        let presentation_queue = unsafe { device.get_device_queue(presentation_queue_index, 0) };

        let presentation_result = unsafe {
            self.forward_swapchain()?
                .swapchain
                .handle_ash
                .queue_present(presentation_queue, &present_info)
        };

        match presentation_result {
            Ok(is_suboptimal) if is_suboptimal => {
                self.create_swapchain(dimensions)?;
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.create_swapchain(dimensions)?;
            }
            Err(error) => bail!(error),
            _ => {}
        }

        Ok(())
    }

    fn create_swapchain(&mut self, dimensions: &[u32; 2]) -> Result<()> {
        unsafe { self.context.logical_device.handle.device_wait_idle() }?;

        self.forward_swapchain = None;
        self.forward_swapchain = Some(ForwardSwapchain::new(self.context.clone(), dimensions)?);

        Ok(())
    }

    fn record_command_buffer(&mut self, image_index: usize) -> Result<()> {
        let forward_swapchain = self.forward_swapchain()?;

        let extent = forward_swapchain.swapchain_properties.extent;

        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    // Cornflower blue
                    float32: [0.39, 0.58, 0.93, 1.0],
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];

        let command_buffer = *self.command_buffers.get(image_index).ok_or(anyhow!(
            "No command buffer was found for the forward swapchain at image index: {}",
            image_index
        ))?;

        let framebuffer = forward_swapchain
            .framebuffers
            .get(image_index)
            .ok_or(anyhow!(
                "No framebuffer was found for the forward swapchain at image index: {}",
                image_index
            ))?;

        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::SIMULTANEOUS_USE);
        let device = self.context.logical_device.handle.clone();
        unsafe { device.begin_command_buffer(command_buffer, &begin_info) }?;

        let begin_info = vk::RenderPassBeginInfo::builder()
            .render_pass(forward_swapchain.render_pass.handle)
            .framebuffer(framebuffer.handle)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            })
            .clear_values(&clear_values);

        unsafe {
            device.cmd_begin_render_pass(command_buffer, &begin_info, vk::SubpassContents::INLINE)
        };

        // TODO: Render stuff

        unsafe {
            device.cmd_end_render_pass(command_buffer);
        }

        unsafe { device.end_command_buffer(command_buffer) }?;

        Ok(())
    }

    fn submit_command_buffer(&self, image_index: usize) -> Result<()> {
        let frame_lock = &self.frame_locks[self.current_frame];

        let device = self.context.logical_device.handle.clone();

        let command_buffer = *self.command_buffers.get(image_index).ok_or(anyhow!(
            "No command buffer was found at image index: {}",
            image_index
        ))?;

        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let image_available_semaphores = [frame_lock.image_available.handle];
        let wait_semaphores = [frame_lock.render_finished.handle];
        let command_buffers = [command_buffer];

        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(&image_available_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&wait_semaphores);

        let graphics_queue_index = self.context.physical_device.graphics_queue_index;
        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_index, 0) };

        unsafe {
            device.queue_submit(
                graphics_queue,
                &[submit_info.build()],
                frame_lock.in_flight.handle,
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
