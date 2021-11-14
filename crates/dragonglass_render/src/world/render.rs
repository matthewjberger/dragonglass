use anyhow::Result;
use dragonglass_world::World;
use nalgebra_glm as glm;
use wgpu::{util::DeviceExt, BufferAddress, Queue};

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
    pub fn update(&self, queue: &Queue, offset: BufferAddress, data: impl bytemuck::Pod) {
        queue.write_buffer(&self.buffer, offset, bytemuck::cast_slice(&[data]));
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [-0.0868241, 0.49240386, 0.0],
        color: [0.5, 0.0, 0.5],
    }, // A
    Vertex {
        position: [-0.49513406, 0.06958647, 0.0],
        color: [0.5, 0.0, 0.5],
    }, // B
    Vertex {
        position: [-0.21918549, -0.44939706, 0.0],
        color: [0.5, 0.0, 0.5],
    }, // C
    Vertex {
        position: [0.35966998, -0.3473291, 0.0],
        color: [0.5, 0.0, 0.5],
    }, // D
    Vertex {
        position: [0.44147372, 0.2347359, 0.0],
        color: [0.5, 0.0, 0.5],
    }, // E
];

const INDICES: &[u16] = &[0, 1, 4, 1, 2, 4, 2, 3, 4, /* padding */ 0];

pub(crate) struct WorldRender {
    pub render_pipeline: wgpu::RenderPipeline,
    uniform_binding: UniformBinding,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    number_of_indices: u32,
}

impl WorldRender {
    pub const MAX_VERTICES: u32 = 10_000;
    pub const MAX_INDICES: u32 = 10_000;

    pub fn new(device: &wgpu::Device, texture_format: wgpu::TextureFormat) -> Result<Self> {
        let shader = Self::create_shader_module(device);

        let uniform_binding = Self::create_uniform_buffer(device);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&uniform_binding.bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
        };

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[vertex_buffer_layout],
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

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"),
            size: u64::from(Self::MAX_VERTICES * std::mem::size_of::<Vertex>() as u32),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Index Buffer"),
            size: u64::from(Self::MAX_INDICES * std::mem::size_of::<u32>() as u32),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            render_pipeline,
            uniform_binding,
            vertex_buffer,
            index_buffer,
            number_of_indices: INDICES.len() as u32,
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

    pub fn load(&self, queue: &Queue, _world: &World) -> Result<()> {
        // TODO: Check if the vertex buffer needs to be resized
        // TODO: Check if the index buffer needs to be resized
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(VERTICES));
        queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(INDICES));
        Ok(())
    }

    pub fn update(&self, queue: &Queue, world: &World, aspect_ratio: f32) -> Result<()> {
        let (projection, view) = world.active_camera_matrices(aspect_ratio)?;
        self.uniform_binding
            .update(queue, 0, WorldUniform { view, projection });
        Ok(())
    }

    pub fn render<'a, 'b>(&'a self, render_pass: &'b mut wgpu::RenderPass<'a>) {
        render_pass.set_pipeline(&self.render_pipeline);

        render_pass.set_bind_group(0, &self.uniform_binding.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

        render_pass.draw_indexed(0..self.number_of_indices, 0, 0..1);
    }
}
