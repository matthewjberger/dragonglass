use super::{
    core::{Context, LogicalDevice},
    render::{
        CommandPool, CpuToGpuBuffer, DescriptorPool, DescriptorSetLayout, GeometryBuffer,
        GraphicsPipeline, GraphicsPipelineSettingsBuilder, ImageBundle, ImageDescription,
        PipelineLayout, RenderPass, ShaderCache, ShaderPathSetBuilder,
    },
};
use anyhow::{anyhow, Result};
use ash::{version::DeviceV1_0, vk};
use nalgebra_glm as glm;
use std::{mem, sync::Arc};

pub struct UniformBuffer {
    pub view: glm::Mat4,
    pub projection: glm::Mat4,
    pub model: glm::Mat4,
}

pub struct Scene {
    pub pipeline: GraphicsPipeline,
    pub pipeline_layout: PipelineLayout,
    pub geometry_buffer: GeometryBuffer,
    pub uniform_buffer: CpuToGpuBuffer,
    pub descriptor_pool: DescriptorPool,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub descriptor_set: vk::DescriptorSet,
    pub image_bundle: ImageBundle,
    number_of_indices: usize,
    device: Arc<LogicalDevice>,
}

impl Scene {
    pub fn new(
        context: Arc<Context>,
        pool: &CommandPool,
        render_pass: Arc<RenderPass>,
        shader_cache: &mut ShaderCache,
    ) -> Result<Self> {
        let device = context.logical_device.clone();
        let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
        let descriptor_pool = Self::descriptor_pool(device.clone())?;
        let descriptor_set =
            descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];

        let shader_paths = ShaderPathSetBuilder::default()
            .vertex("dragonglass/shaders/triangle/triangle.vert.spv")
            .fragment("dragonglass/shaders/triangle/triangle.frag.spv")
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        let shader_set = shader_cache.create_shader_set(device.clone(), &shader_paths)?;

        let descriptions = Self::vertex_input_descriptions();
        let attributes = Self::vertex_attributes();
        let vertex_state_info = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&descriptions)
            .vertex_attribute_descriptions(&attributes)
            .build();

        let settings = GraphicsPipelineSettingsBuilder::default()
            .shader_set(shader_set)
            .render_pass(render_pass)
            .vertex_state_info(vertex_state_info)
            .descriptor_set_layout(descriptor_set_layout.clone())
            .build()
            .map_err(|error| anyhow!("{}", error))?;

        let (pipeline, pipeline_layout) =
            GraphicsPipeline::from_settings(device.clone(), settings)?;

        #[rustfmt::skip]
        let vertices: [f32; 16] = [
           -0.5, -0.5, 0.0, 0.0,
            0.5,  0.5, 1.0, 1.0,
            0.5, -0.5, 1.0, 0.0,
           -0.5,  0.5, 0.0, 1.0,
        ];

        let indices: [u32; 6] = [0, 1, 2, 3, 1, 0];
        let number_of_indices = indices.len();

        let geometry_buffer = GeometryBuffer::new(
            context.allocator.clone(),
            (vertices.len() * std::mem::size_of::<f32>()) as _,
            Some((indices.len() * std::mem::size_of::<u32>()) as _),
        )?;

        geometry_buffer
            .vertex_buffer
            .upload_data(&vertices, 0, pool, context.graphics_queue())?;

        geometry_buffer
            .index_buffer
            .as_ref()
            .ok_or_else(|| anyhow!("Failed to access index buffer!"))?
            .upload_data(&indices, 0, pool, context.graphics_queue())?;

        let uniform_buffer = CpuToGpuBuffer::uniform_buffer(
            context.allocator.clone(),
            mem::size_of::<UniformBuffer>() as _,
        )?;

        let description = ImageDescription::from_file("dragonglass/textures/stone.png")?;
        let image_bundle = ImageBundle::new(
            context.logical_device.clone(),
            context.graphics_queue(),
            context.allocator.clone(),
            pool,
            &description,
        )?;

        let mut rendering = Self {
            pipeline,
            pipeline_layout,
            geometry_buffer,
            uniform_buffer,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_set,
            image_bundle,
            number_of_indices,
            device,
        };

        rendering.update_descriptor_set();

        Ok(rendering)
    }

    pub fn vertex_attributes() -> [vk::VertexInputAttributeDescription; 2] {
        let position_description = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0)
            .build();

        let tex_coord_description = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(1)
            .format(vk::Format::R32G32_SFLOAT)
            .offset((std::mem::size_of::<f32>() * 2) as _)
            .build();

        [position_description, tex_coord_description]
    }

    pub fn vertex_input_descriptions() -> [vk::VertexInputBindingDescription; 1] {
        let vertex_input_binding_description = vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride((4 * std::mem::size_of::<f32>()) as _)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build();
        [vertex_input_binding_description]
    }

    pub fn descriptor_pool(device: Arc<LogicalDevice>) -> Result<DescriptorPool> {
        // TODO: Replace with builders
        let ubo_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
        };

        let sampler_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
        };

        let pool_sizes = [ubo_pool_size, sampler_pool_size];

        let pool_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&pool_sizes)
            .max_sets(1);

        DescriptorPool::new(device, pool_info)
    }

    pub fn descriptor_set_layout(device: Arc<LogicalDevice>) -> Result<DescriptorSetLayout> {
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

    fn update_descriptor_set(&mut self) {
        let buffer_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.uniform_buffer.handle())
            .offset(0)
            .range(std::mem::size_of::<UniformBuffer>() as _)
            .build();
        let buffer_info_list = [buffer_info];

        let ubo_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(&buffer_info_list)
            .build();

        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(self.image_bundle.view.handle)
            .sampler(self.image_bundle.sampler.handle)
            .build();
        let image_info_list = [image_info];

        let sampler_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_info_list)
            .build();

        unsafe {
            self.device
                .handle
                .update_descriptor_sets(&[ubo_descriptor_write, sampler_descriptor_write], &[])
        }
    }

    pub fn update_ubo(&self, aspect_ratio: f32) -> Result<()> {
        let projection = glm::perspective_zo(aspect_ratio, 70_f32.to_radians(), 0.1_f32, 1000_f32);

        let view = glm::look_at(
            &glm::vec3(1.0, 0.0, -1.0),
            &glm::vec3(0.0, 0.0, 0.0),
            &glm::vec3(0.0, 1.0, 0.0),
        );

        let ubo = UniformBuffer {
            view,
            projection,
            model: glm::Mat4::identity(),
        };

        self.uniform_buffer.upload_data(&[ubo], 0)?;

        Ok(())
    }

    pub fn issue_commands(&self, command_buffer: vk::CommandBuffer) -> Result<()> {
        self.pipeline.bind(&self.device.handle, command_buffer);

        self.geometry_buffer
            .bind(&self.device.handle, command_buffer)?;

        unsafe {
            self.device.handle.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout.handle,
                0,
                &[self.descriptor_set],
                &[],
            );

            self.device.handle.cmd_draw_indexed(
                command_buffer,
                self.number_of_indices as _,
                1,
                0,
                0,
                0,
            )
        };

        Ok(())
    }
}
