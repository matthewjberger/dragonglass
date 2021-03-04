use crate::vulkan::core::Device;
use anyhow::Result;
use ash::{
    extensions::ext::DebugUtils,
    version::{EntryV1_0, InstanceV1_0},
    vk,
};
use std::{ffi::CStr, sync::Arc};

pub struct VulkanDebug {
    pub debug: Option<DebugUtils>,
    device: Arc<Device>,
}

impl VulkanDebug {
    pub fn new(
        entry: &impl EntryV1_0,
        instance: &impl InstanceV1_0,
        device: Arc<Device>,
    ) -> Result<Self> {
        let debug = if Self::debug_enabled() {
            Some(DebugUtils::new(entry, instance))
        } else {
            None
        };
        Ok(Self { debug, device })
    }

    pub const fn debug_enabled() -> bool {
        true
    }

    pub fn extension_name() -> &'static CStr {
        DebugUtils::name()
    }

    pub fn name_image(&self, name: &str, handle: u64) -> Result<()> {
        self.name_object(name, handle, vk::ObjectType::IMAGE)
    }

    pub fn name_image_view(&self, name: &str, handle: u64) -> Result<()> {
        self.name_object(name, handle, vk::ObjectType::IMAGE_VIEW)
    }

    pub fn name_buffer(&self, name: &str, handle: u64) -> Result<()> {
        self.name_object(name, handle, vk::ObjectType::BUFFER)
    }

    pub fn name_framebuffer(&self, name: &str, handle: u64) -> Result<()> {
        self.name_object(name, handle, vk::ObjectType::FRAMEBUFFER)
    }

    pub fn name_semaphore(&self, name: &str, handle: u64) -> Result<()> {
        self.name_object(name, handle, vk::ObjectType::SEMAPHORE)
    }

    pub fn name_fence(&self, name: &str, handle: u64) -> Result<()> {
        self.name_object(name, handle, vk::ObjectType::FENCE)
    }

    pub fn name_object(&self, name: &str, handle: u64, object_type: vk::ObjectType) -> Result<()> {
        let object_name = format!("{}\0", name);
        if let Some(debug) = self.debug.as_ref() {
            let name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                .object_type(object_type)
                .object_name(CStr::from_bytes_with_nul(object_name.as_bytes())?)
                .object_handle(handle)
                .build();
            unsafe {
                debug.debug_utils_set_object_name(self.device.handle.handle(), &name_info)?;
            }
        }
        Ok(())
    }
}
