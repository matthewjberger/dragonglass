use crate::vulkan::core::{
    CommandPool, Context, DescriptorPool, DescriptorSetLayout, Device, GraphicsPipelineSettings,
    GraphicsPipelineSettingsBuilder, Image, ImageNode, ImageView, Pipeline, PipelineLayout,
    RenderGraph, RenderPass, Sampler, ShaderCache, ShaderPathSet, ShaderPathSetBuilder,
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use std::sync::Arc;

pub struct Brdflut {
    pub image: Box<dyn Image>,
    pub view: ImageView,
    pub sampler: Sampler,
}

impl Brdflut {
    pub fn new(
        context: &Context,
        command_pool: &CommandPool,
        shader_cache: &mut ShaderCache,
    ) -> Result<Brdflut> {
        let device = context.device.clone();
        let allocator = context.allocator.clone();

        let dimension = 512;
        let extent = vk::Extent2D::builder()
            .width(dimension)
            .height(dimension)
            .build();

        let fullscreen = "fullscreen";
        let color = "color";
        let mut rendergraph = RenderGraph::new(
            &[fullscreen],
            vec![ImageNode {
                name: color.to_string(),
                extent: extent,
                format: vk::Format::R16G16_SFLOAT,
                clear_value: vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [1.0, 1.0, 1.0, 1.0],
                    },
                },
                samples: vk::SampleCountFlags::TYPE_1,
                force_store: true,
            }],
            &[(fullscreen, &color)],
        )?;

        rendergraph.build(device.clone(), allocator)?;

        let fullscreen_pass = rendergraph.pass_handle(fullscreen)?;
        let pipeline = FullscreenRender::new(
            context,
            fullscreen_pass,
            shader_cache,
            rendergraph.image_view(&color)?.handle,
            rendergraph.sampler("default")?.handle,
        )?;

        command_pool.execute_once(|command_buffer| {
            rendergraph.execute_pass(command_buffer, fullscreen, 0, |pass, command_buffer| {
                device.update_viewport(command_buffer, pass.extent, true)?;
                pipeline.issue_commands(command_buffer)?;
                Ok(())
            })
        })?;

        let (image, view) = rendergraph.take_image(&color)?;

        Ok(Brdflut {
            image,
            view,
            sampler: Sampler::default(device)?,
        })
    }
}

// FIXME_BRDFLUT: This needs to reuse the fullscreenrender that already exists!
pub struct FullscreenRender {
    pub pipeline: Option<Pipeline>,
    pub pipeline_layout: PipelineLayout,
    pub descriptor_pool: DescriptorPool,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub descriptor_set: vk::DescriptorSet,
    device: Arc<Device>,
}

impl FullscreenRender {
    pub fn new(
        context: &Context,
        render_pass: Arc<RenderPass>,
        shader_cache: &mut ShaderCache,
        color_target: vk::ImageView,
        sampler: vk::Sampler,
    ) -> Result<Self> {
        let device = context.device.clone();
        let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
        let descriptor_pool = Self::descriptor_pool(device.clone())?;
        let descriptor_set =
            descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];
        let settings = Self::settings(
            device.clone(),
            shader_cache,
            render_pass,
            descriptor_set_layout.clone(),
        )?;
        let (pipeline, pipeline_layout) = settings.create_pipeline(device.clone())?;
        let mut rendering = Self {
            pipeline: Some(pipeline),
            pipeline_layout,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_set,
            device,
        };
        rendering.update_descriptor_set(color_target, sampler);
        Ok(rendering)
    }

    fn shader_paths() -> Result<ShaderPathSet> {
        let shader_path_set = ShaderPathSetBuilder::default()
            .vertex("assets/shaders/postprocessing/fullscreen_triangle.vert.spv")
            .fragment("assets/shaders/environment/genbrdflut.frag.spv")
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        Ok(shader_path_set)
    }

    fn settings(
        device: Arc<Device>,
        shader_cache: &mut ShaderCache,
        render_pass: Arc<RenderPass>,
        descriptor_set_layout: Arc<DescriptorSetLayout>,
    ) -> Result<GraphicsPipelineSettings> {
        let shader_paths = Self::shader_paths()?;
        let shader_set = shader_cache.create_shader_set(device, &shader_paths)?;
        let settings = GraphicsPipelineSettingsBuilder::default()
            .shader_set(shader_set)
            .render_pass(render_pass)
            .vertex_inputs(Vec::new())
            .vertex_attributes(Vec::new())
            .descriptor_set_layout(descriptor_set_layout)
            .dynamic_states(vec![vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR])
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        Ok(settings)
    }

    fn descriptor_pool(device: Arc<Device>) -> Result<DescriptorPool> {
        let sampler_pool_size = vk::DescriptorPoolSize::builder()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .build();
        let pool_sizes = [sampler_pool_size];

        let pool_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&pool_sizes)
            .max_sets(1);

        DescriptorPool::new(device, pool_info)
    }

    fn descriptor_set_layout(device: Arc<Device>) -> Result<DescriptorSetLayout> {
        let sampler_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build();
        let bindings = [sampler_binding];

        let create_info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);
        DescriptorSetLayout::new(device, create_info)
    }

    fn update_descriptor_set(&mut self, target: vk::ImageView, sampler: vk::Sampler) {
        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(target)
            .sampler(sampler);
        let image_info_list = [image_info.build()];

        let sampler_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_info_list);

        let writes = &[sampler_write.build()];
        unsafe { self.device.handle.update_descriptor_sets(writes, &[]) }
    }

    pub fn issue_commands(&self, command_buffer: vk::CommandBuffer) -> Result<()> {
        let pipeline = self
            .pipeline
            .as_ref()
            .context("Failed to get fullscreen pipeline!")?;
        pipeline.bind(&self.device.handle, command_buffer);

        unsafe {
            self.device.handle.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout.handle,
                0,
                &[self.descriptor_set],
                &[],
            );

            self.device.handle.cmd_draw(command_buffer, 3, 1, 0, 0);
        };

        Ok(())
    }
}
