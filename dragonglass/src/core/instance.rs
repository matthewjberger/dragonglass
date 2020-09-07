use super::debug::DebugLayer;
use anyhow::{anyhow, Result};
use ash::{
    version::{EntryV1_0, InstanceV1_0},
    vk::{self, make_version},
};
use ash_window::enumerate_required_extensions;
use log::info;
use raw_window_handle::HasRawWindowHandle;
use std::ffi::{CStr, CString};

pub struct Instance {
    pub handle: ash::Instance,
}

impl Instance {
    const APPLICATION_NAME: &'static str = "Dragonglass";
    const APPLICATION_VERSION: u32 = make_version(1, 0, 0);
    const API_VERSION: u32 = make_version(1, 0, 0);
    const ENGINE_VERSION: u32 = make_version(1, 0, 0);
    const ENGINE_NAME: &'static str = "Dragonglass Engine";

    pub fn new<T: HasRawWindowHandle>(entry: &ash::Entry, window_handle: &T) -> Result<Self> {
        let application_create_info = Self::application_create_info()?;
        let instance_extensions = Self::extensions(window_handle)?;
        let layers = Self::layers()?;
        Self::check_layers_supported(entry, &layers)?;

        let instance_create_info = vk::InstanceCreateInfo::builder()
            .application_info(&application_create_info)
            .enabled_extension_names(&instance_extensions)
            .enabled_layer_names(&layers);

        let handle = unsafe { entry.create_instance(&instance_create_info, None) }?;
        let instance = Instance { handle };
        Ok(instance)
    }

    fn application_create_info() -> Result<vk::ApplicationInfo> {
        let app_name = CString::new(Instance::APPLICATION_NAME)?;
        let engine_name = CString::new(Instance::ENGINE_NAME)?;
        let app_info = vk::ApplicationInfo::builder()
            .application_name(&app_name)
            .engine_name(&engine_name)
            .api_version(Instance::API_VERSION)
            .application_version(Instance::APPLICATION_VERSION)
            .engine_version(Instance::ENGINE_VERSION)
            .build();
        Ok(app_info)
    }

    fn extensions<T: HasRawWindowHandle>(window_handle: &T) -> Result<Vec<*const i8>> {
        let mut extensions: Vec<*const i8> = enumerate_required_extensions(window_handle)?
            .iter()
            .map(|extension| extension.as_ptr())
            .collect();
        if DebugLayer::enabled() {
            extensions.push(DebugLayer::extension_name().as_ptr());
        }
        Ok(extensions)
    }

    pub fn layers() -> Result<Vec<*const i8>> {
        let mut layers = Vec::new();
        if DebugLayer::enabled() {
            layers.push(DebugLayer::layer_name()?.as_ptr());
        }
        Ok(layers)
    }

    fn check_layers_supported(entry: &ash::Entry, layers: &[*const i8]) -> Result<()> {
        let supported_layers = entry.enumerate_instance_layer_properties()?;

        let supported_layer_names = supported_layers
            .iter()
            .map(|layer| layer.layer_name.as_ptr())
            .map(|name_ptr| unsafe { CStr::from_ptr(name_ptr) }.to_str())
            .collect::<Result<Vec<&str>, std::str::Utf8Error>>()?;

        info!("Supported layers: {:#?}", supported_layer_names);

        for name in layers.iter() {
            let name = unsafe { CStr::from_ptr(*name) }.to_str()?;
            if !supported_layer_names.contains(&name) {
                return Err(anyhow!("Requested layer not supported: {}", name));
            }
        }

        Ok(())
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            self.handle.destroy_instance(None);
        }
    }
}
