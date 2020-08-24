use anyhow::Result;
use ash::{
    extensions::ext::DebugUtils,
    vk::{
        self, Bool32, DebugUtilsMessageSeverityFlagsEXT, DebugUtilsMessageTypeFlagsEXT,
        DebugUtilsMessengerCallbackDataEXT, DebugUtilsMessengerEXT,
    },
};
use log::{debug, error, info, trace, warn};
use std::{ffi::CStr, os::raw::c_void};

pub struct DebugLayer {
    debug_utils: DebugUtils,
    debug_utils_messenger: DebugUtilsMessengerEXT,
}

impl DebugLayer {
    pub fn new(entry: &ash::Entry, instance: &ash::Instance) -> Result<Self> {
        let debug_utils = DebugUtils::new(entry, instance);

        let create_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .flags(vk::DebugUtilsMessengerCreateFlagsEXT::all())
            .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::all())
            .message_type(vk::DebugUtilsMessageTypeFlagsEXT::all())
            .pfn_user_callback(Some(vulkan_debug_callback));

        let debug_utils_messenger =
            unsafe { debug_utils.create_debug_utils_messenger(&create_info, None) }?;

        let layer = Self {
            debug_utils,
            debug_utils_messenger,
        };

        Ok(layer)
    }

    pub fn enabled() -> bool {
        cfg!(debug_assertions) || cfg!(feature = "validation")
    }

    pub fn layer_name() -> Result<&'static CStr> {
        Ok(CStr::from_bytes_with_nul(b"VK_LAYER_KHRONOS_validation\0")?)
    }

    pub fn extension_name() -> &'static CStr {
        DebugUtils::name()
    }
}

impl Drop for DebugLayer {
    fn drop(&mut self) {
        unsafe {
            self.debug_utils
                .destroy_debug_utils_messenger(self.debug_utils_messenger, None);
        }
    }
}

// Setup the callback for the debug utils extension
unsafe extern "system" fn vulkan_debug_callback(
    flags: DebugUtilsMessageSeverityFlagsEXT,
    type_flags: DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void,
) -> Bool32 {
    let type_flag = match type_flags {
        DebugUtilsMessageTypeFlagsEXT::GENERAL => "General",
        DebugUtilsMessageTypeFlagsEXT::PERFORMANCE => "Performance",
        DebugUtilsMessageTypeFlagsEXT::VALIDATION => "Validation",
        _ => "Unspecified",
    };

    let message = format!(
        "[{}] {:?}",
        type_flag,
        CStr::from_ptr((*p_callback_data).p_message)
    );

    match flags {
        DebugUtilsMessageSeverityFlagsEXT::ERROR => error!("{}", message),
        DebugUtilsMessageSeverityFlagsEXT::INFO => info!("{}", message),
        DebugUtilsMessageSeverityFlagsEXT::WARNING => warn!("{}", message),
        DebugUtilsMessageSeverityFlagsEXT::VERBOSE => trace!("{}", message),
        _ => debug!("{}", message),
    }

    vk::FALSE
}
