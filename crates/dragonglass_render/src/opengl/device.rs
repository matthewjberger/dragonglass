use crate::{opengl::world::WorldRender, Render};
use anyhow::{Context, Result};
use dragonglass_opengl::{
    gl::{self, types::*},
    glutin::{ContextWrapper, PossiblyCurrent},
    load_context,
};
use dragonglass_world::{
    AlphaMode, EntityStore, Material, MeshRender, RigidBody, Transform, World,
};
use imgui::{Context as ImguiContext, DrawData};
use raw_window_handle::HasRawWindowHandle;
use std::{ffi::CString, ptr, str};

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
