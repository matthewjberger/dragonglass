use crate::{
    context::{Context, LogicalDevice, Surface},
    resources::{Image, ImageView},
};
use anyhow::{ensure, Result};
use ash::{extensions::khr::Swapchain as AshSwapchain, vk};
use std::{cmp, sync::Arc};

pub struct Swapchain {
    pub handle_ash: AshSwapchain,
    pub handle_khr: vk::SwapchainKHR,
    pub images: Vec<SwapchainImage>,
}

impl Swapchain {
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
            images: Vec::new(),
        };
        Ok(swapchain)
    }

    pub fn images(&self) -> Result<Vec<vk::Image>> {
        let images = unsafe { self.handle_ash.get_swapchain_images(self.handle_khr) }?;
        Ok(images)
    }

    pub fn create_image_views(
        &mut self,
        device: Arc<LogicalDevice>,
        format: vk::Format,
    ) -> Result<()> {
        self.images = self
            .images()?
            .into_iter()
            .map(|image| SwapchainImage::new(image, device.clone(), format))
            .collect::<Result<Vec<_>>>()?;
        Ok(())
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

impl Drop for Swapchain {
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

pub struct SwapchainImage {
    pub image: vk::Image,
    pub view: ImageView,
}

impl Image for SwapchainImage {
    fn handle(&self) -> vk::Image {
        self.image
    }
}

impl SwapchainImage {
    pub fn new(image: vk::Image, device: Arc<LogicalDevice>, format: vk::Format) -> Result<Self> {
        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .components(vk::ComponentMapping {
                r: vk::ComponentSwizzle::IDENTITY,
                g: vk::ComponentSwizzle::IDENTITY,
                b: vk::ComponentSwizzle::IDENTITY,
                a: vk::ComponentSwizzle::IDENTITY,
            })
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let view = ImageView::new(device, create_info)?;
        let swapchain_image = Self { image, view };
        Ok(swapchain_image)
    }
}

pub fn create_swapchain(
    context: &Context,
    dimensions: &[u32; 2],
) -> Result<(Swapchain, SwapchainProperties)> {
    let properties =
        SwapchainProperties::new(dimensions, context.physical_device.handle, &context.surface)?;

    let queue_indices = context.physical_device.queue_indices();
    let create_info = swapchain_create_info(context, &queue_indices, properties)?;

    let mut swapchain = Swapchain::new(
        &context.instance.handle,
        &context.logical_device.handle,
        create_info,
    )?;

    swapchain.create_image_views(
        context.logical_device.clone(),
        properties.surface_format.format,
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
