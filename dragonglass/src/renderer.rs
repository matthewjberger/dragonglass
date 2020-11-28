use anyhow::Result;
use dragonglass_scene::World;
use nalgebra_glm as glm;
use raw_window_handle::HasRawWindowHandle;

#[cfg(feature = "vulkan")]
use crate::vulkan::VulkanRenderer;

pub unsafe fn byte_slice_from<T: Sized>(data: &T) -> &[u8] {
    let data_ptr = (data as *const T) as *const u8;
    std::slice::from_raw_parts(data_ptr, std::mem::size_of::<T>())
}

pub enum Backend {
    Vulkan,
}

pub trait Renderer {
    fn toggle_wireframe(&mut self);
    fn load_skybox(&mut self, path: &str) -> Result<()>;
    fn load_world(&mut self, world: &World) -> Result<()>;
    fn render(
        &mut self,
        dimensions: &[u32; 2],
        view: glm::Mat4,
        camera_position: glm::Vec3,
        world: &World,
    ) -> Result<()>;
}

impl dyn Renderer {
    pub fn create_backend(
        backend: &Backend,
        window_handle: &impl HasRawWindowHandle,
        dimensions: &[u32; 2],
    ) -> Result<impl Renderer> {
        match backend {
            #[cfg(feature = "vulkan")]
            Backend::Vulkan => VulkanRenderer::new(window_handle, dimensions),
        }
    }
}
