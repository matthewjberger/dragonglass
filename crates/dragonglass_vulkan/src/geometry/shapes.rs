use crate::core::{CommandPool, GeometryBuffer};
use anyhow::{Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use nalgebra_glm as glm;
use parry3d::{math::Real, na::Point3, shape};
use std::{collections::HashMap, sync::Arc};
use vk_mem::Allocator;

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub enum Shape {
    Sphere,
    Cube,
    Cylinder,
    Cone,
}

#[derive(Default)]
struct Entry {
    pub first_index: u32,
    pub number_of_indices: u32,
}

impl Entry {
    pub fn new(first_index: u32, number_of_indices: u32) -> Self {
        Self {
            first_index,
            number_of_indices,
        }
    }
}

pub struct ShapeBuffer {
    pub geometry_buffer: GeometryBuffer,
    entries: HashMap<Shape, Entry>,
}

impl ShapeBuffer {
    pub fn new(allocator: Arc<Allocator>, command_pool: &CommandPool) -> Result<Self> {
        // TODO: Move this out to a separate struct (geometry accumulator?)
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut vertex_offset = 0;
        let mut entries = HashMap::new();

        let mut add_trimesh = |shape: Shape, trimesh: (Vec<Point3<Real>>, Vec<[u32; 3]>)| {
            let (v, i) = trimesh;
            vertices.extend_from_slice(&v);

            let shape_indices = i
                .iter()
                .flat_map(|j| j)
                .map(|j| j + vertex_offset)
                .collect::<Vec<_>>();

            entries.insert(
                shape,
                Entry::new(indices.len() as _, shape_indices.len() as _),
            );

            indices.extend_from_slice(&shape_indices);
            vertex_offset += vertices.len() as u32;
        };

        // let cube = shape::Cuboid::new(glm::vec3(0.5, 0.5, 0.5));
        // add_trimesh(Shape::Cube, cube.to_trimesh());

        // let ball = shape::Ball::new(0.5);
        // add_trimesh(Shape::Sphere, ball.to_trimesh(100, 100));

        // let cylinder = shape::Cylinder::new(0.5, 0.5);
        // add_trimesh(Shape::Cylinder, cylinder.to_trimesh(100));

        let cone = shape::Cone::new(10.0, 10.0);
        add_trimesh(Shape::Cone, cone.to_trimesh(100));

        let geometry_buffer = GeometryBuffer::new(
            allocator,
            (vertices.len() * std::mem::size_of::<Point3<Real>>()) as _,
            Some((indices.len() * std::mem::size_of::<u32>()) as _),
        )?;

        geometry_buffer
            .vertex_buffer
            .upload_data(&vertices, 0, command_pool)?;

        geometry_buffer
            .index_buffer
            .as_ref()
            .context("Failed to access shape index buffer!")?
            .upload_data(&indices, 0, command_pool)?;

        Ok(Self {
            geometry_buffer,
            entries,
        })
    }

    pub fn vertex_attributes() -> [vk::VertexInputAttributeDescription; 1] {
        let position_description = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(0)
            .build();

        [position_description]
    }

    pub fn vertex_inputs() -> [vk::VertexInputBindingDescription; 1] {
        let vertex_input_binding_description = vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride(std::mem::size_of::<Point3<Real>>() as _)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build();
        [vertex_input_binding_description]
    }

    pub fn draw(
        &self,
        device: &ash::Device,
        command_buffer: vk::CommandBuffer,
        shape: &Shape,
    ) -> Result<()> {
        let entry = self
            .entries
            .get(shape)
            .context("Requested shape doesn't exist in shape buffer")?;
        self.geometry_buffer.bind(device, command_buffer)?;
        unsafe {
            device.cmd_draw_indexed(
                command_buffer,
                entry.number_of_indices,
                1,
                entry.first_index,
                0,
                0,
            );
        }
        Ok(())
    }
}
