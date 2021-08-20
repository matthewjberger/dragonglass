use anyhow::Result;
use dragonglass_world::World;
use imgui::{Context as ImguiContext, DrawData};
use raw_window_handle::HasRawWindowHandle;

#[cfg(target_os = "windows")]
const BACKEND: wgpu::BackendBit = wgpu::BackendBit::DX12;

#[cfg(target_os = "macos")]
const BACKEND: wgpu::BackendBit = wgpu::BackendBit::METAL;

#[cfg(target_os = "linux")]
const BACKEND: wgpu::BackendBit = wgpu::BackendBit::VULKAN;

pub trait Render {
    fn toggle_wireframe(&mut self);
    fn load_world(&mut self, world: &World) -> Result<()>;
    fn reload_asset_shaders(&mut self) -> Result<()>;
    fn render(&mut self, dimensions: &[u32; 2], world: &World, draw_data: &DrawData) -> Result<()>;
}

pub fn create_render_backend(
    window_handle: &impl HasRawWindowHandle,
    dimensions: &[u32; 2],
    imgui: &mut ImguiContext,
) -> Result<Box<dyn Render>> {
    unimplemented!("no backend available yet!")
}
