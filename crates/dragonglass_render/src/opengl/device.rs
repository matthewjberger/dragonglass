use crate::Render;
use anyhow::Result;
use dragonglass_opengl::{
    gl::{self, types::*},
    glutin::{ContextWrapper, PossiblyCurrent},
    load_context,
};
use dragonglass_world::World;
use imgui::{Context as ImguiContext, DrawData};
use raw_window_handle::HasRawWindowHandle;

pub struct OpenGLRenderBackend {
    context: ContextWrapper<PossiblyCurrent, ()>,
}

impl OpenGLRenderBackend {
    pub fn new(
        window_handle: &impl HasRawWindowHandle,
        _dimensions: &[u32; 2],
        _imgui: &mut ImguiContext,
    ) -> Result<Self> {
        Ok(Self {
            context: unsafe { load_context(window_handle)? },
        })
    }
}

impl Render for OpenGLRenderBackend {
    fn load_skybox(&mut self, _path: &str) -> Result<()> {
        Ok(())
    }

    fn load_world(&mut self, _world: &World) -> Result<()> {
        Ok(())
    }

    fn reload_asset_shaders(&mut self) -> Result<()> {
        Ok(())
    }

    fn render(
        &mut self,
        _dimensions: &[u32; 2],
        _world: &World,
        _draw_data: &DrawData,
    ) -> Result<()> {
        let red: [GLfloat; 4] = [1.0, 0.0, 0.0, 0.0];
        unsafe {
            gl::ClearBufferfv(gl::COLOR, 0, &red as *const f32);
        }
        self.context.swap_buffers()?;
        Ok(())
    }

    fn toggle_wireframe(&mut self) {}
}
