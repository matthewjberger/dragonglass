use super::{ImageView, LogicalDevice, Surface};
use anyhow::{ensure, Result};
use ash::{extensions::khr::Swapchain as AshSwapchain, vk};
use std::sync::Arc;

pub struct SwapchainImage {
    image: vk::Image,
    view: ImageView,
}

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
    ) -> Result<Vec<ImageView>> {
        let images = unsafe { self.handle_ash.get_swapchain_images(self.handle_khr) }?;
        images
            .iter()
            .map(|image| Self::create_image_view(*image, device.clone(), format))
            .collect::<Result<Vec<_>>>()
    }

    fn create_image_view(
        image: vk::Image,
        device: Arc<LogicalDevice>,
        format: vk::Format,
    ) -> Result<ImageView> {
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
        ImageView::new(device, create_info)
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

    pub fn aspect_ratio(&self) -> f32 {
        let height = if self.extent.height == 0 {
            0
        } else {
            self.extent.height
        };
        self.extent.width as f32 / height as f32
    }
}
