use crate::core::LogicalDevice;
use anyhow::Result;
use ash::{version::DeviceV1_0, vk};
use std::ffi::CStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Copy, Clone)]
pub struct ShaderSet;

pub struct Shader {
    device: Arc<LogicalDevice>,
    module: vk::ShaderModule,
}

impl Shader {
    pub const SHADER_ENTRY_POINT_NAME: &'static str = "main";

    pub fn new(
        device: Arc<LogicalDevice>,
        create_info: vk::ShaderModuleCreateInfoBuilder,
    ) -> Result<Self> {
        let module = unsafe { device.handle.create_shader_module(&create_info, None)? };
        let shader = Self { device, module };
        Ok(shader)
    }

    pub fn from_file<P: AsRef<Path> + Into<PathBuf>>(
        path: P,
        flags: vk::ShaderStageFlags,
        device: Arc<LogicalDevice>,
    ) -> Result<Self> {
        let mut shader_file = std::fs::File::open(path)?;
        let shader_source = ash::util::read_spv(&mut shader_file)?;
        let create_info = vk::ShaderModuleCreateInfo::builder().code(&shader_source);
        Self::new(device.clone(), create_info)
    }

    pub fn entry_point_name() -> Result<&'static CStr> {
        Ok(CStr::from_bytes_with_nul(b"main")?)
    }
}

impl Drop for Shader {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_shader_module(self.module, None);
        }
    }
}
