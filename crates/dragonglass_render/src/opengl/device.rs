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
    blur_program: ShaderProgram,
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
            blur_program: blur_shader_program()?,
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

        // Blur the second color attachment for bloom
        let mut horizontal = true;
        let mut first_iteration = true;
        let amount = 10;
        self.blur_program.use_program();
        self.blur_program.set_uniform_int("image", 0);
        for _ in 0..amount {
            unsafe {
                gl::BindFramebuffer(gl::FRAMEBUFFER, self.offscreen.pingpong_framebuffers[if horizontal { 1 } else { 0 }]);
                self.blur_program.set_uniform_bool("horizontal", horizontal);
                gl::ActiveTexture(gl::TEXTURE0);
                gl::BindTexture(gl::TEXTURE_2D, if first_iteration { self.offscreen.color_attachments[1] } else { self.offscreen.pingpong_textures[if horizontal { 0 } else { 1 }] });
                self.fullscreen_quad.draw();
                horizontal = !horizontal;
                first_iteration = false;
            }
        }

        // Second pass
        self.fullscreen_program.use_program();
        self.fullscreen_program.set_uniform_int("screenTexture", 0);
        self.fullscreen_program.set_uniform_int("blurredScreenTexture", 1);
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0); // default framebuffer
            gl::Disable(gl::DEPTH_TEST);
            gl::ClearColor(1.0, 1.0, 1.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.offscreen.color_attachments[0]);
            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, self.offscreen.color_attachments[1]);
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
    pub pingpong_framebuffers: [u32; 2],
    pub pingpong_textures: [u32; 2],
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

            let mut pingpong_framebuffers = [0_u32; 2];
            let mut pingpong_textures =[0_u32; 2];

            gl::GenFramebuffers(2, pingpong_framebuffers.as_mut_ptr() as *mut _);
            gl::GenTextures(2, pingpong_textures.as_mut_ptr() as *mut _);

            for (fbo, texture) in pingpong_framebuffers.iter().zip(pingpong_textures.iter()) {
                gl::BindFramebuffer(gl::FRAMEBUFFER, *fbo);
                gl::BindTexture(gl::TEXTURE_2D, *texture);
                gl::TexImage2D(
                    gl::TEXTURE_2D, 0, gl::RGBA16F as _, screen_width, screen_height, 0, gl::RGBA, gl::FLOAT, std::ptr::null(),
                );
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);

                gl::FramebufferTexture2D(
                    gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, *fbo, 0
                );
            }

            Ok(Self {
                framebuffer,
                color_attachments,
                depth_rbo,
                pingpong_framebuffers,
                pingpong_textures,
            })
        }
    }
}

fn blur_shader_program() -> Result<ShaderProgram> {
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

uniform sampler2D image;

uniform bool horizontal;
uniform float weight[5] = float[] (0.2270270270, 0.1945945946, 0.1216216216, 0.0540540541, 0.0162162162);

void main()
{             
    vec2 tex_offset = 1.0 / textureSize(image, 0); // gets size of single texel
    vec3 result = texture(image, TexCoords).rgb * weight[0];
    if(horizontal)
    {
        for(int i = 1; i < 5; ++i)
        {
        result += texture(image, TexCoords + vec2(tex_offset.x * i, 0.0)).rgb * weight[i];
        result += texture(image, TexCoords - vec2(tex_offset.x * i, 0.0)).rgb * weight[i];
        }
    }
    else
    {
        for(int i = 1; i < 5; ++i)
        {
            result += texture(image, TexCoords + vec2(0.0, tex_offset.y * i)).rgb * weight[i];
            result += texture(image, TexCoords - vec2(0.0, tex_offset.y * i)).rgb * weight[i];
        }
    }
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
uniform sampler2D blurredScreenTexture;

void main()
{ 
    const float gamma = 2.2;
    const float exposure = 1.0;

    vec3 hdrColor = texture(screenTexture, TexCoords).rgb;
    vec3 bloomColor = texture(blurredScreenTexture, TexCoords).rgb;

    hdrColor += bloomColor;

    // tone mapping
    vec3 result = vec3(1.0) - exp(-hdrColor * exposure);

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
