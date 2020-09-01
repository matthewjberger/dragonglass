use super::{
    debug::DebugLayer, Instance, LogicalDevice, PhysicalDevice, Surface, Swapchain,
    SwapchainProperties,
};
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use log::info;
use raw_window_handle::RawWindowHandle;
use std::cmp;
use std::sync::Arc;
use vk_mem::{Allocator, AllocatorCreateInfo};

// The order the struct members are declared in
// determines the order they are 'Drop'ped in
// when this struct is dropped
pub struct Context {
    pub allocator: vk_mem::Allocator,
    pub logical_device: Arc<LogicalDevice>,
    pub debug_layer: Option<DebugLayer>,
    pub physical_device: PhysicalDevice,
    pub surface: Surface,
    pub instance: Instance,
    pub entry: ash::Entry,
}

impl Context {
    pub fn new(raw_window_handle: &RawWindowHandle) -> Result<Self> {
        let entry = ash::Entry::new()?;
        let instance = Instance::new(&entry)?;
        let surface = Surface::new(&entry, &instance.handle, &raw_window_handle)?;
        let physical_device = PhysicalDevice::new(&instance.handle, &surface)?;
        let debug_layer = if DebugLayer::enabled() {
            info!("Loading debug layer");
            Some(DebugLayer::new(&entry, &instance.handle)?)
        } else {
            None
        };
        let logical_device = Arc::new(LogicalDevice::from_physical(
            &instance.handle,
            &physical_device,
        )?);

        let allocator_create_info = AllocatorCreateInfo {
            device: logical_device.handle.clone(),
            instance: instance.handle.clone(),
            physical_device: physical_device.handle,
            ..Default::default()
        };

        let allocator = Allocator::new(&allocator_create_info)?;

        let context = Self {
            allocator,
            logical_device,
            debug_layer,
            physical_device,
            surface,
            instance,
            entry,
        };

        Ok(context)
    }

    pub fn physical_device_surface_capabilities(&self) -> Result<vk::SurfaceCapabilitiesKHR> {
        let capabilities = unsafe {
            self.surface
                .handle_ash
                .get_physical_device_surface_capabilities(
                    self.physical_device.handle,
                    self.surface.handle_khr,
                )
        }?;
        Ok(capabilities)
    }

    pub fn create_swapchain(&self, dimensions: &[u32; 2]) -> Result<Swapchain> {
        let properties =
            SwapchainProperties::new(dimensions, self.physical_device.handle, &self.surface)?;

        let capabilities = self.physical_device_surface_capabilities()?;

        let image_count = cmp::max(
            capabilities.max_image_count,
            capabilities.min_image_count + 1,
        );

        let queue_indices = self.physical_device.queue_indices();

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

        Ok(swapchain)
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            self.logical_device
                .handle
                .device_wait_idle()
                .expect("Failed to wait for the logical device to idle when dropping the context!")
        }
    }
}
