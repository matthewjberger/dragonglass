use super::{debug::DebugLayer, Instance, LogicalDevice, PhysicalDevice, Surface};
use anyhow::{anyhow, Result};
use ash::{
    version::{DeviceV1_0, InstanceV1_0},
    vk,
};
use log::info;
use raw_window_handle::RawWindowHandle;
use std::sync::Arc;
use vk_mem::{Allocator, AllocatorCreateInfo};

// The order the struct members are declared in
// determines the order they are 'Drop'ped in
// when this struct is dropped
pub struct Context {
    pub allocator: Arc<vk_mem::Allocator>,
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

        let allocator = Arc::new(Allocator::new(&allocator_create_info)?);

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

        if let Some(format) = depth_format {
            Ok(format)
        } else {
            Err(anyhow!("Couldn't determine the depth format!"))
        }
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
