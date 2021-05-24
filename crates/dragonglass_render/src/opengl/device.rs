use crate::{opengl::world::WorldRender, Render};
use anyhow::{bail, Result};
use dragonglass_opengl::{
    gl,
    glutin::{ContextWrapper, PossiblyCurrent},
    load_context, GeometryBuffer, ShaderProgram,
};
use dragonglass_world::World;
use imgui::{Context as ImguiContext, DrawData};
use raw_window_handle::HasRawWindowHandle;
use std::str;
use nalgebra_glm as glm;

pub struct OpenGLRenderBackend {
    context: ContextWrapper<PossiblyCurrent, ()>,
    offscreen: OffscreenFramebuffer,
    fullscreen_program: ShaderProgram,
    fullscreen_quad: QuadGeometry,
    world_render: Option<WorldRender>,
    current_dimensions: glm::Vec2,
}

impl OpenGLRenderBackend {
    pub fn new(
        window_handle: &impl HasRawWindowHandle,
        dimensions: &[u32; 2],
        _imgui: &mut ImguiContext,
    ) -> Result<Self> {
        let context = unsafe { load_context(window_handle)? };
        Ok(Self {
            context,
            offscreen: OffscreenFramebuffer::new(dimensions[0] as i32, dimensions[1] as i32)?,
            fullscreen_program: fullscreen_shader_program()?,
            fullscreen_quad: QuadGeometry::new(),
            world_render: None,
            current_dimensions: glm::vec2(dimensions[0] as _, dimensions[1] as _),
        })
    }
}

impl Render for OpenGLRenderBackend {
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
        // TODO: clean this up...
        let dimensions = glm::vec2(dimensions[0] as _, dimensions[1] as _);
        if self.current_dimensions != dimensions {
            unsafe {
                gl::Viewport(0, 0, dimensions[0] as _, dimensions[1] as _);
            }
            self.offscreen =
                OffscreenFramebuffer::new(dimensions.x as _, dimensions.y as _)?;
            self.current_dimensions = dimensions;
        }

        unsafe {
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
        }

        // First pass
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.offscreen.framebuffer);
            gl::Enable(gl::DEPTH_TEST);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
            gl::Enable(gl::DEPTH_TEST);
        }

        if let Some(world_render) = self.world_render.as_ref() {
            let aspect_ratio = dimensions.x / std::cmp::max(dimensions.y as u32, 1) as f32;
            world_render.render(world, aspect_ratio)?;
        }

        // Second pass
        self.fullscreen_program.use_program();
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0); // default framebuffer
            gl::Disable(gl::DEPTH_TEST);
            gl::ClearColor(1.0, 1.0, 1.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.offscreen.color_attachments[0]);
            self.fullscreen_quad.draw();
        }

        self.context.swap_buffers()?;
        Ok(())
    }

    fn toggle_wireframe(&mut self) {}
}

struct OffscreenFramebuffer {
    pub framebuffer: u32,
    pub color_attachments: Vec<u32>,
    pub depth_rbo: u32,
}

impl OffscreenFramebuffer {
    pub fn new(screen_width: i32, screen_height: i32) -> Result<Self> {
        unsafe {
            // Offscreen framebuffer
            let mut framebuffer = 0;
            gl::GenFramebuffers(1, &mut framebuffer);
            gl::BindFramebuffer(gl::FRAMEBUFFER, framebuffer);

            // Color attachments for the offscreen framebuffer
            let mut color_attachments = Vec::new();
            let number_of_color_attachments = 2;
            for index in 0..number_of_color_attachments {
                let mut color_attachment = 0;
                gl::GenTextures(1, &mut color_attachment);
                gl::BindTexture(gl::TEXTURE_2D, color_attachment);
                gl::TexImage2D(
                    gl::TEXTURE_2D,
                    0,
                    gl::RGBA16F as _,
                    screen_width,
                    screen_height,
                    0,
                    gl::RGBA,
                    gl::FLOAT,
                    std::ptr::null(),
                );
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);

                // Attach the color texture
                gl::FramebufferTexture2D(
                    gl::FRAMEBUFFER,
                    gl::COLOR_ATTACHMENT0 + index,
                    gl::TEXTURE_2D,
                    color_attachment,
                    0,
                );

                color_attachments.push(color_attachment);
            }

            let attachments = (0..color_attachments.len()).map(|i| gl::COLOR_ATTACHMENT0 + i as u32).collect::<Vec<_>>();
            gl::DrawBuffers(attachments.len() as _, attachments.as_ptr() as *const _);

            // Renderbuffer object for the Depth/Stencil attachment of the offscreen framebuffer
            let mut depth_rbo = 0;
            gl::GenRenderbuffers(1, &mut depth_rbo);
            gl::BindRenderbuffer(gl::RENDERBUFFER, depth_rbo);
            gl::RenderbufferStorage(
                gl::RENDERBUFFER,
                gl::DEPTH24_STENCIL8,
                screen_width,
                screen_width,
            );

            // Attach the renderbuffer object to the Depth/Stencil attachment of the offscreen framebuffer
            gl::FramebufferRenderbuffer(
                gl::FRAMEBUFFER,
                gl::DEPTH_STENCIL_ATTACHMENT,
                gl::RENDERBUFFER,
                depth_rbo,
            );

            if gl::CheckFramebufferStatus(gl::FRAMEBUFFER) != gl::FRAMEBUFFER_COMPLETE {
                bail!("Offscreen framebuffer is not complete!")
            }

            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);

            Ok(Self {
                framebuffer,
                color_attachments,
                depth_rbo,
            })
        }
    }
}

fn fullscreen_shader_program() -> Result<ShaderProgram> {
    const VERTEX_SHADER_SOURCE: &'static str = &r#"
#version 450 core
layout (location = 0) in vec3 aPos;
layout (location = 1) in vec2 aTexCoords;

out vec2 TexCoords;

void main()
{
    gl_Position = vec4(aPos.x, aPos.y, 0.0, 1.0); 
    TexCoords = aTexCoords;
}  
"#;

    const FRAGMENT_SHADER_SOURCE: &'static str = &r#"
#version 450 core

out vec4 FragColor;
    
in vec2 TexCoords;

uniform sampler2D screenTexture;

void main()
{ 
    // Normal
    FragColor = texture(screenTexture, TexCoords);

    // Invert colors
    // FragColor = vec4(vec3(1.0 - texture(screenTexture, TexCoords)), 1.0);

    // Grayscale
    // FragColor = texture(screenTexture, TexCoords);
    // float average = 0.2126 * FragColor.r + 0.7152 * FragColor.g + 0.0722 * FragColor.b;
    // FragColor = vec4(average, average, average, 1.0);

    /* Kernels */

    // Sharpen Kernel
    // float kernel[9] = float[](
    //     -1, -1, -1,
    //     -1,  9, -1,
    //     -1, -1, -1
    // );

    // Blur Kernel
    // float kernel[9] = float[](
    //     1.0 / 16, 2.0 / 16, 1.0 / 16,
    //     2.0 / 16, 4.0 / 16, 2.0 / 16,
    //     1.0 / 16, 2.0 / 16, 1.0 / 16  
    // );   

    // Edge Detection
    // float kernel[9] = float[](
    //     1, 1, 1,
    //     1, -8, 1,
    //     1, 1, 1
    // );

    // Apply a kernel
    // const float offset = 1.0 / 300.0;  
    // vec2 offsets[9] = vec2[](
    //     vec2(-offset,  offset), // top-left
    //     vec2( 0.0f,    offset), // top-center
    //     vec2( offset,  offset), // top-right
    //     vec2(-offset,  0.0f),   // center-left
    //     vec2( 0.0f,    0.0f),   // center-center
    //     vec2( offset,  0.0f),   // center-right
    //     vec2(-offset, -offset), // bottom-left
    //     vec2( 0.0f,   -offset), // bottom-center
    //     vec2( offset, -offset)  // bottom-right    
    // );
    // vec3 sampleTex[9];
    // for(int i = 0; i < 9; i++)
    // {
    //     sampleTex[i] = vec3(texture(screenTexture, TexCoords.st + offsets[i]));
    // }
    // vec3 col = vec3(0.0);
    // for(int i = 0; i < 9; i++)
    //     col += sampleTex[i] * kernel[i];
    // FragColor = vec4(col, 1.0);

    const float gamma = 2.2;
    const float exposure = 1.0;

    // tone mapping
    vec3 result = vec3(1.0) - exp(-FragColor.rgb * exposure);

    // gamma correct
    result = pow(result, vec3(1.0 / gamma));

    FragColor = vec4(result, 1.0);
}
"#;

    let mut shader_program = ShaderProgram::new();
    shader_program
        .vertex_shader_source(VERTEX_SHADER_SOURCE)?
        .fragment_shader_source(FRAGMENT_SHADER_SOURCE)?
        .link();

    Ok(shader_program)
}

#[derive(Copy, Clone)]
struct Vertex {
    pub position: glm::Vec3,
    pub tex_coords: glm::Vec2,
}

impl Vertex {
    pub fn new(position: glm::Vec3, tex_coords: glm::Vec2) -> Self {
        Self {
            position,
            tex_coords,
        }
    }
}

// Renders a 1x1 XY quad in NDC
struct QuadGeometry {
    geometry: GeometryBuffer,
}

impl QuadGeometry {
    pub fn new() -> Self {
        #[rustfmt::skip]
        let vertices = vec![
            Vertex::new(glm::vec3(-1.0,  1.0, 0.0), glm::vec2(0.0, 1.0)), 
            Vertex::new(glm::vec3(-1.0, -1.0, 0.0), glm::vec2(0.0, 0.0)), 
            Vertex::new(glm::vec3( 1.0, -1.0, 0.0), glm::vec2(1.0, 0.0)), 
            Vertex::new(glm::vec3(-1.0,  1.0, 0.0),  glm::vec2(0.0, 1.0)),
            Vertex::new(glm::vec3( 1.0, -1.0, 0.0),  glm::vec2(1.0, 0.0)),
            Vertex::new(glm::vec3( 1.0,  1.0, 0.0,), glm::vec2(1.0, 1.0)),
        ];
        Self {
            geometry: GeometryBuffer::new(&vertices, None, &[3, 2]),
        }
    }

    pub fn draw(&self) {
        self.geometry.bind();
        unsafe {
            gl::DrawArrays(gl::TRIANGLES, 0, 6);
        }
    }
}
