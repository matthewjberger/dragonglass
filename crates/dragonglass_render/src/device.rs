use anyhow::Result;
use dragonglass_physics::PhysicsWorld;
use dragonglass_world::{Ecs, World};
use imgui::{Context as ImguiContext, DrawData};
use ncollide3d::world::CollisionWorld;
use raw_window_handle::HasRawWindowHandle;

#[cfg(feature = "vulkan")]
use crate::vulkan::VulkanRenderBackend;

pub enum Backend {
    Vulkan,
}

pub trait Render {
    fn toggle_wireframe(&mut self);
    // TODO: Make this part of the world
    fn load_skybox(&mut self, path: &str) -> Result<()>;
    fn load_world(&mut self, world: &World) -> Result<()>;
    fn reload_asset_shaders(&mut self) -> Result<()>;
    fn render(
        &mut self,
        dimensions: &[u32; 2],
        ecs: &mut Ecs,
        world: &World,
        physics_world: &PhysicsWorld,
        collision_world: &CollisionWorld<f32, ()>,
        draw_data: &DrawData,
    ) -> Result<()>;
}

impl dyn Render {
    pub fn create_backend(
        backend: &Backend,
        window_handle: &impl HasRawWindowHandle,
        dimensions: &[u32; 2],
        imgui: &mut ImguiContext,
    ) -> Result<impl Render> {
        match backend {
            #[cfg(feature = "vulkan")]
            Backend::Vulkan => VulkanRenderBackend::new(window_handle, dimensions, imgui),
        }
    }
}
