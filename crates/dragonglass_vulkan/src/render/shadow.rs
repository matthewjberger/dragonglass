use crate::core::{
    CommandPool, Context, CpuToGpuBuffer, DescriptorPool, DescriptorSetLayout, Device,
    GraphicsPipelineSettingsBuilder, Image, ImageView, Pipeline, RenderPass, Sampler, ShaderCache,
    ShaderPathSet, ShaderPathSetBuilder,
};
use anyhow::Result;
use ash::vk;
use nalgebra_glm as glm;
use std::{mem, sync::Arc};

pub struct ShadowPushConstant {
    position: glm::Vec4,
}

pub struct ShadowMap {
    pub image: Box<dyn Image>,
    pub view: ImageView,
    pub sampler: Sampler,
}

#[derive(Debug, Clone, Copy)]
pub struct ShadowUniformBuffer {
    pub light_view: glm::Mat4,
    pub scene_view: glm::Mat4,
    pub perspective: glm::Mat4,
}

pub struct ShadowPipelineData {
    pub uniform_buffer: CpuToGpuBuffer,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub descriptor_pool: DescriptorPool,
    pub descriptor_set: vk::DescriptorSet,
}

impl ShadowPipelineData {
    pub fn new(context: &Context) -> Result<Self> {
        let device = context.device.clone();
        let allocator = context.allocator.clone();

        let uniform_buffer = CpuToGpuBuffer::uniform_buffer(
            device.clone(),
            allocator.clone(),
            mem::size_of::<ShadowUniformBuffer>() as _,
        )?;

        let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
        let descriptor_pool = Self::descriptor_pool(device.clone())?;
        let descriptor_set =
            descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];

        Ok(Self {
            uniform_buffer,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_set,
        })
    }

    pub fn descriptor_set_layout(device: Arc<Device>) -> Result<DescriptorSetLayout> {
        let ubo_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .build();
        let sampler_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build();
        let bindings = [ubo_binding, sampler_binding];
        let create_info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);
        DescriptorSetLayout::new(device, create_info)
    }

    fn descriptor_pool(device: Arc<Device>) -> Result<DescriptorPool> {
        let ubo_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
        };

        let sampler_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
        };

        let pool_sizes = [ubo_pool_size, sampler_pool_size];

        let create_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&pool_sizes)
            .max_sets(1);

        DescriptorPool::new(device, create_info)
    }

    fn update_descriptor_set(
        &self,
        context: &Context,
        device: Arc<Device>,
        shadow_map: &ShadowMap,
    ) {
        let uniform_buffer_size = mem::size_of::<ShadowUniformBuffer>() as vk::DeviceSize;

        let buffer_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.uniform_buffer.handle())
            .offset(0)
            .range(uniform_buffer_size)
            .build();
        let buffer_infos = [buffer_info];

        let shadow_map_image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(shadow_map.view.handle)
            .sampler(shadow_map.sampler.handle)
            .build();
        let shadow_map_image_infos = [shadow_map_image_info];

        let ubo_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(&buffer_infos)
            .build();

        let sampler_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&shadow_map_image_infos)
            .build();

        let descriptor_writes = [ubo_descriptor_write, sampler_descriptor_write];

        unsafe {
            device
                .handle
                .update_descriptor_sets(&descriptor_writes, &[])
        }
    }
}

pub struct ShadowMapRender {
    pub shadow_pipeline_data: ShadowPipelineData,
    pub pipeline: Option<Pipeline>,
    device: Arc<Device>,
}

impl ShadowMapRender {
    pub fn new(context: &Context, command_pool: &CommandPool) -> Result<Self> {
        let shadow_pipeline_data = ShadowPipelineData::new(context)?;
        Ok(Self {
            pipeline: None,
            shadow_pipeline_data,
            device: context.device.clone(),
        })
    }

    fn shader_paths() -> Result<ShaderPathSet> {
        let shader_path_set = ShaderPathSetBuilder::default()
            .vertex("assets/shaders/world/shadow.vert.spv")
            .build()?;
        Ok(shader_path_set)
    }

    pub fn create_pipeline(
        &mut self,
        shader_cache: &mut ShaderCache,
        render_pass: Arc<RenderPass>,
        samples: vk::SampleCountFlags,
    ) -> Result<()> {
        let push_constant_range = vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .size(mem::size_of::<ShadowPushConstant>() as u32)
            .build();

        let shader_paths = Self::shader_paths()?;
        let shader_set = shader_cache.create_shader_set(self.device.clone(), &shader_paths)?;

        let mut settings = GraphicsPipelineSettingsBuilder::default();
        settings
            .render_pass(render_pass)
            .vertex_inputs(vertex_inputs())
            .vertex_attributes(vertex_attributes())
            .descriptor_set_layout(self.shadow_pipeline_data.descriptor_set_layout.clone())
            .shader_set(shader_set)
            .rasterization_samples(samples)
            .sample_shading_enabled(true)
            .cull_mode(vk::CullModeFlags::BACK)
            .dynamic_states(vec![vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);

        self.pipeline = None;
        let (pipeline, pipeline_layout) = settings.build()?.create_pipeline(self.device.clone())?;
        self.pipeline = Some(pipeline);

        Ok(())
    }
}

fn vertex_inputs() -> [vk::VertexInputBindingDescription; 1] {
    let vertex_input_binding_description = vk::VertexInputBindingDescription::builder()
        .binding(0)
        .input_rate(vk::VertexInputRate::VERTEX)
        .build();
    [vertex_input_binding_description]
}

fn vertex_attributes() -> [vk::VertexInputAttributeDescription; 1] {}
