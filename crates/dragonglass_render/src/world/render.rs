use anyhow::{Context, Result};
use dragonglass_world::{AlphaMode, EntityStore, MeshRender, RigidBody, Transform, Vertex, World};
use nalgebra_glm as glm;
use std::mem::size_of;
use wgpu::{util::DeviceExt, BufferAddress, Queue};

pub(crate) struct WorldRender {
    pipeline: wgpu::RenderPipeline,
    uniform_binding: UniformBinding,
    geometry: Geometry,
}

impl WorldRender {
    pub fn new(device: &wgpu::Device, texture_format: wgpu::TextureFormat) -> Result<Self> {
        let (uniform_binding, geometry, pipeline) = Self::create_pipeline(device, texture_format);
        Ok(Self {
            pipeline,
            uniform_binding,
            geometry,
        })
    }

    fn create_shader_module(device: &wgpu::Device) -> wgpu::ShaderModule {
        device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/shader.wgsl").into()),
        })
    }

    fn create_uniform_buffer(device: &wgpu::Device) -> UniformBinding {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("World Uniform Buffer"),
            contents: bytemuck::cast_slice(&[WorldUniform::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
            label: Some("World Uniform Buffer Bind Group Layout"),
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
            label: Some("World Uniform Buffer Bind Group"),
        });

        UniformBinding {
            buffer,
            bind_group_layout,
            bind_group,
        }
    }

    fn create_geometry(device: &wgpu::Device) -> Geometry {
        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // normal
                wgpu::VertexAttribute {
                    offset: size_of::<glm::Vec3>() as _,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // uv_0
                wgpu::VertexAttribute {
                    offset: (2 * size_of::<glm::Vec3>()) as _,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // uv_1
                wgpu::VertexAttribute {
                    offset: (2 * size_of::<glm::Vec3>() + size_of::<glm::Vec2>()) as _,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // joint_0
                wgpu::VertexAttribute {
                    offset: (2 * size_of::<glm::Vec3>() + 2 * size_of::<glm::Vec2>()) as _,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // weight_0
                wgpu::VertexAttribute {
                    offset: (2 * size_of::<glm::Vec3>()
                        + 2 * size_of::<glm::Vec2>()
                        + size_of::<glm::Vec4>()) as _,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // color_0
                wgpu::VertexAttribute {
                    offset: (2 * size_of::<glm::Vec3>()
                        + 2 * size_of::<glm::Vec2>()
                        + 2 * size_of::<glm::Vec4>()) as _,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        };

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"),
            size: u64::from(Geometry::MAX_VERTICES * std::mem::size_of::<Vertex>() as u32),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Index Buffer"),
            size: u64::from(Geometry::MAX_INDICES * std::mem::size_of::<u32>() as u32),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Geometry {
            vertex_buffer,
            vertex_buffer_layout,
            index_buffer,
        }
    }

    fn create_pipeline(
        device: &wgpu::Device,
        texture_format: wgpu::TextureFormat,
    ) -> (UniformBinding, Geometry, wgpu::RenderPipeline) {
        let uniform_binding = Self::create_uniform_buffer(device);
        let geometry = Self::create_geometry(&device);
        let shader = Self::create_shader_module(device);

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&uniform_binding.bind_group_layout],
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
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
        });

        (uniform_binding, geometry, pipeline)
    }

    pub fn load(&self, queue: &Queue, world: &World) -> Result<()> {
        self.geometry
            .upload_vertices(queue, 0, &world.geometry.vertices);
        self.geometry
            .upload_indices(queue, 0, &world.geometry.indices);
        Ok(())
    }

    pub fn update(&self, queue: &Queue, world: &World, aspect_ratio: f32) -> Result<()> {
        let (projection, view) = world.active_camera_matrices(aspect_ratio)?;
        self.uniform_binding
            .upload_uniform_data(queue, 0, &[WorldUniform { view, projection }]);
        Ok(())
    }

    pub fn render<'a, 'b>(
        &'a self,
        render_pass: &'b mut wgpu::RenderPass<'a>,
        world: &'a World,
    ) -> Result<()> {
        render_pass.set_pipeline(&self.pipeline);

        render_pass.set_bind_group(0, &self.uniform_binding.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.geometry.vertex_buffer.slice(..));
        render_pass.set_index_buffer(
            self.geometry.index_buffer.slice(..),
            wgpu::IndexFormat::Uint16,
        );

        for alpha_mode in [AlphaMode::Opaque, AlphaMode::Mask, AlphaMode::Blend].iter() {
            for graph in world.scene.graphs.iter() {
                graph.walk(|node_index| {
                    let entity = graph[node_index];
                    let entry = world.ecs.entry_ref(entity)?;

                    let mesh_name = match entry.get_component::<MeshRender>().ok() {
                        Some(mesh_render) => &mesh_render.name,
                        None => return Ok(()),
                    };

                    let mesh = match world.geometry.meshes.get(mesh_name) {
                        Some(mesh) => mesh,
                        None => return Ok(()),
                    };

                    match alpha_mode {
                        AlphaMode::Opaque | AlphaMode::Mask => {} /* Disable blending*/
                        AlphaMode::Blend => {}                    /* Enable blending */
                    }

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

                    // TODO: Assign model matrix

                    for primitive in mesh.primitives.iter() {
                        // TODO: Use material
                        // let material = match primitive.material_index {
                        //     Some(material_index) => {
                        //         let primitive_material = world.material_at_index(material_index)?;
                        //         if primitive_material.alpha_mode != *alpha_mode {
                        //             continue;
                        //         }
                        //         primitive_material.clone()
                        //     }
                        //     None => Material::default(),
                        // };

                        let start = primitive.first_index as u32;
                        let end = start + (primitive.number_of_indices as u32);
                        render_pass.draw_indexed(start..end, 0, 0..1);
                    }

                    Ok(())
                })?;
            }
        }

        Ok(())
    }
}

#[repr(C)]
#[derive(Default, Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct WorldUniform {
    view: glm::Mat4,
    projection: glm::Mat4,
}

struct UniformBinding {
    buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
}

impl UniformBinding {
    pub fn upload_uniform_data(
        &self,
        queue: &Queue,
        offset: BufferAddress,
        data: &[impl bytemuck::Pod],
    ) {
        queue.write_buffer(&self.buffer, offset, bytemuck::cast_slice(data));
    }
}

struct Geometry {
    vertex_buffer: wgpu::Buffer,
    vertex_buffer_layout: wgpu::VertexBufferLayout<'static>,
    index_buffer: wgpu::Buffer,
}

impl Geometry {
    // TODO: Determine these using the wgpu::limits
    pub const MAX_VERTICES: u32 = 1_000_000;
    pub const MAX_INDICES: u32 = 1_000_000;

    pub fn upload_vertices(
        &self,
        queue: &Queue,
        offset: BufferAddress,
        data: &[impl bytemuck::Pod],
    ) {
        // TODO: Check if the vertex buffer needs to be resized
        queue.write_buffer(&self.vertex_buffer, offset, bytemuck::cast_slice(data));
    }

    pub fn upload_indices(
        &self,
        queue: &Queue,
        offset: BufferAddress,
        data: &[impl bytemuck::Pod],
    ) {
        // TODO: Check if the index buffer needs to be resized
        queue.write_buffer(&self.index_buffer, offset, bytemuck::cast_slice(data));
    }
}
