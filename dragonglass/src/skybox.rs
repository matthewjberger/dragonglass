use crate::{
    adapters::{
        CommandPool, DescriptorSetLayout, GraphicsPipeline, GraphicsPipelineSettingsBuilder,
        PipelineLayout, RenderPass,
    },
    context::{Context, Device},
    cube::Cube,
    resources::{ShaderCache, ShaderPathSet, ShaderPathSetBuilder},
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use nalgebra_glm as glm;
use std::sync::Arc;

pub unsafe fn byte_slice_from<T: Sized>(data: &T) -> &[u8] {
    let data_ptr = (data as *const T) as *const u8;
    std::slice::from_raw_parts(data_ptr, std::mem::size_of::<T>())
}

#[derive(Debug)]
pub struct SkyboxPushConstantBlock {
    pub view: glm::Mat4,
    pub projection: glm::Mat4,
}

pub struct SkyboxRendering {
    pub cube: Cube,
    pub pipeline: Option<GraphicsPipeline>,
    pub pipeline_layout: Option<PipelineLayout>,
    pub view: glm::Mat4,
    pub projection: glm::Mat4,
    device: Arc<Device>,
}

impl SkyboxRendering {
    pub fn new(context: &Context, command_pool: &CommandPool) -> Result<Self> {
        let cube = Cube::new(context, command_pool)?;
        Ok(Self {
            cube,
            pipeline: None,
            pipeline_layout: None,
            view: glm::Mat4::identity(),
            projection: glm::Mat4::identity(),
            device: context.device.clone(),
        })
    }

    fn shader_paths() -> Result<ShaderPathSet> {
        let shader_path_set = ShaderPathSetBuilder::default()
            .vertex("assets/shaders/skybox/skybox.vert.spv")
            .fragment("assets/shaders/skybox/skybox.frag.spv")
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
            .size(std::mem::size_of::<SkyboxPushConstantBlock>() as u32)
            .build();

        let shader_paths = Self::shader_paths()?;
        let shader_set = shader_cache.create_shader_set(self.device.clone(), &shader_paths)?;

        let descriptor_set_layout = Arc::new(DescriptorSetLayout::new(
            self.device.clone(),
            vk::DescriptorSetLayoutCreateInfo::builder(),
        )?);

        let mut settings = GraphicsPipelineSettingsBuilder::default();
        settings
            .render_pass(render_pass)
            .vertex_inputs(Cube::vertex_inputs())
            .vertex_attributes(Cube::vertex_attributes())
            .descriptor_set_layout(descriptor_set_layout)
            .shader_set(shader_set)
            .rasterization_samples(samples)
            .depth_test_enabled(false)
            .depth_write_enabled(false)
            .cull_mode(vk::CullModeFlags::FRONT)
            .push_constant_range(push_constant_range);

        let mut wireframe_settings = settings.clone();
        wireframe_settings.polygon_mode(vk::PolygonMode::LINE);

        self.pipeline = None;
        self.pipeline_layout = None;

        // TODO: Reuse the pipeline layout across these pipelines since they are the same
        let (pipeline, pipeline_layout) = settings
            .build()
            .map_err(|error| anyhow!("{}", error))?
            .create_pipeline(self.device.clone())?;

        self.pipeline = Some(pipeline);
        self.pipeline_layout = Some(pipeline_layout);

        Ok(())
    }

    pub fn issue_commands(&self, command_buffer: vk::CommandBuffer) -> Result<()> {
        let pipeline = self
            .pipeline
            .as_ref()
            .context("Failed to get pipeline for rendering asset!")?;

        let pipeline_layout = self
            .pipeline_layout
            .as_ref()
            .context("Failed to get pipeline layout for rendering asset!")?;

        pipeline.bind(&self.device.handle, command_buffer);

        let push_constants = SkyboxPushConstantBlock {
            view: self.view,
            projection: self.projection,
        };

        unsafe {
            self.device.handle.cmd_push_constants(
                command_buffer,
                pipeline_layout.handle,
                vk::ShaderStageFlags::ALL_GRAPHICS,
                0,
                byte_slice_from(&push_constants),
            );
        }

        self.cube.draw(&self.device.handle, command_buffer)?;

        Ok(())
    }
}
