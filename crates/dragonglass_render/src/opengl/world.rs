use anyhow::Result;
use dragonglass_opengl::{gl, GeometryBuffer, ShaderProgram, Texture};
use dragonglass_world::{Format, World};
use std::str;

pub struct WorldRender {
    pub geometry: GeometryBuffer,
    pub shader_program: ShaderProgram,
    pub textures: Vec<Texture>,
}

impl WorldRender {
    const VERTEX_SHADER_SOURCE: &'static str = &r#"
#version 450 core
layout (location = 0) in vec3 inPosition;
layout (location = 1) in vec3 inNormal;
layout (location = 2) in vec2 inUV0;
layout (location = 3) in vec2 inUV1;
layout (location = 4) in vec4 inJoint0;
layout (location = 5) in vec4 inWeight0;
layout (location = 6) in vec3 inColor0;

uniform mat4 mvpMatrix;

out vec2 outUV0;

void main()
{
   gl_Position = mvpMatrix * vec4(inPosition, 1.0f);
   outUV0 = inUV0;
}
"#;

    const FRAGMENT_SHADER_SOURCE: &'static str = &r#"
#version 450 core

uniform sampler2D diffuseTexture;

in vec2 outUV0;

out vec4 color;

void main(void)
{
  color = texture(diffuseTexture, outUV0);
}
"#;

    pub fn new(world: &World) -> Result<Self> {
        let geometry = GeometryBuffer::new(
            &world.geometry.vertices,
            &world.geometry.indices,
            &[3, 3, 2, 2, 4, 4, 3],
        );

        let mut shader_program = ShaderProgram::new();
        shader_program
            .vertex_shader_source(Self::VERTEX_SHADER_SOURCE)?
            .fragment_shader_source(Self::FRAGMENT_SHADER_SOURCE)?
            .link();

        let textures = world
            .textures
            .iter()
            .map(Self::map_world_texture)
            .collect::<Vec<_>>();

        Ok(Self {
            geometry,
            shader_program,
            textures,
        })
    }

    fn map_world_texture(
        world_texture: &dragonglass_world::Texture,
    ) -> dragonglass_opengl::Texture {
        let pixel_format = match world_texture.format {
            Format::R8 => gl::R8,
            Format::R8G8 => gl::RG,
            Format::R8G8B8 => gl::RGB,
            Format::R8G8B8A8 => gl::RGBA,
            Format::B8G8R8 => gl::BGR,
            Format::B8G8R8A8 => gl::BGRA,
            Format::R16 => gl::R16,
            Format::R16G16 => gl::RG16,
            Format::R16G16B16 => gl::RGB16,
            Format::R16G16B16A16 => gl::RGBA16,
        };

        let mut texture = Texture::new();
        texture.load_data(
            world_texture.width,
            world_texture.height,
            &world_texture.pixels,
            pixel_format,
        );
        texture
    }
}
