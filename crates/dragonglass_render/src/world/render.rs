use crate::world::uniforms::{UniformBuffer, WorldUniformData};
use anyhow::Result;
use dragonglass_world::World;
use wgpu::{util::DeviceExt, Queue};

use super::EntityUniformData;

pub(crate) struct WorldRender {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub render_pipeline: wgpu::RenderPipeline,
    pub world_uniforms: UniformBuffer<WorldUniformData>,
    pub entity_uniforms: UniformBuffer<EntityUniformData>,
}

impl WorldRender {
    // This does not need to be matched in the shader
    pub const MAX_NUMBER_OF_MESHES: usize = 500;

    pub fn new(
        device: &wgpu::Device,
        texture_format: wgpu::TextureFormat,
        world: &World,
    ) -> Result<Self> {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("World Vertex Buffer"),
            contents: bytemuck::cast_slice(&world.geometry.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("World Index Buffer"),
            contents: bytemuck::cast_slice(&world.geometry.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let shader = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/shader.wgsl").into()),
        });

        let world_uniforms = UniformBuffer::<WorldUniformData>::new(device)?;
        let entity_uniforms = UniformBuffer::<EntityUniformData>::new(device)?;

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("World Render Pipeline Layout"),
                bind_group_layouts: &[
                    &world_uniforms.bind_group_layout,
                    &entity_uniforms.bind_group_layout,
                ],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "main",
                buffers: &[Self::vertex_descriptor()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "main",
                targets: &[wgpu::ColorTargetState {
                    format: texture_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                clamp_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        });

        Ok(Self {
            vertex_buffer,
            index_buffer,
            render_pipeline,
            world_uniforms,
            entity_uniforms,
        })
    }

    pub fn update(&self, queue: &Queue, world: &World, aspect_ratio: f32) -> Result<()> {
        let (projection, view) = world.active_camera_matrices(aspect_ratio)?;

        self.world_uniforms
            .upload(queue, WorldUniformData { view, projection });

        let mut buffers = vec![EntityUniformData::default(); Self::MAX_NUMBER_OF_MESHES];

        let mut ubo_offset = 0;
        for graph in world.scene.graphs.iter() {
            graph.walk(|node_index| {
                let model = world.global_transform(graph, node_index)?;
                buffers[ubo_offset] = EntityUniformData { model };
                ubo_offset += 1;
                Ok(())
            })?;
        }

        self.entity_uniforms.upload_all(queue, &buffers);

        Ok(())
    }

    fn vertex_descriptor<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<dragonglass_world::Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            // [3, 3, 2, 2, 4, 4, 3]
            attributes: &[
                // Position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // Normal
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // UV_0
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // UV_1
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // JOINT_0
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 10]>() as wgpu::BufferAddress,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // JOINT_1
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 14]>() as wgpu::BufferAddress,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // COLOR_0
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 18]>() as wgpu::BufferAddress,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}
