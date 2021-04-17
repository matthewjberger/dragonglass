use crate::{
    byte_slice_from,
    core::{
        DescriptorSetLayout, Device, GraphicsPipelineSettingsBuilder, Pipeline, PipelineLayout,
        RenderPass, ShaderCache, ShaderPathSet, ShaderPathSetBuilder,
    },
    geometry::{Shape, ShapeBuffer},
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use nalgebra_glm as glm;
use std::sync::Arc;

#[derive(Debug)]
pub struct ShapePushConstantBlock {
    pub mvp: glm::Mat4,
    pub color: glm::Vec4,
}

pub struct ShapeRender {
    pub shape_buffer: ShapeBuffer,
    pub solid_pipeline: Option<Pipeline>,
    pub pipeline_layout: Option<PipelineLayout>,
    device: Arc<Device>,
}

impl ShapeRender {
    pub fn new(device: Arc<Device>, shape_buffer: ShapeBuffer) -> Self {
        Self {
            shape_buffer,
            solid_pipeline: None,
            pipeline_layout: None,
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
            .size(std::mem::size_of::<ShapePushConstantBlock>() as u32)
            .build();

        let shader_paths = Self::shader_paths()?;
        let shader_set = shader_cache.create_shader_set(self.device.clone(), &shader_paths)?;

        let descriptor_set_layout = Arc::new(DescriptorSetLayout::new(
            self.device.clone(),
            vk::DescriptorSetLayoutCreateInfo::builder(),
        )?);

        self.pipeline_layout = None;

        let mut settings = GraphicsPipelineSettingsBuilder::default();
        settings
            .render_pass(render_pass)
            .vertex_inputs(ShapeBuffer::vertex_inputs())
            .vertex_attributes(ShapeBuffer::vertex_attributes())
            .descriptor_set_layout(descriptor_set_layout)
            .shader_set(shader_set)
            .rasterization_samples(samples)
            .push_constant_range(push_constant_range);

        let mut solid_settings = settings.clone();
        solid_settings
            .polygon_mode(vk::PolygonMode::LINE)
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .dynamic_states(vec![vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);

        let (solid_pipeline, pipeline_layout) = solid_settings
            .build()
            .map_err(|error| anyhow!("{}", error))?
            .create_pipeline(self.device.clone())?;

        self.solid_pipeline = Some(solid_pipeline);
        self.pipeline_layout = Some(pipeline_layout);

        Ok(())
    }

    pub fn issue_commands(
        &self,
        command_buffer: vk::CommandBuffer,
        mvp: glm::Mat4,
        color: glm::Vec4,
        shape: &Shape,
    ) -> Result<()> {
        let solid_pipeline = self
            .solid_pipeline
            .as_ref()
            .context("Failed to get solid pipeline for rendering asset!")?;

        let pipeline_layout = self
            .pipeline_layout
            .as_ref()
            .context("Failed to get pipeline layout for rendering asset!")?;

        let push_constants = ShapePushConstantBlock { mvp, color };
        unsafe {
            self.device.handle.cmd_push_constants(
                command_buffer,
                pipeline_layout.handle,
                vk::ShaderStageFlags::ALL_GRAPHICS,
                0,
                byte_slice_from(&push_constants),
            );
        }

        solid_pipeline.bind(&self.device.handle, command_buffer);
        self.shape_buffer
            .draw(&self.device.handle, command_buffer, shape)?;

        Ok(())
    }
}
