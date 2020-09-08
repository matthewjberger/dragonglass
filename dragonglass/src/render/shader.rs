use crate::core::LogicalDevice;
use anyhow::{anyhow, Result};
use ash::{version::DeviceV1_0, vk};
use derive_builder::Builder;
use std::{
    collections::HashMap,
    ffi::CStr,
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct Shader {
    pub module: vk::ShaderModule,
    device: Arc<LogicalDevice>,
}

impl Shader {
    pub fn new(
        device: Arc<LogicalDevice>,
        create_info: vk::ShaderModuleCreateInfoBuilder,
    ) -> Result<Self> {
        let module = unsafe { device.handle.create_shader_module(&create_info, None)? };
        let shader = Self { module, device };
        Ok(shader)
    }

    pub fn from_file<P: AsRef<Path> + Into<PathBuf>>(
        path: P,
        device: Arc<LogicalDevice>,
    ) -> Result<Self> {
        let mut shader_file = std::fs::File::open(path)?;
        let shader_source = ash::util::read_spv(&mut shader_file)?;
        let create_info = vk::ShaderModuleCreateInfo::builder().code(&shader_source);
        Self::new(device.clone(), create_info)
    }
}

impl Drop for Shader {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_shader_module(self.module, None);
        }
    }
}

#[derive(Default, Clone)]
pub struct ShaderSet {
    pub vertex: Option<Arc<Shader>>,
    pub fragment: Option<Arc<Shader>>,
    pub geometry: Option<Arc<Shader>>,
    pub tessellation_evaluation: Option<Arc<Shader>>,
    pub tessellation_control: Option<Arc<Shader>>,
    pub compute: Option<Arc<Shader>>,
}

impl ShaderSet {
    pub fn entry_point_name() -> Result<&'static CStr> {
        Ok(CStr::from_bytes_with_nul(b"main\0")?)
    }

    // TODO: Use a macro to fill this out
    pub fn stages(&self) -> Result<Vec<vk::PipelineShaderStageCreateInfo>> {
        let mut state_info_vec = Vec::new();

        if let Some(vertex_shader) = self.vertex.as_ref() {
            let state_info = vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex_shader.module)
                .name(Self::entry_point_name()?)
                .build();
            state_info_vec.push(state_info);
        }

        if let Some(fragment_shader) = self.fragment.as_ref() {
            let state_info = vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(fragment_shader.module)
                .name(Self::entry_point_name()?)
                .build();
            state_info_vec.push(state_info);
        }

        Ok(state_info_vec)
    }
}

#[derive(Builder, Clone, Default)]
#[builder(default, setter(into, strip_option))]
pub struct ShaderPathSet {
    pub vertex: Option<String>,
    pub fragment: Option<String>,
    pub geometry: Option<String>,
    pub tessellation_evaluation: Option<String>,
    pub tessellation_control: Option<String>,
    pub compute: Option<String>,
}

#[derive(Default)]
pub struct ShaderCache {
    pub shaders: HashMap<String, Arc<Shader>>,
}

impl ShaderCache {
    pub fn load_shader<P: AsRef<Path> + Into<PathBuf>>(
        &mut self,
        path: P,
        device: Arc<LogicalDevice>,
    ) -> Result<Arc<Shader>> {
        let shader_path = path
            .as_ref()
            .to_str()
            .ok_or(anyhow!("The shader path is not a valid UTF-8 sequence"))?
            .to_string();
        let shader = self
            .shaders
            .entry(shader_path)
            .or_insert(Arc::new(Shader::from_file(path, device)?))
            .clone();
        Ok(shader)
    }
}

macro_rules! impl_create_shader_set {
    ($( $field:ident ),*) => {
        impl ShaderCache {
            pub fn create_shader_set(
                &mut self,
                device: Arc<LogicalDevice>,
                shader_paths: &ShaderPathSet,
            ) -> Result<ShaderSet> {
                let mut shader_set = ShaderSet::default();
                $(
                    if let Some(shader_path) = shader_paths.$field.as_ref() {
                        let shader = self.load_shader(
                            &shader_path,
                            device.clone(),
                        )?;
                        shader_set.$field = Some(shader);
                    }
                )*
                Ok(shader_set)
            }
        }
    };
}

impl_create_shader_set!(
    vertex,
    fragment,
    geometry,
    tessellation_control,
    tessellation_evaluation,
    compute
);
