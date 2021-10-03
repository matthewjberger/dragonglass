use std::mem;

use anyhow::Result;
use nalgebra_glm as glm;
use wgpu::util::DeviceExt;

use super::WorldRender;

pub(crate) struct UniformBuffer<T>
where
    T: bytemuck::Pod + bytemuck::Zeroable,
{
    pub data: T,
    pub buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

#[repr(C)]
#[derive(Default, Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct WorldUniformData {
    pub view: glm::Mat4,
    pub projection: glm::Mat4,
}

impl UniformBuffer<WorldUniformData> {
    pub fn new(device: &wgpu::Device) -> Result<Self> {
        let data = WorldUniformData::default();

        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("World Uniform Buffer"),
            contents: bytemuck::cast_slice(&[data]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("World Uniform Buffer Layout"),
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
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
            label: Some("World Uniform Buffer Bind Group"),
        });

        Ok(Self {
            data,
            buffer,
            bind_group_layout,
            bind_group,
        })
    }

    pub fn upload(&self, queue: &wgpu::Queue) {
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[self.data]));
    }
}

#[repr(C)]
#[derive(Default, Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct EntityUniformData {
    pub model: glm::Mat4,
}

impl UniformBuffer<EntityUniformData> {
    pub fn new(device: &wgpu::Device) -> Result<Self> {
        let data = EntityUniformData::default();

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Entity Uniform Buffer"),
            size: (WorldRender::MAX_NUMBER_OF_MESHES as wgpu::BufferAddress)
                * wgpu::BIND_BUFFER_ALIGNMENT,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Entity Uniform Buffer Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(
                        mem::size_of::<EntityUniformData>() as _
                    ),
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(mem::size_of::<EntityUniformData>() as _),
                }),
            }],
            label: Some("Entity Uniform Buffer Bind Group"),
        });

        Ok(Self {
            data,
            buffer,
            bind_group_layout,
            bind_group,
        })
    }
}
