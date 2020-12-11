pub use self::{device::*, instance::*, physical_device::*};

mod device;
mod instance;
mod physical_device;

use anyhow::{ensure, Context as AnyhowContext, Result};
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
    pub device: Arc<Device>,
    pub physical_device: PhysicalDevice,
    pub surface: Surface,
    pub instance: Instance,
    pub entry: ash::Entry,
}

impl Context {
    pub fn new(window_handle: &impl HasRawWindowHandle) -> Result<Self> {
        let entry = ash::Entry::new()?;
        let instance = Instance::new(&entry, window_handle)?;
        let surface = Surface::new(&entry, &instance.handle, window_handle)?;
        let physical_device = PhysicalDevice::new(&instance.handle, &surface)?;
        let device = Device::from_physical(&instance.handle, &physical_device)?;
        let device = Arc::new(device);

        let allocator_create_info = AllocatorCreateInfo {
            device: device.handle.clone(),
            instance: instance.handle.clone(),
            physical_device: physical_device.handle,
            ..Default::default()
        };

        let allocator = Arc::new(Allocator::new(&allocator_create_info)?);

        Ok(Self {
            allocator,
            device,
            physical_device,
            surface,
            instance,
            entry,
        })
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

    pub fn physical_device_format_properties(&self, format: vk::Format) -> vk::FormatProperties {
        unsafe {
            self.instance
                .handle
                .get_physical_device_format_properties(self.physical_device.handle, format)
        }
    }

    #[allow(dead_code)]
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
            let properties = self.physical_device_format_properties(*format);
            match tiling {
                vk::ImageTiling::LINEAR => properties.linear_tiling_features.contains(features),
                vk::ImageTiling::OPTIMAL => properties.optimal_tiling_features.contains(features),
                _ => false,
            }
        });

        depth_format.context("Couldn't determine the depth format!")
    }

    pub fn ensure_linear_blitting_supported(&self, format: vk::Format) -> Result<()> {
        let properties = self.physical_device_format_properties(format);

        let format_supported = properties
            .optimal_tiling_features
            .contains(vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR);

        ensure!(
            format_supported,
            "Linear blitting is not supported for format: {:?}",
            format
        );

        Ok(())
    }

    pub fn graphics_queue(&self) -> vk::Queue {
        let index = self.physical_device.graphics_queue_family_index;
        unsafe { self.device.handle.get_device_queue(index, 0) }
    }

    pub fn presentation_queue(&self) -> vk::Queue {
        let index = self.physical_device.presentation_queue_family_index;
        unsafe { self.device.handle.get_device_queue(index, 0) }
    }

    pub fn physical_device_properties(&self) -> vk::PhysicalDeviceProperties {
        unsafe {
            self.instance
                .handle
                .get_physical_device_properties(self.physical_device.handle)
        }
    }

    pub fn max_usable_samples(&self) -> vk::SampleCountFlags {
        let properties = self.physical_device_properties();
        let color_sample_counts = properties.limits.framebuffer_color_sample_counts;
        let depth_sample_counts = properties.limits.framebuffer_depth_sample_counts;
        let sample_counts = color_sample_counts.min(depth_sample_counts);

        if sample_counts.contains(vk::SampleCountFlags::TYPE_64) {
            vk::SampleCountFlags::TYPE_64
        } else if sample_counts.contains(vk::SampleCountFlags::TYPE_32) {
            vk::SampleCountFlags::TYPE_32
        } else if sample_counts.contains(vk::SampleCountFlags::TYPE_16) {
            vk::SampleCountFlags::TYPE_16
        } else if sample_counts.contains(vk::SampleCountFlags::TYPE_8) {
            vk::SampleCountFlags::TYPE_8
        } else if sample_counts.contains(vk::SampleCountFlags::TYPE_4) {
            vk::SampleCountFlags::TYPE_4
        } else if sample_counts.contains(vk::SampleCountFlags::TYPE_2) {
            vk::SampleCountFlags::TYPE_2
        } else {
            vk::SampleCountFlags::TYPE_1
        }
    }

    pub fn dynamic_alignment_of<T>(&self) -> u64 {
        let properties = self.physical_device_properties();
        let minimum_ubo_alignment = properties.limits.min_uniform_buffer_offset_alignment;
        let dynamic_alignment = std::mem::size_of::<T>() as u64;
        if minimum_ubo_alignment > 0 {
            (dynamic_alignment + minimum_ubo_alignment - 1) & !(minimum_ubo_alignment - 1)
        } else {
            dynamic_alignment
        }
    }
}

pub struct Surface {
    pub handle_ash: AshSurface,
    pub handle_khr: SurfaceKHR,
}

impl Surface {
    pub fn new(
        entry: &ash::Entry,
        instance: &ash::Instance,
        window_handle: &impl HasRawWindowHandle,
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
