use super::WorldRender;
use anyhow::Result;
use nalgebra_glm as glm;
use std::{marker::PhantomData, mem};

pub(crate) struct UniformBuffer<T>
where
    T: bytemuck::Pod + bytemuck::Zeroable,
{
    pub buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    _marker: PhantomData<T>,
}

#[repr(C)]
#[derive(Default, Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct WorldUniformData {
    pub view: glm::Mat4,
    pub projection: glm::Mat4,
}

impl UniformBuffer<WorldUniformData> {
    pub fn new(device: &wgpu::Device) -> Result<Self> {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("World Uniform Buffer"),
            size: std::mem::size_of::<WorldUniformData>() as _,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
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
            buffer,
            bind_group_layout,
            bind_group,
            _marker: PhantomData::default(),
        })
    }

    pub fn upload(&self, queue: &wgpu::Queue, data: WorldUniformData) {
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[data]));
    }
}

#[repr(C)]
#[derive(Default, Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct EntityUniformData {
    pub model: glm::Mat4,
}

impl UniformBuffer<EntityUniformData> {
    pub fn new(device: &wgpu::Device) -> Result<Self> {
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
            buffer,
            bind_group_layout,
            bind_group,
            _marker: PhantomData::default(),
        })
    }

    pub fn upload_all(&self, queue: &wgpu::Queue, data: &[EntityUniformData]) {
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(data));
    }
}
