pub use self::{debug::*, device::*, instance::*, physical_device::*};

mod debug;
mod device;
mod instance;
mod physical_device;

use anyhow::{ensure, Context as AnyhowContext, Result};
use ash::{
    extensions::khr::{Surface as AshSurface, Swapchain},
    vk::{self, SurfaceKHR},
};
use ash_window::{create_surface, enumerate_required_extensions};
use gpu_allocator::vulkan::{Allocator, AllocatorCreateDesc};
use raw_window_handle::HasRawWindowHandle;
use std::{os::raw::c_char, sync::Arc};

// The order the struct members are declared in
// determines the order they are 'Drop'ped in
// when this struct is dropped
pub struct Context {
    pub debug: Option<VulkanDebug>,
    pub allocator: Arc<Allocator>,
    pub device: Arc<Device>,
    pub physical_device: PhysicalDevice,
    pub surface: Option<Surface>,
    pub instance: Instance,
    pub entry: ash::Entry,
}

impl Context {
    pub fn new(window_handle: &impl HasRawWindowHandle) -> Result<Self> {
        let instance_extensions = Self::instance_extensions(window_handle)?;
        let layers = Self::layers()?;
        let device_extensions = Self::device_extensions();
        let features = Self::features();

        let entry = unsafe { ash::Entry::new()? };
        let instance = Instance::new(&entry, &instance_extensions, &layers)?;
        let surface = Surface::new(&entry, &instance.handle, window_handle)?;
        let physical_device = PhysicalDevice::new(&instance.handle, &surface)?;

        let mut queue_indices = vec![
            physical_device.graphics_queue_family_index,
            physical_device.presentation_queue_family_index,
        ];
        queue_indices.dedup();
        let queue_create_info_list = queue_indices
            .iter()
            .map(|index| {
                vk::DeviceQueueCreateInfo::builder()
                    .queue_family_index(*index)
                    .queue_priorities(&[1.0f32])
                    .build()
            })
            .collect::<Vec<_>>();

        // Distinguishing between instance and device specific validation layers
        // has been deprecated as of Vulkan 1.1, but the spec recommends stil
        // passing the layer name pointers here to maintain backwards compatibility
        // with older implementations.
        let create_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(queue_create_info_list.as_slice())
            .enabled_extension_names(&device_extensions)
            .enabled_features(&features)
            .enabled_layer_names(&layers);

        let device = Device::new(&instance.handle, physical_device.handle, create_info)?;
        let device = Arc::new(device);

        let mut allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.handle,
            device: device.handle,
            physical_device: physical_device.handle,
            debug_settings: Default::default(),
            buffer_device_address: true, // Ideally, check the BufferDeviceAddressFeatures struct.
        });

        let debug = if VulkanDebug::enabled() {
            Some(VulkanDebug::new(&entry, &instance.handle, device.clone())?)
        } else {
            None
        };

        Ok(Self {
            debug,
            allocator,
            device,
            physical_device,
            surface: Some(surface),
            instance,
            entry,
        })
    }

    fn instance_extensions(window_handle: &impl HasRawWindowHandle) -> Result<Vec<*const i8>> {
        let mut extensions: Vec<*const i8> = enumerate_required_extensions(window_handle)?
            .iter()
            .map(|extension| extension.as_ptr())
            .collect();
        if VulkanDebug::enabled() {
            extensions.push(VulkanDebug::extension_name().as_ptr());
        }
        Ok(extensions)
    }

    fn layers() -> Result<Vec<*const i8>> {
        let mut layers = Vec::new();
        if VulkanDebug::enabled() {
            layers.push(VulkanDebug::layer_name()?.as_ptr());
        }
        Ok(layers)
    }

    fn device_extensions() -> Vec<*const c_char> {
        vec![Swapchain::name().as_ptr()]
    }

    fn features<'a>() -> vk::PhysicalDeviceFeaturesBuilder<'a> {
        vk::PhysicalDeviceFeatures::builder()
            .sample_rate_shading(true)
            .sampler_anisotropy(true)
            .fill_mode_non_solid(true)
            .wide_lines(true)
    }

    pub fn debug(&self) -> Result<&VulkanDebug> {
        self.debug
            .as_ref()
            .context("Vulkan debug object not found in Vulkan context!")
    }

    pub fn surface(&self) -> Result<&Surface> {
        self.surface.as_ref().context(
            "Surface was requested from a context that was not constructed with a surface!",
        )
    }

    pub fn physical_device_surface_capabilities(&self) -> Result<vk::SurfaceCapabilitiesKHR> {
        let surface = self.surface()?;
        let capabilities = unsafe {
            surface.handle_ash.get_physical_device_surface_capabilities(
                self.physical_device.handle,
                surface.handle_khr,
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
