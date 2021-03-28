use crate::core::Device;
use anyhow::Result;
use ash::{
    extensions::ext::DebugUtils,
    version::{EntryV1_0, InstanceV1_0},
    vk::{self, Bool32, DebugUtilsMessengerEXT},
};
use log::{debug, error, info, trace, warn};
use std::{
    ffi::{c_void, CStr},
    sync::Arc,
};

pub struct VulkanDebug {
    pub debug: DebugUtils,
    messenger: DebugUtilsMessengerEXT,
    device: Arc<Device>,
}

impl VulkanDebug {
    pub fn new(
        entry: &impl EntryV1_0,
        instance: &impl InstanceV1_0,
        device: Arc<Device>,
    ) -> Result<Self> {
        let debug = DebugUtils::new(entry, instance);

        let create_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .flags(vk::DebugUtilsMessengerCreateFlagsEXT::all())
            .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::all())
            .message_type(vk::DebugUtilsMessageTypeFlagsEXT::all())
            .pfn_user_callback(Some(vulkan_debug_callback));

        let messenger = unsafe { debug.create_debug_utils_messenger(&create_info, None) }?;

        Ok(Self {
            debug,
            messenger,
            device,
        })
    }

    pub const fn enabled() -> bool {
        true
    }

    pub fn layer_name() -> Result<&'static CStr> {
        Ok(CStr::from_bytes_with_nul(b"VK_LAYER_KHRONOS_validation\0")?)
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
        let name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
            .object_type(object_type)
            .object_name(CStr::from_bytes_with_nul(object_name.as_bytes())?)
            .object_handle(handle)
            .build();
        unsafe {
            self.debug
                .debug_utils_set_object_name(self.device.handle.handle(), &name_info)?;
        }
        Ok(())
    }
}

impl Drop for VulkanDebug {
    fn drop(&mut self) {
        unsafe {
            self.debug
                .destroy_debug_utils_messenger(self.messenger, None);
        }
    }
}

unsafe extern "system" fn vulkan_debug_callback(
    flags: vk::DebugUtilsMessageSeverityFlagsEXT,
    type_flags: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void,
) -> Bool32 {
    let type_flag = match type_flags {
        vk::DebugUtilsMessageTypeFlagsEXT::GENERAL => "General",
        vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE => "Performance",
        vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION => "Validation",
        _ => "Unspecified",
    };

    let message = format!(
        "[{}] {:?}",
        type_flag,
        CStr::from_ptr((*p_callback_data).p_message)
    );

    match flags {
        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => error!("{}", message),
        vk::DebugUtilsMessageSeverityFlagsEXT::INFO => info!("{}", message),
        vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => warn!("{}", message),
        vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE => trace!("{}", message),
        _ => debug!("{}", message),
    }

    vk::FALSE
}
