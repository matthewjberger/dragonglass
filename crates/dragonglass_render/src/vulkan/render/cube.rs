use crate::{
    renderer::byte_slice_from,
    vulkan::{
        core::{
            DescriptorSetLayout, Device, GraphicsPipelineSettingsBuilder, Pipeline, PipelineLayout,
            RenderPass, ShaderCache, ShaderPathSet, ShaderPathSetBuilder,
        },
        cube::Cube,
    },
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use nalgebra_glm as glm;
use std::sync::Arc;

#[derive(Debug)]
pub struct CubePushConstantBlock {
    pub mvp: glm::Mat4,
    pub color: glm::Vec4,
}

pub struct CubeRender {
    pub cube: Cube,
    pub solid_pipeline: Option<Pipeline>,
    pub loop_pipeline: Option<Pipeline>,
    pub segment_pipeline: Option<Pipeline>,
    pub pipeline_layout: Option<PipelineLayout>,
    device: Arc<Device>,
}

impl CubeRender {
    pub fn new(device: Arc<Device>, cube: Cube) -> Self {
        Self {
            cube,
            solid_pipeline: None,
            loop_pipeline: None,
            segment_pipeline: None,
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
            .size(std::mem::size_of::<CubePushConstantBlock>() as u32)
            .build();

        let shader_paths = Self::shader_paths()?;
        let shader_set = shader_cache.create_shader_set(self.device.clone(), &shader_paths)?;

        let descriptor_set_layout = Arc::new(DescriptorSetLayout::new(
            self.device.clone(),
            vk::DescriptorSetLayoutCreateInfo::builder(),
        )?);

        self.loop_pipeline = None;
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
            .push_constant_range(push_constant_range);

        let mut solid_settings = settings.clone();
        solid_settings
            .polygon_mode(vk::PolygonMode::FILL)
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .dynamic_states(vec![vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);

        let mut loop_settings = settings.clone();
        loop_settings
            .polygon_mode(vk::PolygonMode::LINE)
            .topology(vk::PrimitiveTopology::LINE_STRIP)
            .dynamic_states(vec![
                vk::DynamicState::VIEWPORT,
                vk::DynamicState::SCISSOR,
                vk::DynamicState::LINE_WIDTH,
                vk::DynamicState::DEPTH_BIAS,
            ]);

        let mut segment_settings = settings;
        segment_settings
            .polygon_mode(vk::PolygonMode::LINE)
            .topology(vk::PrimitiveTopology::LINE_LIST)
            .dynamic_states(vec![
                vk::DynamicState::VIEWPORT,
                vk::DynamicState::SCISSOR,
                vk::DynamicState::LINE_WIDTH,
                vk::DynamicState::DEPTH_BIAS,
            ]);

        let (solid_pipeline, pipeline_layout) = solid_settings
            .build()
            .map_err(|error| anyhow!("{}", error))?
            .create_pipeline(self.device.clone())?;

        let (loop_pipeline, _) = loop_settings
            .build()
            .map_err(|error| anyhow!("{}", error))?
            .create_pipeline(self.device.clone())?;

        let (segment_pipeline, _) = segment_settings
            .build()
            .map_err(|error| anyhow!("{}", error))?
            .create_pipeline(self.device.clone())?;

        self.solid_pipeline = Some(solid_pipeline);
        self.loop_pipeline = Some(loop_pipeline);
        self.segment_pipeline = Some(segment_pipeline);
        self.pipeline_layout = Some(pipeline_layout);

        Ok(())
    }

    pub fn issue_commands(
        &self,
        command_buffer: vk::CommandBuffer,
        mvp: glm::Mat4,
        color: glm::Vec4,
        solid: bool,
    ) -> Result<()> {
        let solid_pipeline = self
            .solid_pipeline
            .as_ref()
            .context("Failed to get solid pipeline for rendering asset!")?;

        let loop_pipeline = self
            .loop_pipeline
            .as_ref()
            .context("Failed to get wireframe pipeline for rendering asset!")?;

        let pipeline_layout = self
            .pipeline_layout
            .as_ref()
            .context("Failed to get pipeline layout for rendering asset!")?;

        let push_constants = CubePushConstantBlock { mvp, color };
        unsafe {
            self.device.handle.cmd_push_constants(
                command_buffer,
                pipeline_layout.handle,
                vk::ShaderStageFlags::ALL_GRAPHICS,
                0,
                byte_slice_from(&push_constants),
            );
        }

        if solid {
            solid_pipeline.bind(&self.device.handle, command_buffer);
            self.cube.draw(&self.device.handle, command_buffer)?;
        } else {
            loop_pipeline.bind(&self.device.handle, command_buffer);
            unsafe {
                self.device.handle.cmd_set_line_width(command_buffer, 3.0);
                self.device
                    .handle
                    .cmd_set_depth_bias(command_buffer, 1.25, 0.0, 1.0);
            }
            self.cube.draw_loops(&self.device.handle, command_buffer)?;
            let segment_pipeline = self
                .segment_pipeline
                .as_ref()
                .context("Failed to get wireframe pipeline for rendering asset!")?;
            segment_pipeline.bind(&self.device.handle, command_buffer);
            self.cube
                .draw_segments(&self.device.handle, command_buffer)?;
        }

        Ok(())
    }
}
