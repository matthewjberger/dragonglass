use crate::{vulkan::scene::Scene, Renderer};
use anyhow::Result;
use dragonglass_gui::egui::{ClippedMesh, CtxRef};
use dragonglass_vulkan::core::{Context, Frame};
use dragonglass_world::{Viewport, World};
use log::error;
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;

pub struct VulkanRenderBackend {
    viewport: Viewport,
    frame: Frame,
    scene: Scene,
    context: Arc<Context>,
}

impl VulkanRenderBackend {
    const MAX_FRAMES_IN_FLIGHT: usize = 2;

    pub fn new(window_handle: &impl HasRawWindowHandle, viewport: Viewport) -> Result<Self> {
        let context = Arc::new(Context::new(window_handle)?);
        let frame = Frame::new(context.clone(), viewport, Self::MAX_FRAMES_IN_FLIGHT)?;
        let scene = Scene::new(
            context.clone(),
            frame.swapchain()?,
            &frame.swapchain_properties,
        )?;
        let renderer = Self {
            viewport,
            frame,
            scene,
            context,
        };
        Ok(renderer)
    }
}

impl Renderer for VulkanRenderBackend {
    fn load_world(&mut self, world: &World) -> Result<()> {
        self.scene.load_world(world)?;
        Ok(())
    }

    fn update(
        &mut self,
        world: &World,
        gui_context: Option<&CtxRef>,
        clipped_meshes: &[ClippedMesh],
        elapsed_milliseconds: u32,
    ) -> Result<()> {
        let aspect_ratio = self.frame.swapchain_properties.aspect_ratio();
        self.scene.update(
            &world,
            aspect_ratio,
            gui_context,
            &clipped_meshes,
            elapsed_milliseconds,
        )?;
        Ok(())
    }

    fn render(&mut self, world: &World, clipped_meshes: Vec<ClippedMesh>) -> Result<()> {
        let Self { frame, scene, .. } = self;

        let aspect_ratio = frame.swapchain_properties.aspect_ratio();
        let viewport = self.viewport;
        frame.render(viewport, |command_buffer, image_index| {
            // TODO: Make this take less parameters...
            scene.execute_passes(
                command_buffer,
                &world,
                image_index,
                aspect_ratio,
                viewport,
                &clipped_meshes,
            )
        })?;

        if frame.recreated_swapchain {
            scene.recreate_rendergraph(frame.swapchain()?, &frame.swapchain_properties)?;
        }

        Ok(())
    }

    fn viewport(&self) -> Viewport {
        self.viewport
    }

    fn set_viewport(&mut self, viewport: Viewport) {
        self.viewport = viewport;
    }
}

impl Drop for VulkanRenderBackend {
    fn drop(&mut self) {
        unsafe {
            if let Err(error) = self.context.device.handle.device_wait_idle() {
                error!("{}", error);
            }
        }
    }
}
