use crate::vulkan::VulkanRenderBackend;
use anyhow::Result;
use dragonglass_config::Config;
use dragonglass_gui::egui::{ClippedMesh, CtxRef};
use dragonglass_world::{Viewport, World};
use raw_window_handle::HasRawWindowHandle;

pub enum Backend {
    Vulkan,
}

pub trait Renderer {
    fn load_world(&mut self, world: &World) -> Result<()>;
    // TODO: make this just take Resources instead of world, elapsed, config, etc
    fn update(
        &mut self,
        world: &World,
        gui_context: Option<&CtxRef>,
        clipped_meshes: &[ClippedMesh],
        elapsed_milliseconds: u32,
        config: &Config,
    ) -> Result<()>;
    fn render(&mut self, world: &World, clipped_meshes: Vec<ClippedMesh>) -> Result<()>;
    fn viewport(&self) -> Viewport;
    fn set_viewport(&mut self, viewport: Viewport);
}

pub fn create_render_backend(
    backend: &Backend,
    window_handle: &impl HasRawWindowHandle,
    viewport: Viewport,
) -> Result<Box<dyn Renderer>> {
    match backend {
        Backend::Vulkan => {
            let backend = VulkanRenderBackend::new(window_handle, viewport)?;
            Ok(Box::new(backend) as Box<dyn Renderer>)
        }
    }
}
