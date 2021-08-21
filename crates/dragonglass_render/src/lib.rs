use anyhow::Result;
use dragonglass_world::World;
use imgui::{Context as ImguiContext, DrawData};
use raw_window_handle::HasRawWindowHandle;

#[cfg(target_os = "windows")]
const _BACKEND: wgpu::Backend = wgpu::Backend::Dx12;

#[cfg(target_os = "macos")]
const _BACKEND: wgpu::Backend = wgpu::Backend::Metal;

#[cfg(target_os = "linux")]
const _BACKEND: wgpu::Backend = wgpu::Backend::Vulkan;

pub struct Renderer {}

impl Renderer {
    pub async fn new(
        _window_handle: &impl HasRawWindowHandle,
        _dimensions: &[u32; 2],
        _imgui: &mut ImguiContext,
    ) -> Result<Self> {
        Ok(Self {})
    }

    pub fn toggle_wireframe(&mut self) {}

    pub fn load_world(&mut self, _world: &World) -> Result<()> {
        Ok(())
    }

    pub fn reload_asset_shaders(&mut self) -> Result<()> {
        Ok(())
    }

    pub fn render(
        &mut self,
        _dimensions: &[u32; 2],
        _world: &World,
        _draw_data: &DrawData,
    ) -> Result<()> {
        Ok(())
    }
}
