use anyhow::Result;
use dragonglass_world::World;
use imgui::{Context as ImguiContext, DrawData};
use raw_window_handle::HasRawWindowHandle;

#[cfg(feature = "vulkan")]
use crate::vulkan::VulkanRenderBackend;

#[cfg(feature = "opengl")]
use crate::opengl::OpenGLRenderBackend;

pub enum Backend {
    #[cfg(feature = "vulkan")]
    Vulkan,

    #[cfg(feature = "opengl")]
    OpenGL,
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
        #[cfg(feature = "vulkan")]
        Backend::Vulkan => {
            let backend = VulkanRenderBackend::new(window_handle, dimensions, imgui)?;
            Ok(Box::new(backend) as Box<dyn Render>)
        }

        #[cfg(feature = "opengl")]
        Backend::OpenGL => {
            let backend = OpenGLRenderBackend::new(window_handle, dimensions, imgui)?;
            Ok(Box::new(backend) as Box<dyn Render>)
        }
    }
}
