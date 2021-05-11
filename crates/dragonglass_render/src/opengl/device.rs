use crate::Render;
use anyhow::{Context, Result};
use dragonglass_opengl::{
    gl::{self, types::*},
    glutin::{ContextWrapper, PossiblyCurrent},
    load_context,
};
use dragonglass_world::{
    AlphaMode, EntityStore, Format, Material, MeshRender, RigidBody, Transform, Vertex, World,
};
use imgui::{Context as ImguiContext, DrawData};
use raw_window_handle::HasRawWindowHandle;
use std::{ffi::CString, mem, ptr, str};

struct WorldRender {
    vao: u32,
    vbo: u32,
    ebo: u32,
    shader_program: u32,
    textures: Vec<u32>,
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

        // Create shader program to render models
        //     needs to take in vertex attributes as listed in Vertex struct
        //     needs a uniform buffer with projection and view matrices

        // Add vertex shader
        let vertex_shader_source = r#"
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

uniform sampler2D diffuseTexture;

in vec2 outUV0;

out vec4 color;

void main(void)
{
  color = texture(diffuseTexture, outUV0);
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

        // TODO
        // Load textures into texture array for shader to use
        let textures = world
            .textures
            .iter()
            .map(|texture| {
                let pixel_format = match texture.format {
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

                let mut id = 0;
                unsafe {
                    gl::GenTextures(1, &mut id);
                    gl::ActiveTexture(gl::TEXTURE0);
                    gl::BindTexture(gl::TEXTURE_2D, id);
                    gl::TexImage2D(
                        gl::TEXTURE_2D,
                        0,
                        pixel_format as i32,
                        texture.width as i32,
                        texture.height as i32,
                        0,
                        pixel_format,
                        gl::UNSIGNED_BYTE,
                        texture.pixels.as_ptr() as *const GLvoid,
                    );
                    gl::GenerateMipmap(gl::TEXTURE_2D);
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
                }
                id
            })
            .collect::<Vec<_>>();

        self.world_render = Some(WorldRender {
            vao,
            vbo,
            ebo,
            shader_program,
            textures,
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
        let depth: [GLfloat; 1] = [1.0];
        unsafe {
            gl::Viewport(0, 0, dimensions[0] as _, dimensions[1] as _);

            gl::Enable(gl::CULL_FACE);
            gl::CullFace(gl::BACK);
            gl::FrontFace(gl::CCW);

            gl::Enable(gl::DEPTH_TEST);
            gl::DepthFunc(gl::LEQUAL);

            gl::ClearBufferfv(gl::COLOR, 0, &color as *const f32);
            gl::ClearBufferfv(gl::DEPTH, 0, &depth as *const f32);
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

                    let entry = world.ecs.entry_ref(entity)?;

                    // Render rigid bodies at the transform specified by the physics world instead of the scenegraph
                    // NOTE: The rigid body collider scaling should be the same as the scale of the entity transform
                    //       otherwise this won't look right. It's probably best to just not scale entities that have rigid bodies
                    //       with colliders on them.
                    let model = match entry.get_component::<RigidBody>() {
                        Ok(rigid_body) => {
                            let body = world
                                .physics
                                .bodies
                                .get(rigid_body.handle)
                                .context("Failed to acquire physics body to render!")?;
                            let position = body.position();
                            let translation = position.translation.vector;
                            let rotation = *position.rotation.quaternion();
                            let scale =
                                Transform::from(world.global_transform(graph, node_index)?).scale;
                            Transform::new(translation, rotation, scale).matrix()
                        }
                        Err(_) => world.global_transform(graph, node_index)?,
                    };

                    let mvp = projection * view * model;
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
                                    let material = match primitive.material_index {
                                        Some(material_index) => {
                                            let primitive_material =
                                                world.material_at_index(material_index)?;
                                            if primitive_material.alpha_mode != *alpha_mode {
                                                continue;
                                            }
                                            primitive_material.clone()
                                        }
                                        None => Material::default(),
                                    };

                                    unsafe {
                                        gl::ActiveTexture(gl::TEXTURE0);
                                        gl::BindTexture(
                                            gl::TEXTURE_2D,
                                            world_render.textures
                                                [material.color_texture_index as usize],
                                        );
                                    }

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
