use crate::buffer::*;
use crate::shader::*;
use crate::texture::*;
use nalgebra_glm as glm;

// TODO: Make common primitive geometry file (with indices)
#[rustfmt::skip]
const VERTEX_POSITIONS: &[GLfloat; 108] =
    &[
       -1.0,  1.0, -1.0,
       -1.0, -1.0, -1.0,
        1.0, -1.0, -1.0,

        1.0, -1.0, -1.0,
        1.0,  1.0, -1.0,
       -1.0,  1.0, -1.0,

        1.0, -1.0, -1.0,
        1.0, -1.0,  1.0,
        1.0,  1.0, -1.0,

        1.0, -1.0,  1.0,
        1.0,  1.0,  1.0,
        1.0,  1.0, -1.0,

        1.0, -1.0,  1.0,
       -1.0, -1.0,  1.0,
        1.0,  1.0,  1.0,

       -1.0, -1.0,  1.0,
       -1.0,  1.0,  1.0,
        1.0,  1.0,  1.0,

       -1.0, -1.0,  1.0,
       -1.0, -1.0, -1.0,
       -1.0,  1.0,  1.0,

       -1.0, -1.0, -1.0,
       -1.0,  1.0, -1.0,
       -1.0,  1.0,  1.0,

       -1.0, -1.0,  1.0,
        1.0, -1.0,  1.0,
        1.0, -1.0, -1.0,

        1.0, -1.0, -1.0,
       -1.0, -1.0, -1.0,
       -1.0, -1.0,  1.0,

       -1.0,  1.0, -1.0,
        1.0,  1.0, -1.0,
        1.0,  1.0,  1.0,

        1.0,  1.0,  1.0,
       -1.0,  1.0,  1.0,
       -1.0,  1.0, -1.0
    ];

const VERTEX_SHADER: &str = r#"#version 330 core
layout (location = 0) in vec3 aPos;

out vec3 TexCoords;

uniform mat4 projection;
uniform mat4 view;

void main()
{
    TexCoords = aPos;
    vec4 pos = projection * view * vec4(aPos, 1.0);
    gl_Position = pos.xyww;
}
"#;

const FRAGMENT_SHADER: &str = r#"#version 330 core
out vec4 FragColor;

in vec3 TexCoords;

uniform samplerCube skybox;

void main()
{    
    FragColor = texture(skybox, TexCoords);
}
"#;

#[derive(Default)]
pub struct Skybox {
    vao: VertexArrayObject,
    vbo: Buffer,
    shader_program: ShaderProgram,
    pub texture: Texture,
}

impl Skybox {
    pub fn new(paths: &[String; 6]) -> Self {
        let mut skybox = Skybox::default();
        skybox.shader_program = ShaderProgram::new();
        skybox
            .shader_program
            .vertex_shader_source(VERTEX_SHADER)
            .fragment_shader_source(FRAGMENT_SHADER)
            .link();

        skybox.vao = VertexArrayObject::new();
        skybox.vbo = Buffer::new(BufferKind::Array);
        skybox.vbo.add_data(VERTEX_POSITIONS);
        skybox.vbo.upload(&skybox.vao, DrawingHint::StaticDraw);
        skybox.vao.configure_attribute(0, 3, 3, 0);
        skybox.texture = Texture::cubemap_from_files(paths);
        skybox
    }

    pub fn render(&self, projection_matrix: &glm::Mat4, view_matrix: &glm::Mat4) {
        self.shader_program.activate();

        let view_matrix = glm::mat3_to_mat4(&glm::mat4_to_mat3(&*view_matrix));
        self.vao.bind();
        self.texture.bind(0);

        self.shader_program
            .set_uniform_matrix4x4("projection", projection_matrix.as_slice());

        self.shader_program
            .set_uniform_matrix4x4("view", view_matrix.as_slice());

        self.shader_program.set_uniform_int("skybox", 0);

        unsafe {
            gl::Enable(gl::DEPTH_TEST);
            gl::DepthFunc(gl::LEQUAL);
            gl::DrawArrays(gl::TRIANGLES, 0, 36);
            gl::DepthFunc(gl::LESS);
        }
    }
}
