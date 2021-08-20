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

pub trait Render {
    fn toggle_wireframe(&mut self);
    fn load_world(&mut self, world: &World) -> Result<()>;
    fn reload_asset_shaders(&mut self) -> Result<()>;
    fn render(&mut self, dimensions: &[u32; 2], world: &World, draw_data: &DrawData) -> Result<()>;
}

pub fn create_render_backend(
    _window_handle: &impl HasRawWindowHandle,
    _dimensions: &[u32; 2],
    _imgui: &mut ImguiContext,
) -> Result<Box<dyn Render>> {
    unimplemented!("no backend available yet!")
}
