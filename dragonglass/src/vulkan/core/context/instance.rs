use anyhow::{anyhow, Result};
use ash::{
    version::{EntryV1_0, InstanceV1_0},
    vk::{self, make_version},
};
use log::info;
use std::ffi::{CStr, CString};

pub struct Instance {
    pub handle: ash::Instance,
}

impl Instance {
    const APPLICATION_NAME: &'static str = "Dragonglass";
    const APPLICATION_VERSION: u32 = make_version(1, 0, 0);
    const API_VERSION: u32 = make_version(1, 2, 0);
    const ENGINE_VERSION: u32 = make_version(1, 0, 0);
    const ENGINE_NAME: &'static str = "Dragonglass Engine";

    pub fn new(
        entry: &ash::Entry,
        extensions: Vec<*const i8>,
        layers: Vec<*const i8>,
    ) -> Result<Self> {
        let application_create_info = Self::application_create_info()?;
        Self::check_layers_supported(entry, &layers)?;

        let instance_create_info = vk::InstanceCreateInfo::builder()
            .application_info(&application_create_info)
            .enabled_extension_names(&extensions)
            .enabled_layer_names(&layers);

        let handle = unsafe { entry.create_instance(&instance_create_info, None) }?;
        Ok(Self { handle })
    }

    fn application_create_info() -> Result<vk::ApplicationInfo> {
        let app_name = CString::new(Self::APPLICATION_NAME)?;
        let engine_name = CString::new(Self::ENGINE_NAME)?;
        let app_info = vk::ApplicationInfo::builder()
            .application_name(&app_name)
            .engine_name(&engine_name)
            .api_version(Self::API_VERSION)
            .application_version(Self::APPLICATION_VERSION)
            .engine_version(Self::ENGINE_VERSION)
            .build();
        Ok(app_info)
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
