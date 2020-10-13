use crate::{Instance, PhysicalDevice};
use anyhow::Result;
use ash::{
    extensions::khr::Swapchain,
    version::{DeviceV1_0, InstanceV1_0},
    vk,
};
use std::os::raw::c_char;

pub struct LogicalDevice {
    pub handle: ash::Device,
}

impl LogicalDevice {
    pub fn new(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        create_info: vk::DeviceCreateInfoBuilder,
    ) -> Result<Self> {
        let handle = unsafe { instance.create_device(physical_device, &create_info, None) }?;
        let logical_device = Self { handle };
        Ok(logical_device)
    }

    pub fn from_physical(
        instance: &ash::Instance,
        physical_device: &PhysicalDevice,
    ) -> Result<Self> {
        let extensions = Self::extensions();

        let features = Self::features();

        let queue_indices = [
            physical_device.graphics_queue_index,
            physical_device.presentation_queue_index,
        ];
        let queue_create_info_list = Self::queue_create_info_list(&queue_indices);

        // Distinguishing between instance and device specific validation layers
        // has been deprecated as of Vulkan 1.1, but the spec recommends stil
        // passing the layer name pointers here to maintain backwards compatibility
        // with older implementations.
        let layers = Instance::layers()?;

        let create_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(queue_create_info_list.as_slice())
            .enabled_extension_names(&extensions)
            .enabled_features(&features)
            .enabled_layer_names(&layers);

        Self::new(instance, physical_device.handle, create_info)
    }

    fn extensions() -> Vec<*const c_char> {
        vec![Swapchain::name().as_ptr()]
    }

    fn features<'a>() -> vk::PhysicalDeviceFeaturesBuilder<'a> {
        vk::PhysicalDeviceFeatures::builder()
            .sample_rate_shading(true)
            .sampler_anisotropy(true)
    }

    fn queue_create_info_list(queue_indices: &[u32]) -> Vec<vk::DeviceQueueCreateInfo> {
        let mut queue_indices = queue_indices.to_vec();
        queue_indices.dedup();
        queue_indices
            .iter()
            .map(|index| {
                vk::DeviceQueueCreateInfo::builder()
                    .queue_family_index(*index)
                    .queue_priorities(&[1.0f32])
                    .build()
            })
            .collect::<Vec<_>>()
    }

    pub fn record_command_buffer<T>(
        &self,
        buffer: vk::CommandBuffer,
        usage: vk::CommandBufferUsageFlags,
        mut action: T,
    ) -> Result<()>
    where
        T: FnMut() -> Result<()>,
    {
        let begin_info = vk::CommandBufferBeginInfo::builder().flags(usage);
        unsafe { self.handle.begin_command_buffer(buffer, &begin_info) }?;
        action()?;
        unsafe { self.handle.end_command_buffer(buffer) }?;
        Ok(())
    }
}

impl Drop for LogicalDevice {
    fn drop(&mut self) {
        unsafe {
            self.handle.destroy_device(None);
        }
    }
}
