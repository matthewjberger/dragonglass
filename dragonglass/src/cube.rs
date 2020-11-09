use crate::{
    adapters::{
        CommandPool, DescriptorSetLayout, GraphicsPipeline, GraphicsPipelineSettingsBuilder,
        PipelineLayout, RenderPass,
    },
    context::{Context, Device},
    resources::{GeometryBuffer, ShaderCache, ShaderPathSet, ShaderPathSetBuilder},
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use nalgebra_glm as glm;
use std::sync::Arc;

#[rustfmt::skip]
pub const VERTICES: &[f32; 24] =
    &[
        // Front
       -1.0, -1.0,  1.0,
        1.0, -1.0,  1.0,
        1.0,  1.0,  1.0,
       -1.0,  1.0,  1.0,
        // Back
       -1.0, -1.0, -1.0,
        1.0, -1.0, -1.0,
        1.0,  1.0, -1.0,
       -1.0,  1.0, -1.0
    ];

#[rustfmt::skip]
pub const INDICES: &[u32; 20] =
    &[
        // Front
        0, 1, 2,
        2, 3, 0,
        // Back
        7, 6, 5,
        5, 4, 7,
        // Line Segments
        0,4,
        1,5,
        2,6,
        3,7,
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

    pub fn draw_loops(
        &self,
        device: &ash::Device,
        command_buffer: vk::CommandBuffer,
    ) -> Result<()> {
        self.geometry_buffer.bind(device, command_buffer)?;
        unsafe {
            device.cmd_draw_indexed(command_buffer, 6, 1, 0, 0, 0);
            device.cmd_draw_indexed(command_buffer, 6, 1, 6, 0, 0);
        }
        Ok(())
    }

    pub fn draw_segments(
        &self,
        device: &ash::Device,
        command_buffer: vk::CommandBuffer,
    ) -> Result<()> {
        self.geometry_buffer.bind(device, command_buffer)?;
        unsafe {
            device.cmd_draw_indexed(command_buffer, 8, 1, 12, 0, 0);
        }
        Ok(())
    }
}

pub unsafe fn byte_slice_from<T: Sized>(data: &T) -> &[u8] {
    let data_ptr = (data as *const T) as *const u8;
    std::slice::from_raw_parts(data_ptr, std::mem::size_of::<T>())
}

#[derive(Debug)]
pub struct CubePushConstantBlock {
    pub mvp: glm::Mat4,
}

pub struct CubeRendering {
    pub cube: Cube,
    pub pipeline: Option<GraphicsPipeline>,
    pub segment_pipeline: Option<GraphicsPipeline>,
    pub pipeline_layout: Option<PipelineLayout>,
    pub is_bounding_box: bool,
    pub mvp: glm::Mat4,
    device: Arc<Device>,
}

impl CubeRendering {
    pub fn new(device: Arc<Device>, cube: Cube) -> Self {
        Self {
            cube,
            pipeline: None,
            segment_pipeline: None,
            pipeline_layout: None,
            is_bounding_box: false,
            mvp: glm::Mat4::identity(),
            device,
        }
    }

    fn shader_paths() -> Result<ShaderPathSet> {
        let shader_path_set = ShaderPathSetBuilder::default()
            .vertex("assets/shaders/cube/cube.vert.spv")
            .fragment("assets/shaders/cube/cube.frag.spv")
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        Ok(shader_path_set)
    }

    pub fn create_pipeline(
        &mut self,
        shader_cache: &mut ShaderCache,
        render_pass: Arc<RenderPass>,
        samples: vk::SampleCountFlags,
    ) -> Result<()> {
        let push_constant_range = vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
            .size(std::mem::size_of::<CubePushConstantBlock>() as u32)
            .build();

        let shader_paths = Self::shader_paths()?;
        let shader_set = shader_cache.create_shader_set(self.device.clone(), &shader_paths)?;

        let descriptor_set_layout = Arc::new(DescriptorSetLayout::new(
            self.device.clone(),
            vk::DescriptorSetLayoutCreateInfo::builder(),
        )?);

        self.pipeline = None;
        self.segment_pipeline = None;
        self.pipeline_layout = None;

        let mut settings = GraphicsPipelineSettingsBuilder::default();
        settings
            .render_pass(render_pass)
            .vertex_inputs(Cube::vertex_inputs())
            .vertex_attributes(Cube::vertex_attributes())
            .descriptor_set_layout(descriptor_set_layout)
            .shader_set(shader_set)
            .rasterization_samples(samples)
            .dynamic_states(vec![vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR])
            .push_constant_range(push_constant_range)
            .polygon_mode(vk::PolygonMode::LINE)
            .topology(vk::PrimitiveTopology::LINE_STRIP)
            .dynamic_states(vec![
                vk::DynamicState::VIEWPORT,
                vk::DynamicState::SCISSOR,
                vk::DynamicState::LINE_WIDTH,
                // vk::DynamicState::DEPTH_BIAS,
            ]);

        let mut segment_settings = settings.clone();
        segment_settings.topology(vk::PrimitiveTopology::LINE_LIST);

        let (pipeline, pipeline_layout) = settings
            .build()
            .map_err(|error| anyhow!("{}", error))?
            .create_pipeline(self.device.clone())?;

        let (segment_pipeline, _) = segment_settings
            .build()
            .map_err(|error| anyhow!("{}", error))?
            .create_pipeline(self.device.clone())?;

        self.pipeline = Some(pipeline);
        self.segment_pipeline = Some(segment_pipeline);
        self.pipeline_layout = Some(pipeline_layout);

        Ok(())
    }

    pub fn issue_commands(&self, command_buffer: vk::CommandBuffer) -> Result<()> {
        let pipeline = self
            .pipeline
            .as_ref()
            .context("Failed to get wireframe pipeline for rendering asset!")?;

        let pipeline_layout = self
            .pipeline_layout
            .as_ref()
            .context("Failed to get pipeline layout for rendering asset!")?;

        pipeline.bind(&self.device.handle, command_buffer);

        let push_constants = CubePushConstantBlock { mvp: self.mvp };

        unsafe {
            self.device.handle.cmd_push_constants(
                command_buffer,
                pipeline_layout.handle,
                vk::ShaderStageFlags::ALL_GRAPHICS,
                0,
                byte_slice_from(&push_constants),
            );

            self.device.handle.cmd_set_line_width(command_buffer, 3.0);
            // self.device
            //     .handle
            //     .cmd_set_depth_bias(command_buffer, 1.25, 0.0, 1.0);
        }

        self.cube.draw_loops(&self.device.handle, command_buffer)?;

        let segment_pipeline = self
            .segment_pipeline
            .as_ref()
            .context("Failed to get wireframe pipeline for rendering asset!")?;

        segment_pipeline.bind(&self.device.handle, command_buffer);

        self.cube
            .draw_segments(&self.device.handle, command_buffer)?;

        Ok(())
    }
}
