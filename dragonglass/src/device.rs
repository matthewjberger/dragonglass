use super::{
    core::{Context, LogicalDevice},
    forward::ForwardSwapchain,
    render::{CommandPool, Fence, RenderPass, Semaphore, ShaderCache},
    scene::Scene,
};
use anyhow::{anyhow, bail, Result};
use ash::{prelude::VkResult, version::DeviceV1_0, vk};
use log::error;
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;

pub struct RenderingDevice {
    scene: Scene,
    shader_cache: ShaderCache,
    frame: usize,
    frame_locks: Vec<FrameLock>,
    command_buffers: Vec<vk::CommandBuffer>,
    _command_pool: CommandPool,
    transient_command_pool: CommandPool,
    forward_swapchain: Option<ForwardSwapchain>,
    context: Context,
}

impl RenderingDevice {
    const MAX_FRAMES_IN_FLIGHT: usize = 2;

    pub fn new<T: HasRawWindowHandle>(window_handle: &T, dimensions: &[u32; 2]) -> Result<Self> {
        let context = Context::new(window_handle)?;
        let device = context.logical_device.clone();
        let frame_locks = Self::frame_locks(device)?;
        let device = context.logical_device.clone();
        let graphics_queue_index = context.physical_device.graphics_queue_index;
        let command_pool = Self::command_pool(device.clone(), graphics_queue_index)?;
        let transient_command_pool = Self::transient_command_pool(device, graphics_queue_index)?;
        let forward_swapchain = ForwardSwapchain::new(&context, dimensions)?;
        let mut shader_cache = ShaderCache::default();
        let scene = Scene::new(
            &context,
            &transient_command_pool,
            forward_swapchain.render_pass.clone(),
            &mut shader_cache,
        )?;
        let number_of_framebuffers = forward_swapchain.framebuffers.len() as _;
        let command_buffers = command_pool
            .allocate_command_buffers(number_of_framebuffers, vk::CommandBufferLevel::PRIMARY)?;
        let renderer = Self {
            scene,
            shader_cache,
            frame: 0,
            frame_locks,
            command_buffers,
            _command_pool: command_pool,
            transient_command_pool,
            forward_swapchain: Some(forward_swapchain),
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

    fn transient_command_pool(device: Arc<LogicalDevice>, queue_index: u32) -> Result<CommandPool> {
        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_index)
            .flags(vk::CommandPoolCreateFlags::TRANSIENT);
        let command_pool = CommandPool::new(device, create_info)?;
        Ok(command_pool)
    }

    fn forward_swapchain(&self) -> Result<&ForwardSwapchain> {
        self.forward_swapchain
            .as_ref()
            .ok_or_else(|| anyhow!("No forward swapchain was available for rendering!"))
    }

    fn device(&self) -> ash::Device {
        self.context.logical_device.handle.clone()
    }

    pub fn render(&mut self, dimensions: &[u32; 2]) -> Result<()> {
        self.wait_for_in_flight_fence()?;
        if let Some(image_index) = self.acquire_next_frame(dimensions)? {
            self.reset_in_flight_fence()?;
            self.update()?;
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

    fn update(&self) -> Result<()> {
        let aspect_ratio = self
            .forward_swapchain()?
            .swapchain_properties
            .aspect_ratio();
        self.scene.update_ubo(aspect_ratio)?;
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
        if dimensions[0] == 0 || dimensions[1] == 0 {
            return Ok(());
        }

        unsafe { self.context.logical_device.handle.device_wait_idle() }?;

        self.forward_swapchain = None;
        self.forward_swapchain = Some(ForwardSwapchain::new(&self.context, dimensions)?);

        let render_pass = self.forward_swapchain()?.render_pass.clone();
        self.scene = Scene::new(
            &self.context,
            &self.transient_command_pool,
            render_pass,
            &mut self.shader_cache,
        )?;

        Ok(())
    }

    fn record_command_buffer(&mut self, image_index: usize) -> Result<()> {
        let command_buffer = *self.command_buffers.get(image_index).ok_or_else(|| {
            anyhow!(
                "No command buffer was found for the forward swapchain at image index: {}",
                image_index
            )
        })?;

        self.context.logical_device.clone().record_command_buffer(
            command_buffer,
            vk::CommandBufferUsageFlags::empty(),
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
        let extent = self.forward_swapchain()?.swapchain_properties.extent;
        let clear_values = Self::clear_values();
        let begin_info = vk::RenderPassBeginInfo::builder()
            .render_pass(self.forward_swapchain()?.render_pass.handle)
            .framebuffer(self.framebuffer_at(image_index)?)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            })
            .clear_values(&clear_values);

        RenderPass::record(
            self.context.logical_device.clone(),
            command_buffer,
            begin_info,
            || {
                self.update_viewport(command_buffer)?;
                self.scene.issue_commands(command_buffer)?;
                Ok(())
            },
        )?;

        Ok(())
    }

    fn update_viewport(&self, command_buffer: vk::CommandBuffer) -> Result<()> {
        let extent = self.forward_swapchain()?.swapchain_properties.extent;

        let viewport = vk::Viewport {
            x: 0.0,
            y: extent.height as _,
            width: extent.width as _,
            height: (-1.0 * extent.height as f32) as _,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let viewports = [viewport];

        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };
        let scissors = [scissor];

        let device = self.context.logical_device.handle.clone();
        unsafe {
            device.cmd_set_viewport(command_buffer, 0, &viewports);
            device.cmd_set_scissor(command_buffer, 0, &scissors);
        }

        Ok(())
    }

    fn framebuffer_at(&self, image_index: usize) -> Result<vk::Framebuffer> {
        let framebuffer = self
            .forward_swapchain()?
            .framebuffers
            .get(image_index)
            .ok_or_else(|| {
                anyhow!(
                    "No framebuffer was found for the forward swapchain at image index: {}",
                    image_index
                )
            })?
            .handle;
        Ok(framebuffer)
    }

    fn command_buffer_at(&self, image_index: usize) -> Result<vk::CommandBuffer> {
        let command_buffer = *self.command_buffers.get(image_index).ok_or_else(|| {
            anyhow!(
                "No command buffer was found at image index: {}",
                image_index
            )
        })?;
        Ok(command_buffer)
    }

    fn frame_lock(&self) -> Result<&FrameLock> {
        let lock = &self
            .frame_locks
            .get(self.frame)
            .ok_or_else(|| anyhow!("No frame lock was found at frame index: {}", self.frame,))?;
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
