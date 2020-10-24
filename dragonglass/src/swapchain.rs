use crate::{
    adapters::{CommandPool, Fence, Semaphore},
    context::{Context, LogicalDevice, Surface},
};
use anyhow::{bail, ensure, Context as AnyhowContext, Result};
use ash::{extensions::khr::Swapchain as AshSwapchain, prelude::VkResult, version::DeviceV1_0, vk};
use std::{cmp, sync::Arc};

pub struct VulkanSwapchain {
    pub handle_ash: AshSwapchain,
    pub handle_khr: vk::SwapchainKHR,
}

impl VulkanSwapchain {
    pub fn new(
        instance: &ash::Instance,
        device: &ash::Device,
        create_info: vk::SwapchainCreateInfoKHRBuilder,
    ) -> Result<Self> {
        let handle_ash = AshSwapchain::new(instance, device);
        let handle_khr = unsafe { handle_ash.create_swapchain(&create_info, None) }?;
        let swapchain = Self {
            handle_ash,
            handle_khr,
        };
        Ok(swapchain)
    }

    pub fn images(&self) -> Result<Vec<vk::Image>> {
        let images = unsafe { self.handle_ash.get_swapchain_images(self.handle_khr) }?;
        Ok(images)
    }

    pub fn acquire_next_image(
        &self,
        semaphore: vk::Semaphore,
        fence: vk::Fence,
    ) -> ash::prelude::VkResult<(u32, bool)> {
        unsafe {
            self.handle_ash
                .acquire_next_image(self.handle_khr, std::u64::MAX, semaphore, fence)
        }
    }
}

impl Drop for VulkanSwapchain {
    fn drop(&mut self) {
        unsafe {
            self.handle_ash.destroy_swapchain(self.handle_khr, None);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SwapchainProperties {
    pub surface_format: vk::SurfaceFormatKHR,
    pub present_mode: vk::PresentModeKHR,
    pub extent: vk::Extent2D,
}

impl SwapchainProperties {
    pub fn new(
        dimensions: &[u32; 2],
        device: vk::PhysicalDevice,
        surface: &Surface,
    ) -> Result<Self> {
        let extent = Self::select_extent(dimensions, device, surface)?;
        let surface_format = Self::select_format(device, surface)?;
        let present_mode = Self::select_present_mode(device, surface)?;
        let properties = Self {
            surface_format,
            present_mode,
            extent,
        };
        Ok(properties)
    }

    fn select_extent(
        dimensions: &[u32; 2],
        device: vk::PhysicalDevice,
        surface: &Surface,
    ) -> Result<vk::Extent2D> {
        let capabilities = unsafe {
            surface
                .handle_ash
                .get_physical_device_surface_capabilities(device, surface.handle_khr)
        }?;

        if capabilities.current_extent.width == std::u32::MAX {
            let min = capabilities.min_image_extent;
            let max = capabilities.max_image_extent;
            let width = dimensions[0].min(max.width).max(min.width);
            let height = dimensions[1].min(max.height).max(min.height);
            let extent = vk::Extent2D { width, height };
            Ok(extent)
        } else {
            Ok(capabilities.current_extent)
        }
    }

    fn select_format(
        device: vk::PhysicalDevice,
        surface: &Surface,
    ) -> Result<vk::SurfaceFormatKHR> {
        let formats = unsafe {
            surface
                .handle_ash
                .get_physical_device_surface_formats(device, surface.handle_khr)
        }?;

        let error_message = "No physical device surface formats are available!";
        ensure!(!formats.is_empty(), error_message);

        let default_format = vk::SurfaceFormatKHR {
            format: vk::Format::R8G8B8A8_UNORM,
            color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
        };

        let all_formats_undefined = formats
            .iter()
            .all(|format| format.format == vk::Format::UNDEFINED);

        let default_available = formats.contains(&default_format);

        if default_available || all_formats_undefined {
            Ok(default_format)
        } else {
            log::info!("Non-default swapchain format selected: {:#?}", formats[0]);
            Ok(formats[0])
        }
    }

    fn select_present_mode(
        device: vk::PhysicalDevice,
        surface: &Surface,
    ) -> Result<vk::PresentModeKHR> {
        let present_modes = unsafe {
            surface
                .handle_ash
                .get_physical_device_surface_present_modes(device, surface.handle_khr)
        }?;

        let present_mode = match present_modes.as_slice() {
            [vk::PresentModeKHR::MAILBOX, ..] => vk::PresentModeKHR::MAILBOX,
            [vk::PresentModeKHR::FIFO, ..] => vk::PresentModeKHR::FIFO,
            _ => vk::PresentModeKHR::IMMEDIATE,
        };

        Ok(present_mode)
    }

    pub fn aspect_ratio(&self) -> f32 {
        self.extent.width as f32 / cmp::max(self.extent.height, 1) as f32
    }
}

pub fn create_swapchain(
    context: &Context,
    dimensions: &[u32; 2],
) -> Result<(VulkanSwapchain, SwapchainProperties)> {
    let properties =
        SwapchainProperties::new(dimensions, context.physical_device.handle, &context.surface)?;

    let queue_indices = context.physical_device.queue_indices();
    let create_info = swapchain_create_info(context, &queue_indices, properties)?;

    let swapchain = VulkanSwapchain::new(
        &context.instance.handle,
        &context.logical_device.handle,
        create_info,
    )?;

    Ok((swapchain, properties))
}

fn swapchain_create_info<'a>(
    context: &Context,
    queue_indices: &'a [u32],
    properties: SwapchainProperties,
) -> Result<vk::SwapchainCreateInfoKHRBuilder<'a>> {
    let capabilities = context.physical_device_surface_capabilities()?;
    let image_count = std::cmp::max(
        capabilities.max_image_count,
        capabilities.min_image_count + 1,
    );
    let builder = vk::SwapchainCreateInfoKHR::builder()
        .surface(context.surface.handle_khr)
        .min_image_count(image_count)
        .image_format(properties.surface_format.format)
        .image_color_space(properties.surface_format.color_space)
        .image_extent(properties.extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .pre_transform(capabilities.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(properties.present_mode)
        .clipped(true);

    let builder = if queue_indices.len() == 1 {
        // Only one queue family is being used for graphics and presentation
        builder
            .image_sharing_mode(vk::SharingMode::CONCURRENT)
            .queue_family_indices(queue_indices)
    } else {
        builder.image_sharing_mode(vk::SharingMode::EXCLUSIVE)
    };

    Ok(builder)
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

pub struct Swapchain {
    frame: usize,
    frame_locks: Vec<FrameLock>,
    command_buffers: Vec<vk::CommandBuffer>,
    _command_pool: CommandPool,
    frames_in_flight: usize,
    swapchain: Option<VulkanSwapchain>,
    pub properties: SwapchainProperties,
    pub recreated_swapchain: bool,
    context: Arc<Context>,
}

impl Swapchain {
    pub fn new(
        context: Arc<Context>,
        dimensions: &[u32; 2],
        frames_in_flight: usize,
    ) -> Result<Self> {
        let frame_locks = (0..frames_in_flight)
            .map(|_| FrameLock::new(context.logical_device.clone()))
            .collect::<Result<Vec<_>>>()?;

        let graphics_queue_index = context.physical_device.graphics_queue_index;
        let command_pool = CommandPool::new(
            context.logical_device.clone(),
            vk::CommandPoolCreateInfo::builder()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(graphics_queue_index),
        )?;

        let (swapchain, properties) = create_swapchain(&context, dimensions)?;
        let number_of_framebuffers = swapchain.images()?.len() as _;
        let command_buffers = command_pool
            .allocate_command_buffers(number_of_framebuffers, vk::CommandBufferLevel::PRIMARY)?;

        Ok(Self {
            frame: 0,
            frame_locks,
            command_buffers,
            _command_pool: command_pool,
            frames_in_flight,
            swapchain: Some(swapchain),
            recreated_swapchain: false,
            properties,
            context,
        })
    }

    pub fn swapchain(&self) -> Result<&VulkanSwapchain> {
        self.swapchain
            .as_ref()
            .context("Failed to get inner swapchain!")
    }

    pub fn render_frame(
        &mut self,
        dimensions: &[u32; 2],
        mut pre_recording_callback: impl FnMut(&SwapchainProperties) -> Result<()>,
        mut recording_callback: impl FnMut(vk::CommandBuffer, usize) -> Result<()>,
    ) -> Result<()> {
        self.recreated_swapchain = false;
        self.wait_for_in_flight_fence()?;
        if let Some(image_index) = self.acquire_next_frame(dimensions)? {
            self.reset_in_flight_fence()?;
            pre_recording_callback(&self.properties)?;
            self.context.logical_device.record_command_buffer(
                self.command_buffer_at(image_index)?,
                vk::CommandBufferUsageFlags::empty(),
                |command_buffer| recording_callback(command_buffer, image_index),
            )?;
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
        unsafe {
            self.context
                .logical_device
                .handle
                .reset_fences(&[in_flight_fence])
        }?;
        Ok(())
    }

    fn wait_for_in_flight_fence(&self) -> Result<()> {
        let fence = self.frame_lock()?.in_flight.handle;
        unsafe {
            self.context
                .logical_device
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

        unsafe { self.context.logical_device.handle.device_wait_idle() }?;

        self.swapchain = None;
        let (swapchain, properties) = create_swapchain(&self.context, dimensions)?;
        self.swapchain = Some(swapchain);
        self.properties = properties;

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
