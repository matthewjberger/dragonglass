use crate::Render;
use anyhow::{Context, Result};
use dragonglass_opengl::{
    gl::{self, types::*},
    glutin::{ContextWrapper, PossiblyCurrent},
    load_context, GeometryBuffer, ShaderProgram, Texture,
};
use dragonglass_world::{
    AlphaMode, EntityStore, Format, Material, MeshRender, RigidBody, Transform, World,
};
use imgui::{Context as ImguiContext, DrawData};
use raw_window_handle::HasRawWindowHandle;
use std::{ffi::CString, ptr, str};

struct WorldRender {
    geometry: GeometryBuffer,
    shader_program: ShaderProgram,
    textures: Vec<Texture>,
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
        let geometry = GeometryBuffer::new(
            &world.geometry.vertices,
            &world.geometry.indices,
            &[3, 3, 2, 2, 4, 4, 3],
        );

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
        let mut shader_program = ShaderProgram::new();
        shader_program
            .vertex_shader_source(vertex_shader_source)
            .fragment_shader_source(fragment_shader_source)
            .link();

        // TODO
        // Load textures into texture array for shader to use
        let textures = world
            .textures
            .iter()
            .map(|world_texture| {
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
                // FIXME GL: impl From on this
                texture.load_data(
                    world_texture.width,
                    world_texture.height,
                    &world_texture.pixels,
                    pixel_format,
                );
                texture
            })
            .collect::<Vec<_>>();

        self.world_render = Some(WorldRender {
            geometry,
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

        world_render.geometry.bind();
        world_render.shader_program.use_program();

        let name: CString = CString::new("mvpMatrix".as_bytes())?;
        let mvp_location =
            unsafe { gl::GetUniformLocation(world_render.shader_program.id(), name.as_ptr()) };

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
                                    AlphaMode::Opaque | AlphaMode::Mask => unsafe {
                                        gl::Disable(gl::BLEND);
                                    },
                                    AlphaMode::Blend => unsafe {
                                        gl::Enable(gl::BLEND);
                                        gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
                                    },
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

                                    world_render.textures[material.color_texture_index as usize]
                                        .bind(0);

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
