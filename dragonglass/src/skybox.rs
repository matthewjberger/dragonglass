use crate::{
    core::{
        CommandPool, Context, DescriptorPool, DescriptorSetLayout, Device, GraphicsPipeline,
        GraphicsPipelineSettingsBuilder, PipelineLayout, RenderPass, ShaderCache, ShaderPathSet,
        ShaderPathSetBuilder,
    },
    cube::Cube,
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
    _descriptor_pool: DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    descriptor_set_layout: Arc<DescriptorSetLayout>,
    device: Arc<Device>,
}

impl SkyboxRendering {
    pub fn new(
        context: &Context,
        command_pool: &CommandPool,
        image_view: vk::ImageView,
        sampler: vk::Sampler,
    ) -> Result<Self> {
        let cube = Cube::new(context, command_pool)?;
        let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(context.device.clone())?);
        let descriptor_pool = Self::descriptor_pool(context.device.clone())?;
        let descriptor_set =
            descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];
        let rendering = Self {
            cube,
            pipeline: None,
            pipeline_layout: None,
            view: glm::Mat4::identity(),
            projection: glm::Mat4::identity(),
            _descriptor_pool: descriptor_pool,
            descriptor_set,
            descriptor_set_layout,
            device: context.device.clone(),
        };
        rendering.update_descriptor_set(context.device.clone(), image_view, sampler);
        Ok(rendering)
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

        let mut settings = GraphicsPipelineSettingsBuilder::default();
        settings
            .render_pass(render_pass)
            .vertex_inputs(Cube::vertex_inputs())
            .vertex_attributes(Cube::vertex_attributes())
            .descriptor_set_layout(self.descriptor_set_layout.clone())
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
            .context("Failed to get pipeline for rendering world!")?;

        let pipeline_layout = self
            .pipeline_layout
            .as_ref()
            .context("Failed to get pipeline layout for rendering world!")?;

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

            self.device.handle.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline_layout.handle,
                0,
                &[self.descriptor_set],
                &[],
            );
        }

        self.cube.draw(&self.device.handle, command_buffer)?;

        Ok(())
    }

    pub fn descriptor_set_layout(device: Arc<Device>) -> Result<DescriptorSetLayout> {
        let sampler_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_count(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build();
        let bindings = [sampler_binding];

        let create_info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);
        DescriptorSetLayout::new(device, create_info)
    }

    fn descriptor_pool(device: Arc<Device>) -> Result<DescriptorPool> {
        let sampler_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
        };

        let pool_sizes = [sampler_pool_size];

        let pool_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&pool_sizes)
            .max_sets(1);

        DescriptorPool::new(device, pool_info)
    }

    pub fn update_descriptor_set(
        &self,
        device: Arc<Device>,
        image_view: vk::ImageView,
        sampler: vk::Sampler,
    ) {
        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(image_view)
            .sampler(sampler)
            .build();
        let image_infos = [image_info];

        let sampler_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_infos)
            .build();

        let descriptor_writes = vec![sampler_descriptor_write];

        unsafe {
            device
                .handle
                .update_descriptor_sets(&descriptor_writes, &[])
        }
    }
}
