use crate::world::{
    texture::Texture,
    uniform::{
        DynamicUniform, DynamicUniformBinding, Geometry, TextureBinding, Uniform, UniformBinding,
    },
};
use anyhow::{anyhow, Context, Result};
use dragonglass_world::{
    AlphaMode, EntityStore, Material, MeshRender, RigidBody, Transform, World,
};
use wgpu::Queue;

// TODO:
// Add texture bind group layout, bind group, and make large texture array in shader
// When uploading textures, refresh the texture array in the shader
// Add push constants and use the push constant to offset into the texture array
// Update shader to use uv coordinates
// Check bytes_per_row calculation again, doesn't seem to work for some models
//
// Add joints back in
//
// Write blog post about world
// Write blog post about rendering the world (ubo, dynamic ubo, texture array, push constants for materials, etc)

pub(crate) struct WorldRender {
    render: Render,
}

impl WorldRender {
    pub fn new(device: &wgpu::Device, texture_format: wgpu::TextureFormat) -> Result<Self> {
        Ok(Self {
            render: Render::new(device, texture_format),
        })
    }

    pub fn load(&mut self, device: &wgpu::Device, queue: &Queue, world: &World) -> Result<()> {
        let textures = world
            .textures
            .iter()
            .map(|world_texture| {
                Texture::from_world_texture(device, queue, world_texture, "World Texture")
            })
            .collect::<Result<Vec<_>>>()?;
        self.render.upload_textures(device, textures, 0);
        self.render
            .geometry
            .upload_vertices(queue, 0, &world.geometry.vertices);
        self.render
            .geometry
            .upload_indices(queue, 0, &world.geometry.indices);
        Ok(())
    }

    pub fn update(&self, queue: &Queue, world: &World, aspect_ratio: f32) -> Result<()> {
        let (projection, view) = world.active_camera_matrices(aspect_ratio)?;

        self.render
            .uniform_binding
            .upload_uniform_data(queue, 0, &[Uniform { view, projection }]);

        if world.scene.graphs.is_empty() {
            return Ok(());
        }

        // Upload mesh ubos
        let mut mesh_ubos =
            vec![DynamicUniform::default(); DynamicUniformBinding::MAX_NUMBER_OF_MESHES];
        let mut ubo_offset = 0;
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

                mesh_ubos[ubo_offset] = DynamicUniform { model };
                ubo_offset += 1;
                Ok(())
            })?;
        }
        self.render
            .dynamic_uniform_binding
            .upload_uniform_data(queue, 0, &mesh_ubos);

        Ok(())
    }

    pub fn render<'a, 'b>(
        &'a self,
        render_pass: &'b mut wgpu::RenderPass<'a>,
        world: &'a World,
    ) -> Result<()> {
        self.render.bind_geometry(render_pass);
        self.render.bind_ubo(render_pass);

        for alpha_mode in [AlphaMode::Opaque, AlphaMode::Mask, AlphaMode::Blend].iter() {
            render_pass.set_pipeline(&self.render.pipeline);

            let mut ubo_offset = 0;
            for graph in world.scene.graphs.iter() {
                graph.walk(|node_index| {
                    let entity = graph[node_index];
                    let entry = world.ecs.entry_ref(entity)?;

                    let mesh_name = match entry.get_component::<MeshRender>().ok() {
                        Some(mesh_render) => &mesh_render.name,
                        None => {
                            ubo_offset += 1;
                            return Ok(());
                        }
                    };

                    let mesh = match world.geometry.meshes.get(mesh_name) {
                        Some(mesh) => mesh,
                        None => {
                            ubo_offset += 1;
                            return Ok(());
                        }
                    };

                    self.render.bind_dynamic_ubo(render_pass, ubo_offset);

                    match alpha_mode {
                        AlphaMode::Opaque | AlphaMode::Mask => {} /* Disable blending*/
                        AlphaMode::Blend => {}                    /* Enable blending */
                    }

                    for primitive in mesh.primitives.iter() {
                        let mut material = &Material::default();
                        if let Some(material_index) = primitive.material_index {
                            let primitive_material = world.material_at_index(material_index)?;
                            if primitive_material.alpha_mode != *alpha_mode {
                                continue;
                            }
                            material = primitive_material
                        }

                        if material.color_texture_index > -1 {
                            // TODO: Handle multitexturing here
                            self.render.bind_diffuse_texture(
                                render_pass,
                                material.color_texture_index as _,
                            )?;
                        }

                        let start = primitive.first_index as u32;
                        let end = start + (primitive.number_of_indices as u32);
                        render_pass.draw_indexed(start..end, 0, 0..1);
                    }

                    ubo_offset += 1;
                    Ok(())
                })?;
            }
        }

        Ok(())
    }
}

struct Render {
    textures: Vec<Texture>,
    pipeline: wgpu::RenderPipeline,
    geometry: Geometry,
    diffuse_texture_binding: TextureBinding,
    uniform_binding: UniformBinding,
    dynamic_uniform_binding: DynamicUniformBinding,
}

impl Render {
    pub const DIFFUSE_TEXTURE_BIND_GROUP_INDEX: u32 = 0;
    pub const UBO_BIND_GROUP_INDEX: u32 = 1;
    pub const DYNAMIC_UBO_BIND_GROUP_INDEX: u32 = 2;

    pub fn new(device: &wgpu::Device, texture_format: wgpu::TextureFormat) -> Self {
        let geometry = Geometry::new(device);

        let diffuse_texture_binding = TextureBinding::new(device);
        let uniform_binding = UniformBinding::new(device);
        let dynamic_uniform_binding = DynamicUniformBinding::new(device);

        let shader = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/shader.wgsl").into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                &diffuse_texture_binding.bind_group_layout,
                &uniform_binding.bind_group_layout,
                &dynamic_uniform_binding.bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[geometry.vertex_buffer_layout.clone()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[texture_format.into()],
            }),
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
        });

        Self {
            textures: Vec::new(),
            pipeline,
            geometry,
            diffuse_texture_binding,
            uniform_binding,
            dynamic_uniform_binding,
        }
    }

    pub fn bind_geometry<'a, 'b>(&'a self, render_pass: &'b mut wgpu::RenderPass<'a>) {
        render_pass.set_vertex_buffer(0, self.geometry.vertex_buffer.slice(..));
        render_pass.set_index_buffer(
            self.geometry.index_buffer.slice(..),
            wgpu::IndexFormat::Uint32,
        );
    }

    pub fn bind_ubo<'a, 'b>(&'a self, render_pass: &'b mut wgpu::RenderPass<'a>) {
        render_pass.set_bind_group(
            Self::UBO_BIND_GROUP_INDEX,
            &self.uniform_binding.bind_group,
            &[],
        );
    }

    pub fn bind_dynamic_ubo<'a, 'b>(
        &'a self,
        render_pass: &'b mut wgpu::RenderPass<'a>,
        offset: u32,
    ) {
        let offset = (offset as wgpu::DynamicOffset)
            * (self.dynamic_uniform_binding.alignment as wgpu::DynamicOffset);
        render_pass.set_bind_group(
            Self::DYNAMIC_UBO_BIND_GROUP_INDEX,
            &self.dynamic_uniform_binding.bind_group,
            &[offset],
        );
    }

    pub fn bind_diffuse_texture<'a, 'b>(
        &'a self,
        render_pass: &'b mut wgpu::RenderPass<'a>,
        texture_offset: usize,
    ) -> Result<()> {
        let bind_group = self
            .diffuse_texture_binding
            .bind_groups
            .get(texture_offset)
            .context(anyhow!(
                "Failed to find texture bind group at index {}",
                texture_offset,
            ))?;
        render_pass.set_bind_group(Self::DIFFUSE_TEXTURE_BIND_GROUP_INDEX, &bind_group, &[]);
        Ok(())
    }

    #[allow(dead_code)]
    pub fn clear_textures(&mut self) {
        self.textures.clear();
    }

    pub fn upload_textures(
        &mut self,
        device: &wgpu::Device,
        textures: Vec<Texture>,
        offset: usize,
    ) {
        textures
            .into_iter()
            .skip(offset)
            .for_each(|t| self.textures.push(t));
        self.diffuse_texture_binding
            .upload_textures(device, &self.textures, offset);
    }
}
