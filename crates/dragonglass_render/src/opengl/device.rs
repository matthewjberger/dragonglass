use crate::Render;
use anyhow::{Context, Result};
use dragonglass_opengl::{
    gl::{self, types::*},
    glutin::{ContextWrapper, PossiblyCurrent},
    load_context,
};
use dragonglass_world::{AlphaMode, EntityStore, MeshRender, Vertex, World};
use imgui::{Context as ImguiContext, DrawData};
use raw_window_handle::HasRawWindowHandle;
use std::{ffi::CString, mem, ptr, str};

struct WorldRender {
    vao: u32,
    vbo: u32,
    ebo: u32,
    shader_program: u32,
}

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
        // TODO: Wrap this logic in a small object with resources freed in Drop impl
        // VAO object
        // VBO object

        // Create vertex array object
        let mut vao = 0;
        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::BindVertexArray(vao);
        }

        // Create VBO and upload vertices to it
        let mut vbo = 0;
        unsafe {
            gl::GenBuffers(1, &mut vbo);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (world.geometry.vertices.len() * mem::size_of::<Vertex>()) as _,
                world.geometry.vertices.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );
        }

        // TODO: support not having indices at some point
        // Create EBO and upload indices to it
        let mut ebo = 0;
        unsafe {
            gl::GenBuffers(1, &mut ebo);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, ebo);
            gl::BufferData(
                gl::ELEMENT_ARRAY_BUFFER,
                (world.geometry.indices.len() * mem::size_of::<u32>()) as _,
                world.geometry.indices.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );
        }

        // Describe vertex layout
        let mut index = 0;
        let mut offset = 0;
        let mut add_vertex = |count: usize| {
            unsafe {
                gl::EnableVertexAttribArray(index);
                gl::VertexAttribPointer(
                    index,
                    count as i32,
                    gl::FLOAT,
                    gl::FALSE,
                    std::mem::size_of::<Vertex>() as i32,
                    (offset * mem::size_of::<f32>()) as *const GLvoid,
                );
            }
            index += 1;
            offset += count;
        };
        [3, 3, 2, 2, 4, 4, 3].iter().for_each(|i| add_vertex(*i));

        // TODO
        // Load textures into texture array for shader to use

        // Create shader program to render models
        //     needs to take in vertex attributes as listed in Vertex struct
        //     needs a uniform buffer with projection and view matrices

        // Add vertex shader
        let vertex_shader_source = r#"
#version 450 core
layout (location = 0) in vec3 v_position;

uniform mat4 mvpMatrix;

void main()
{
   gl_Position = mvpMatrix * vec4(v_position, 1.0f);
}
"#;
        let vertex_shader_source_cstr = CString::new(vertex_shader_source.as_bytes())?;
        let vertex_shader = unsafe {
            let vertex_shader = gl::CreateShader(gl::VERTEX_SHADER);
            gl::ShaderSource(
                vertex_shader,
                1,
                &vertex_shader_source_cstr.as_ptr(),
                ptr::null(),
            );
            gl::CompileShader(vertex_shader);
            vertex_shader
        };
        check_compilation(vertex_shader)?;

        // Add fragment shader
        let fragment_shader_source = r#"
#version 450 core

out vec4 color;

void main(void)
{
  color = vec4(0.0, 1.0, 0.0, 1.0);
}
"#;
        let fragment_shader_source_cstr = CString::new(fragment_shader_source.as_bytes())?;
        let fragment_shader = unsafe {
            let fragment_shader = gl::CreateShader(gl::FRAGMENT_SHADER);
            gl::ShaderSource(
                fragment_shader,
                1,
                &fragment_shader_source_cstr.as_ptr(),
                ptr::null(),
            );
            gl::CompileShader(fragment_shader);
            fragment_shader
        };
        check_compilation(fragment_shader)?;

        // Create the shader program
        let shader_program = unsafe {
            let shader_program = gl::CreateProgram();
            gl::AttachShader(shader_program, vertex_shader);
            gl::AttachShader(shader_program, fragment_shader);
            gl::LinkProgram(shader_program);
            shader_program
        };

        // Discard the shaders
        unsafe {
            gl::DeleteShader(vertex_shader);
            gl::DeleteShader(fragment_shader);
        }

        self.world_render = Some(WorldRender {
            vao,
            vbo,
            ebo,
            shader_program,
        });

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
        let color: [GLfloat; 4] = [0.0, 0.5, 0.0, 0.0];
        unsafe {
            gl::Viewport(0, 0, dimensions[0] as _, dimensions[1] as _);
            gl::ClearBufferfv(gl::COLOR, 0, &color as *const f32);
        }

        let world_render = match self.world_render.as_ref() {
            Some(result) => result,
            None => {
                self.context.swap_buffers()?;
                return Ok(());
            }
        };

        unsafe {
            gl::BindVertexArray(world_render.vao);
            gl::UseProgram(world_render.shader_program);
        }

        let name: CString = CString::new("mvpMatrix".as_bytes())?;
        let mvp_location =
            unsafe { gl::GetUniformLocation(world_render.shader_program, name.as_ptr()) };

        let aspect_ratio = dimensions[0] as f32 / std::cmp::max(dimensions[1], 1) as f32;
        let (projection, view) = world.active_camera_matrices(aspect_ratio)?;

        for alpha_mode in [AlphaMode::Opaque, AlphaMode::Mask, AlphaMode::Blend].iter() {
            for graph in world.scene.graphs.iter() {
                graph.walk(|node_index| {
                    let entity = graph[node_index];

                    let transform = world.entity_global_transform(entity)?;

                    let mvp = projection * view * transform.matrix();
                    unsafe {
                        gl::UniformMatrix4fv(mvp_location, 1, gl::FALSE, mvp.as_ptr());
                    }

                    match world.ecs.entry_ref(entity)?.get_component::<MeshRender>() {
                        Ok(mesh_render) => {
                            if let Some(mesh) = world.geometry.meshes.get(&mesh_render.name) {
                                match alpha_mode {
                                    AlphaMode::Opaque | AlphaMode::Mask => {
                                        // TODO
                                    }
                                    AlphaMode::Blend => {
                                        // TODO: blend
                                    }
                                }

                                for primitive in mesh.primitives.iter() {
                                    // TODO: render primitive
                                    let ptr: *const u8 = ptr::null_mut();
                                    let ptr = unsafe {
                                        ptr.add(primitive.first_index * std::mem::size_of::<u32>())
                                    };
                                    unsafe {
                                        gl::DrawElements(
                                            gl::TRIANGLES,
                                            primitive.number_of_indices as _,
                                            gl::UNSIGNED_INT,
                                            ptr as *const _,
                                        );
                                    }
                                }
                            }
                        }
                        Err(_) => return Ok(()),
                    }

                    Ok(())
                })?;
            }
        }

        self.context.swap_buffers()?;
        Ok(())
    }

    fn toggle_wireframe(&mut self) {}
}

fn check_compilation(id: u32) -> Result<()> {
    let mut success = gl::FALSE as GLint;
    unsafe {
        gl::GetShaderiv(id, gl::COMPILE_STATUS, &mut success);
    }

    if success == gl::TRUE as GLint {
        log::info!("Shader compilation succeeded!");
        return Ok(());
    }

    let mut info_log_length = 0;
    unsafe {
        gl::GetShaderiv(id, gl::INFO_LOG_LENGTH, &mut info_log_length);
    }

    let mut info_log = vec![0; info_log_length as usize];
    unsafe {
        gl::GetShaderInfoLog(
            id,
            info_log_length,
            ptr::null_mut(),
            info_log.as_mut_ptr() as *mut GLchar,
        );
    }

    log::error!(
        "ERROR: Shader compilation failed.\n{}\n",
        str::from_utf8(&info_log)?
    );

    Ok(())
}
