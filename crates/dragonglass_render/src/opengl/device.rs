use crate::Render;
use anyhow::Result;
use dragonglass_world::{legion::EntityStore, Camera, PerspectiveCamera, World};
use imgui::{Context as ImguiContext, DrawData};
use log::{error, info};
use nalgebra_glm as glm;
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;

pub struct OpenGLRenderBackend {}

impl OpenGLRenderBackend {
    const MAX_FRAMES_IN_FLIGHT: usize = 2;

    pub fn new(
        window_handle: &impl HasRawWindowHandle,
        dimensions: &[u32; 2],
        imgui: &mut ImguiContext,
    ) -> Result<Self> {
        Ok(Self {})
    }
}

impl Render for OpenGLRenderBackend {
    fn load_skybox(&mut self, path: &str) -> Result<()> {
        Ok(())
    }

    fn load_world(&mut self, world: &World) -> Result<()> {
        Ok(())
    }

    fn reload_asset_shaders(&mut self) -> Result<()> {
        Ok(())
    }

    fn render(&mut self, dimensions: &[u32; 2], world: &World, draw_data: &DrawData) -> Result<()> {
        Ok(())
    }

    fn toggle_wireframe(&mut self) {}
}
