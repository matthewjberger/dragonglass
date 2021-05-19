use crate::{opengl::world::WorldRender, Render};
use anyhow::{bail, Result};
use dragonglass_opengl::{
    gl,
    glutin::{ContextWrapper, PossiblyCurrent},
    load_context, GeometryBuffer,
};
use dragonglass_world::World;
use imgui::{Context as ImguiContext, DrawData};
use raw_window_handle::HasRawWindowHandle;
use std::str;

pub struct OpenGLRenderBackend {
    context: ContextWrapper<PossiblyCurrent, ()>,
    world_render: Option<WorldRender>,
}

impl OpenGLRenderBackend {
    pub fn new(
        window_handle: &impl HasRawWindowHandle,
        _dimensions: &[u32; 2],
        _imgui: &mut ImguiContext,
    ) -> Result<Self> {
        let context = unsafe { load_context(window_handle)? };
        Ok(Self {
            context,
            world_render: None,
        })
    }
}

impl Render for OpenGLRenderBackend {
    fn load_skybox(&mut self, _path: &str) -> Result<()> {
        Ok(())
    }

    fn load_world(&mut self, world: &World) -> Result<()> {
        self.world_render = Some(WorldRender::new(world)?);
        Ok(())
    }

    fn reload_asset_shaders(&mut self) -> Result<()> {
        Ok(())
    }

    fn render(
        &mut self,
        dimensions: &[u32; 2],
        world: &World,
        _draw_data: &DrawData,
    ) -> Result<()> {
        unsafe {
            gl::Viewport(0, 0, dimensions[0] as _, dimensions[1] as _);

            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
        }

        let world_render = match self.world_render.as_ref() {
            Some(result) => result,
            None => {
                self.context.swap_buffers()?;
                return Ok(());
            }
        };

        let aspect_ratio = dimensions[0] as f32 / std::cmp::max(dimensions[1], 1) as f32;
        world_render.render(world, aspect_ratio)?;

        self.context.swap_buffers()?;
        Ok(())
    }

    fn toggle_wireframe(&mut self) {}
}
