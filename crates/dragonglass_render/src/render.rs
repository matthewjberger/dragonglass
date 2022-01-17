use crate::vulkan::VulkanRenderBackend;
use anyhow::Result;
use dragonglass_gui::egui::{ClippedMesh, CtxRef};
use dragonglass_world::{Viewport, World};
use raw_window_handle::HasRawWindowHandle;

pub enum Backend {
    Vulkan,
}

pub trait Renderer {
    fn load_world(&mut self, world: &World) -> Result<()>;
    fn render(
        &mut self,
        dimensions: &[u32; 2],
        world: &World,
        context: &CtxRef,
        clipped_meshes: Vec<ClippedMesh>,
    ) -> Result<()>;
    fn viewport(&self) -> Viewport;
    fn set_viewport(&mut self, viewport: Viewport);
}

pub fn create_render_backend(
    backend: &Backend,
    window_handle: &impl HasRawWindowHandle,
    dimensions: &[u32; 2],
) -> Result<Box<dyn Renderer>> {
    match backend {
        Backend::Vulkan => {
            let backend = VulkanRenderBackend::new(window_handle, dimensions)?;
            Ok(Box::new(backend) as Box<dyn Renderer>)
        }
    }
}
