use crate::core::Surface;
use anyhow::{anyhow, Result};
use ash::{version::InstanceV1_0, vk};
use log::info;
use std::ffi::CStr;

pub struct PhysicalDevice {
    pub handle: vk::PhysicalDevice,
    pub graphics_queue_family_index: u32,
    pub presentation_queue_family_index: u32,
}

impl PhysicalDevice {
    pub fn new(instance: &ash::Instance, surface: &Surface) -> Result<Self> {
        let devices = unsafe { instance.enumerate_physical_devices() }?;
        for device in devices {
            if let Some(physical_device) = Self::check_device_viability(device, instance, surface)?
            {
                return Ok(physical_device);
            }
        }
        Err(anyhow!("No suitable physical device was found!"))
    }

    fn check_device_viability(
        device: vk::PhysicalDevice,
        instance: &ash::Instance,
        surface: &Surface,
    ) -> Result<Option<Self>> {
        let device_name = Self::device_name(instance, device)?;
        let queue_indices = Self::find_queue_family_indices(instance, device, surface)?;
        let swapchain_supported = Self::swapchain_supported(device, surface)?;
        let features_supported = Self::features_supported(instance, device);

        if !swapchain_supported || queue_indices.is_none() || !features_supported {
            return Ok(None);
        }

        let (graphics_queue_family_index, presentation_queue_family_index) = queue_indices.unwrap();

        info!("Selected physical device: {:?}", device_name);
        let physical_device = Self {
            handle: device,
            graphics_queue_family_index,
            presentation_queue_family_index,
        };

        Ok(Some(physical_device))
    }

    fn device_name(instance: &ash::Instance, device: vk::PhysicalDevice) -> Result<String> {
        let properties = unsafe { instance.get_physical_device_properties(device) };
        let device_name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }.to_str()?;
        info!(
            "Physical device available: {:?} - {:?}",
            device_name, properties.device_type
        );
        Ok(device_name.into())
    }

    fn swapchain_supported(device: vk::PhysicalDevice, surface: &Surface) -> Result<bool> {
        let formats = unsafe {
            surface
                .handle_ash
                .get_physical_device_surface_formats(device, surface.handle_khr)
        }?;

        let present_modes = unsafe {
            surface
                .handle_ash
                .get_physical_device_surface_present_modes(device, surface.handle_khr)
        }?;

        let swapchain_supported = !formats.is_empty() && !present_modes.is_empty();
        Ok(swapchain_supported)
    }

    fn find_queue_family_indices(
        instance: &ash::Instance,
        device: vk::PhysicalDevice,
        surface: &Surface,
    ) -> Result<Option<(u32, u32)>> {
        let queue_family_properties =
            unsafe { instance.get_physical_device_queue_family_properties(device) };

        let (graphics_queue, presentation_queue) =
            Self::check_queue_families(queue_family_properties, device, surface)?;

        let indices = if let (Some(graphics_queue_index), Some(presentation_queue_index)) =
            (graphics_queue, presentation_queue)
        {
            Some((graphics_queue_index, presentation_queue_index))
        } else {
            None
        };

        Ok(indices)
    }

    fn check_queue_families(
        queue_family_properties: Vec<vk::QueueFamilyProperties>,
        device: vk::PhysicalDevice,
        surface: &Surface,
    ) -> Result<(Option<u32>, Option<u32>)> {
        // There may not be a single queue family that supports
        // both graphics and presentation to the surface
        let mut graphics_queue: Option<u32> = None;
        let mut presentation_queue: Option<u32> = None;

        for (index, family) in queue_family_properties
            .iter()
            .filter(|f| f.queue_count > 0)
            .enumerate()
        {
            let index = index as u32;
            let supports_graphics = family.queue_flags.contains(vk::QueueFlags::GRAPHICS);
            if supports_graphics && graphics_queue.is_none() {
                graphics_queue.replace(index);
            }

            let supports_presentation = unsafe {
                surface.handle_ash.get_physical_device_surface_support(
                    device,
                    index,
                    surface.handle_khr,
                )
            }?;

            if supports_presentation && presentation_queue.is_none() {
                presentation_queue.replace(index);
            }
        }

        Ok((graphics_queue, presentation_queue))
    }

    fn features_supported(instance: &ash::Instance, device: vk::PhysicalDevice) -> bool {
        let features = unsafe { instance.get_physical_device_features(device) };
        let required_features = [
            features.sampler_anisotropy,
            features.wide_lines,
            features.fill_mode_non_solid,
            features.wide_lines,
        ];
        required_features.iter().all(|feature| *feature == vk::TRUE)
    }

    pub fn queue_indices(&self) -> Vec<u32> {
        vec![
            self.graphics_queue_family_index,
            self.presentation_queue_family_index,
        ]
    }
}
