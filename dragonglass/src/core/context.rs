use super::{Instance, LogicalDevice, PhysicalDevice};
use anyhow::{anyhow, Result};
use ash::{
    extensions::khr::Surface as AshSurface,
    version::{DeviceV1_0, InstanceV1_0},
    vk::{self, SurfaceKHR},
};
use ash_window::create_surface;
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;
use vk_mem::{Allocator, AllocatorCreateInfo};

// The order the struct members are declared in
// determines the order they are 'Drop'ped in
// when this struct is dropped
pub struct Context {
    pub allocator: Arc<vk_mem::Allocator>,
    pub logical_device: Arc<LogicalDevice>,
    pub physical_device: PhysicalDevice,
    pub surface: Surface,
    pub instance: Instance,
    pub entry: ash::Entry,
}

impl Context {
    pub fn new<T: HasRawWindowHandle>(window_handle: &T) -> Result<Self> {
        let entry = ash::Entry::new()?;
        let instance = Instance::new(&entry, window_handle)?;
        let surface = Surface::new(&entry, &instance.handle, window_handle)?;
        let physical_device = PhysicalDevice::new(&instance.handle, &surface)?;
        let logical_device = LogicalDevice::from_physical(&instance.handle, &physical_device)?;
        let logical_device = Arc::new(logical_device);

        let allocator_create_info = AllocatorCreateInfo {
            device: logical_device.handle.clone(),
            instance: instance.handle.clone(),
            physical_device: physical_device.handle,
            ..Default::default()
        };

        let allocator = Arc::new(Allocator::new(&allocator_create_info)?);

        let context = Self {
            allocator,
            logical_device,
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

    pub fn determine_depth_format(
        &self,
        tiling: vk::ImageTiling,
        features: vk::FormatFeatureFlags,
    ) -> Result<vk::Format> {
        let depth_format = vec![
            vk::Format::D32_SFLOAT,
            vk::Format::D32_SFLOAT_S8_UINT,
            vk::Format::D24_UNORM_S8_UINT,
        ]
        .into_iter()
        .find(|format| {
            let properties = unsafe {
                self.instance
                    .handle
                    .get_physical_device_format_properties(self.physical_device.handle, *format)
            };

            match tiling {
                vk::ImageTiling::LINEAR => properties.linear_tiling_features.contains(features),
                vk::ImageTiling::OPTIMAL => properties.optimal_tiling_features.contains(features),
                _ => false,
            }
        });

        depth_format.ok_or_else(|| anyhow!("Couldn't determine the depth format!"))
    }

    pub fn graphics_queue(&self) -> vk::Queue {
        let index = self.physical_device.presentation_queue_index;
        unsafe { self.logical_device.handle.get_device_queue(index, 0) }
    }

    pub fn presentation_queue(&self) -> vk::Queue {
        let index = self.physical_device.presentation_queue_index;
        unsafe { self.logical_device.handle.get_device_queue(index, 0) }
    }
}

pub struct Surface {
    pub handle_ash: AshSurface,
    pub handle_khr: SurfaceKHR,
}

impl Surface {
    pub fn new<T: HasRawWindowHandle>(
        entry: &ash::Entry,
        instance: &ash::Instance,
        window_handle: &T,
    ) -> Result<Self> {
        let handle_ash = AshSurface::new(entry, instance);
        let handle_khr = unsafe { create_surface(entry, instance, window_handle, None) }?;
        let surface = Self {
            handle_ash,
            handle_khr,
        };
        Ok(surface)
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            self.handle_ash.destroy_surface(self.handle_khr, None);
        }
    }
}
