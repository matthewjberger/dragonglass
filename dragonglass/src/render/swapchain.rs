use super::resource::ImageView;
use crate::core::{LogicalDevice, Surface};
use anyhow::{ensure, Result};
use ash::{extensions::khr::Swapchain as AshSwapchain, vk};
use std::sync::Arc;

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

    pub fn create_image_views(
        &mut self,
        device: Arc<LogicalDevice>,
        format: vk::Format,
    ) -> Result<()> {
        let images = unsafe { self.handle_ash.get_swapchain_images(self.handle_khr) }?;
        self.images = images
            .iter()
            .map(|image| SwapchainImage::new(*image, device.clone(), format))
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

        if capabilities.current_extent.width != std::u32::MAX {
            Ok(capabilities.current_extent)
        } else {
            let min = capabilities.min_image_extent;
            let max = capabilities.max_image_extent;
            let width = dimensions[0].min(max.width).max(min.width);
            let height = dimensions[1].min(max.height).max(min.height);
            let extent = vk::Extent2D { width, height };
            Ok(extent)
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
        ensure!(formats.len() > 0, error_message);

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
}

pub struct SwapchainImage {
    pub image: vk::Image,
    pub view: ImageView,
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

impl crate::core::Context {
    pub fn create_swapchain(
        &self,
        dimensions: &[u32; 2],
    ) -> Result<(Swapchain, SwapchainProperties)> {
        let capabilities = self.physical_device_surface_capabilities()?;

        let image_count = std::cmp::max(
            capabilities.max_image_count,
            capabilities.min_image_count + 1,
        );

        let queue_indices = self.physical_device.queue_indices();

        let properties =
            SwapchainProperties::new(dimensions, self.physical_device.handle, &self.surface)?;

        let create_info = {
            let builder = vk::SwapchainCreateInfoKHR::builder()
                .surface(self.surface.handle_khr)
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

            if queue_indices.len() == 1 {
                // Only one queue family is being used for graphics and presentation
                builder
                    .image_sharing_mode(vk::SharingMode::CONCURRENT)
                    .queue_family_indices(&queue_indices)
            } else {
                builder.image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            }
        };

        let mut swapchain = Swapchain::new(
            &self.instance.handle,
            &self.logical_device.handle,
            create_info,
        )?;

        swapchain.create_image_views(
            self.logical_device.clone(),
            properties.surface_format.format,
        )?;

        Ok((swapchain, properties))
    }
}
