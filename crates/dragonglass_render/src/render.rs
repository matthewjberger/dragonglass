use crate::vulkan::VulkanRenderBackend;
use anyhow::Result;
use dragonglass_world::World;
use raw_window_handle::HasRawWindowHandle;

pub enum Backend {
    Vulkan,
}

pub trait Render {
    fn toggle_wireframe(&mut self);
    fn load_world(&mut self, world: &World) -> Result<()>;
    fn reload_asset_shaders(&mut self) -> Result<()>;
    fn render(&mut self, dimensions: &[u32; 2], world: &World) -> Result<()>;
}

pub fn create_render_backend(
    backend: &Backend,
    window_handle: &impl HasRawWindowHandle,
    dimensions: &[u32; 2],
) -> Result<Box<dyn Render>> {
    match backend {
        Backend::Vulkan => {
            let backend = VulkanRenderBackend::new(window_handle, dimensions)?;
            Ok(Box::new(backend) as Box<dyn Render>)
        }
    }
}
