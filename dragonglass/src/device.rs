use super::{forward::RenderPath, scene::Scene};
use crate::{
    adapters::{CommandPool, Fence, Semaphore},
    context::{Context, LogicalDevice},
    resources::ShaderCache,
};
use anyhow::{bail, Context as AnyhowContext, Result};
use ash::{prelude::VkResult, version::DeviceV1_0, vk};
use log::error;
use nalgebra_glm as glm;
use raw_window_handle::HasRawWindowHandle;
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub struct RenderingDevice {
    scene: Rc<RefCell<Scene>>,
    shader_cache: ShaderCache,
    frame: usize,
    frame_locks: Vec<FrameLock>,
    command_buffers: Vec<vk::CommandBuffer>,
    _command_pool: CommandPool,
    _transient_command_pool: CommandPool,
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
        let transient_command_pool =
            Self::transient_command_pool(context.logical_device.clone(), graphics_queue_index)?;
        let mut shader_cache = ShaderCache::default();
        let mut render_path = RenderPath::new(&context, dimensions, &mut shader_cache)?;
        let scene = Scene::new(
            &context,
            &transient_command_pool,
            render_path.rendergraph.final_pass()?.render_pass.clone(),
            &mut shader_cache,
        )?;
        let number_of_framebuffers = render_path.swapchain.images()?.len() as _;
        let command_buffers = command_pool
            .allocate_command_buffers(number_of_framebuffers, vk::CommandBufferLevel::PRIMARY)?;

        let scene = Rc::new(RefCell::new(scene));

        let scene_ptr = scene.clone();
        render_path
            .rendergraph
            .passes
            .get_mut("offscreen")
            .context("Failed to get offscreen pass to set scene callback")?
            .set_callback(move |command_buffer| scene_ptr.borrow().issue_commands(command_buffer));

        let renderer = Self {
            scene,
            shader_cache,
            frame: 0,
            frame_locks,
            command_buffers,
            _command_pool: command_pool,
            _transient_command_pool: transient_command_pool,
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

    fn transient_command_pool(device: Arc<LogicalDevice>, queue_index: u32) -> Result<CommandPool> {
        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_index)
            .flags(vk::CommandPoolCreateFlags::TRANSIENT);
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
            self.update(*view)?;
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

    fn update(&self, view: glm::Mat4) -> Result<()> {
        let aspect_ratio = self.render_path()?.swapchain_properties.aspect_ratio();
        self.scene.borrow().update_ubo(aspect_ratio, view)?;
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
        let mut render_path = RenderPath::new(&self.context, dimensions, &mut self.shader_cache)?;

        let render_pass = render_path.rendergraph.final_pass()?.render_pass.clone();
        self.scene
            .borrow_mut()
            .create_pipeline(render_pass, &mut self.shader_cache)?;

        let scene_ptr = self.scene.clone();
        render_path
            .rendergraph
            .passes
            .get_mut("offscreen")
            .context("Failed to get offscreen pass to set scene callback")?
            .set_callback(move |command_buffer| scene_ptr.borrow().issue_commands(command_buffer));
        self.render_path = Some(render_path);
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
