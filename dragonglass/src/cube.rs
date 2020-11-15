use crate::{adapters::CommandPool, context::Context, resources::GeometryBuffer};
use anyhow::{Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};

#[rustfmt::skip]
pub const VERTICES: &[f32; 24] =
    &[
        // Front
       -0.5, -0.5,  0.5,
        0.5, -0.5,  0.5,
        0.5,  0.5,  0.5,
       -0.5,  0.5,  0.5,
        // Back
       -0.5, -0.5, -0.5,
        0.5, -0.5, -0.5,
        0.5,  0.5, -0.5,
       -0.5,  0.5, -0.5
    ];

#[rustfmt::skip]
pub const INDICES: &[u32; 36] =
    &[
        // Front
        0, 1, 2,
        2, 3, 0,
        // Right
        1, 5, 6,
        6, 2, 1,
        // Back
        7, 6, 5,
        5, 4, 7,
        // Left
        4, 0, 3,
        3, 7, 4,
        // Bottom
        4, 5, 1,
        1, 0, 4,
        // Top
        3, 2, 6,
        6, 7, 3
    ];

pub struct Cube {
    pub geometry_buffer: GeometryBuffer,
}

impl Cube {
    pub fn new(context: &Context, command_pool: &CommandPool) -> Result<Self> {
        let geometry_buffer = GeometryBuffer::new(
            context.allocator.clone(),
            (VERTICES.len() * std::mem::size_of::<f32>()) as _,
            Some((INDICES.len() * std::mem::size_of::<u32>()) as _),
        )?;

        geometry_buffer.vertex_buffer.upload_data(
            VERTICES,
            0,
            command_pool,
            context.graphics_queue(),
        )?;

        geometry_buffer
            .index_buffer
            .as_ref()
            .context("Failed to access cube index buffer!")?
            .upload_data(INDICES, 0, command_pool, context.graphics_queue())?;

        Ok(Self { geometry_buffer })
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
            .stride((3 * std::mem::size_of::<f32>()) as _)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build();
        [vertex_input_binding_description]
    }

    pub fn draw(&self, device: &ash::Device, command_buffer: vk::CommandBuffer) -> Result<()> {
        self.geometry_buffer.bind(device, command_buffer)?;
        unsafe {
            device.cmd_draw_indexed(command_buffer, INDICES.len() as _, 1, 0, 0, 0);
        }
        Ok(())
    }
}