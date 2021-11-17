use dragonglass_world::Vertex;
use nalgebra_glm as glm;
use std::mem::size_of;
use wgpu::{util::DeviceExt, BufferAddress, Queue};

pub(crate) struct Geometry {
    pub vertex_buffer: wgpu::Buffer,
    pub vertex_buffer_layout: wgpu::VertexBufferLayout<'static>,
    pub index_buffer: wgpu::Buffer,
}

impl Geometry {
    // TODO: Determine these using the wgpu::limits
    pub const MAX_VERTICES: u32 = 7_000_000;
    pub const MAX_INDICES: u32 = 1_000_000;

    pub fn new(device: &wgpu::Device) -> Self {
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
            size: u64::from(Geometry::MAX_VERTICES * size_of::<Vertex>() as u32),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Index Buffer"),
            size: u64::from(Geometry::MAX_INDICES * size_of::<u32>() as u32),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Geometry {
            vertex_buffer,
            vertex_buffer_layout,
            index_buffer,
        }
    }

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

pub(crate) struct UniformBinding {
    pub buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

impl UniformBinding {
    pub fn new(device: &wgpu::Device) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&[Uniform::default()]),
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
            label: Some("Uniform Buffer Bind Group Layout"),
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
            label: Some("Uniform Buffer Bind Group"),
        });

        Self {
            buffer,
            bind_group_layout,
            bind_group,
        }
    }

    pub fn upload_uniform_data(
        &self,
        queue: &Queue,
        offset: BufferAddress,
        data: &[impl bytemuck::Pod],
    ) {
        queue.write_buffer(&self.buffer, offset, bytemuck::cast_slice(data));
    }
}

#[repr(C)]
#[derive(Default, Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Uniform {
    pub view: glm::Mat4,
    pub projection: glm::Mat4,
    pub model: glm::Mat4,
}
