use crate::vulkan::VulkanRenderBackend;
use dragonglass_deps::{
    anyhow::Result,
    imgui::{Context as ImguiContext, DrawData},
    raw_window_handle::HasRawWindowHandle,
};
use dragonglass_world::World;

pub enum Backend {
    Vulkan,
}

pub trait Render {
    fn toggle_wireframe(&mut self);
    fn load_world(&mut self, world: &World) -> Result<()>;
    fn reload_asset_shaders(&mut self) -> Result<()>;
    fn render(&mut self, dimensions: &[u32; 2], world: &World, draw_data: &DrawData) -> Result<()>;
}

pub fn create_render_backend(
    backend: &Backend,
    window_handle: &impl HasRawWindowHandle,
    dimensions: &[u32; 2],
    imgui: &mut ImguiContext,
) -> Result<Box<dyn Render>> {
    match backend {
        Backend::Vulkan => {
            let backend = VulkanRenderBackend::new(window_handle, dimensions, imgui)?;
            Ok(Box::new(backend) as Box<dyn Render>)
        }
    }
}
