use crate::{
    adapters::{
        CommandPool, DescriptorPool, DescriptorSetLayout, GraphicsPipeline,
        GraphicsPipelineSettings, GraphicsPipelineSettingsBuilder, PipelineLayout, RenderPass,
    },
    context::{Context, LogicalDevice},
    resources::{
        CpuToGpuBuffer, GeometryBuffer, Image, ImageDescription, ImageView, Sampler, ShaderCache,
        ShaderPathSet, ShaderPathSetBuilder,
    },
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use nalgebra_glm as glm;
use std::{mem, sync::Arc};
use vk_mem::Allocator;

pub struct UniformBuffer {
    pub view: glm::Mat4,
    pub projection: glm::Mat4,
    pub model: glm::Mat4,
}

pub struct Scene {
    pub pipeline: Option<GraphicsPipeline>,
    pub pipeline_layout: Option<PipelineLayout>,
    pub geometry_buffer: GeometryBuffer,
    pub uniform_buffer: CpuToGpuBuffer,
    pub descriptor_pool: DescriptorPool,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub descriptor_set: vk::DescriptorSet,
    pub texture: Texture,
    number_of_indices: usize,
    device: Arc<LogicalDevice>,
}

impl Scene {
    pub fn new(
        context: &Context,
        pool: &CommandPool,
        render_pass: Arc<RenderPass>,
        shader_cache: &mut ShaderCache,
    ) -> Result<Self> {
        let device = context.logical_device.clone();
        let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
        let descriptor_pool = Self::descriptor_pool(device.clone())?;
        let descriptor_set =
            descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];
        let (geometry_buffer, number_of_indices) = Self::geometry_buffer(context, pool)?;

        let mut rendering = Self {
            pipeline: None,
            pipeline_layout: None,
            geometry_buffer,
            uniform_buffer: Self::uniform_buffer(context.allocator.clone())?,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_set,
            texture: Self::load_image(context, pool)?,
            number_of_indices,
            device,
        };

        rendering.create_pipeline(render_pass, shader_cache)?;
        rendering.update_descriptor_set();

        Ok(rendering)
    }

    pub fn create_pipeline(
        &mut self,
        render_pass: Arc<RenderPass>,
        mut shader_cache: &mut ShaderCache,
    ) -> Result<()> {
        let settings = Self::settings(
            self.device.clone(),
            &mut shader_cache,
            render_pass,
            self.descriptor_set_layout.clone(),
        )?;
        self.pipeline = None;
        self.pipeline_layout = None;
        let (pipeline, pipeline_layout) = settings.create_pipeline(self.device.clone())?;
        self.pipeline = Some(pipeline);
        self.pipeline_layout = Some(pipeline_layout);
        Ok(())
    }

    fn geometry_buffer(context: &Context, pool: &CommandPool) -> Result<(GeometryBuffer, usize)> {
        let vertices: [f32; 16] = [
            -0.5, -0.5, 0.0, 0.0, 0.5, 0.5, 1.0, 1.0, 0.5, -0.5, 1.0, 0.0, -0.5, 0.5, 0.0, 1.0,
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
            .context("Failed to access index buffer!")?
            .upload_data(&indices, 0, pool, context.graphics_queue())?;

        Ok((geometry_buffer, number_of_indices))
    }

    fn shader_paths() -> Result<ShaderPathSet> {
        let shader_path_set = ShaderPathSetBuilder::default()
            .vertex("assets/shaders/triangle/triangle.vert.spv")
            .fragment("assets/shaders/triangle/triangle.frag.spv")
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        Ok(shader_path_set)
    }

    fn settings(
        device: Arc<LogicalDevice>,
        shader_cache: &mut ShaderCache,
        render_pass: Arc<RenderPass>,
        descriptor_set_layout: Arc<DescriptorSetLayout>,
    ) -> Result<GraphicsPipelineSettings> {
        let shader_paths = Self::shader_paths()?;
        let shader_set = shader_cache.create_shader_set(device, &shader_paths)?;
        let descriptions = Self::vertex_input_descriptions();
        let attributes = Self::vertex_attributes();
        let vertex_state_info = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&descriptions)
            .vertex_attribute_descriptions(&attributes);
        let settings = GraphicsPipelineSettingsBuilder::default()
            .shader_set(shader_set)
            .render_pass(render_pass)
            .vertex_state_info(vertex_state_info.build())
            .descriptor_set_layout(descriptor_set_layout)
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        Ok(settings)
    }

    fn uniform_buffer(allocator: Arc<Allocator>) -> Result<CpuToGpuBuffer> {
        CpuToGpuBuffer::uniform_buffer(allocator, mem::size_of::<UniformBuffer>() as _)
    }

    fn load_image(context: &Context, pool: &CommandPool) -> Result<Texture> {
        let description = ImageDescription::from_file("assets/textures/stone.png")?;
        Texture::new(context, pool, &description)
    }

    fn vertex_attributes() -> [vk::VertexInputAttributeDescription; 2] {
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

    fn vertex_input_descriptions() -> [vk::VertexInputBindingDescription; 1] {
        let vertex_input_binding_description = vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride((4 * std::mem::size_of::<f32>()) as _)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build();
        [vertex_input_binding_description]
    }

    fn descriptor_pool(device: Arc<LogicalDevice>) -> Result<DescriptorPool> {
        let ubo_pool_size = vk::DescriptorPoolSize::builder()
            .ty(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .build();
        let sampler_pool_size = vk::DescriptorPoolSize::builder()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .build();
        let pool_sizes = [ubo_pool_size, sampler_pool_size];

        let pool_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&pool_sizes)
            .max_sets(1);

        DescriptorPool::new(device, pool_info)
    }

    fn descriptor_set_layout(device: Arc<LogicalDevice>) -> Result<DescriptorSetLayout> {
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
            .range(std::mem::size_of::<UniformBuffer>() as _);
        let buffer_info_list = [buffer_info.build()];

        let ubo_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(&buffer_info_list);

        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(self.texture.view.handle)
            .sampler(self.texture.sampler.handle);
        let image_info_list = [image_info.build()];

        let sampler_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_info_list);

        let writes = &[ubo_write.build(), sampler_write.build()];
        unsafe { self.device.handle.update_descriptor_sets(writes, &[]) }
    }

    pub fn update_ubo(&self, aspect_ratio: f32, view: glm::Mat4) -> Result<()> {
        let projection = glm::perspective_zo(aspect_ratio, 70_f32.to_radians(), 0.1_f32, 1000_f32);

        let ubo = UniformBuffer {
            view,
            projection,
            model: glm::Mat4::identity(),
        };

        self.uniform_buffer.upload_data(&[ubo], 0)?;

        Ok(())
    }

    pub fn issue_commands(&self, command_buffer: vk::CommandBuffer) -> Result<()> {
        self.pipeline
            .as_ref()
            .context("Failed to get scene pipeline!")?
            .bind(&self.device.handle, command_buffer);

        let pipeline_layout = self
            .pipeline_layout
            .as_ref()
            .context("Failed to get scene pipeline layout!")?
            .handle;

        self.geometry_buffer
            .bind(&self.device.handle, command_buffer)?;

        unsafe {
            self.device.handle.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline_layout,
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

pub struct Texture {
    pub image: Image,
    pub view: ImageView,
    pub sampler: Sampler,
}

impl Texture {
    pub fn new(
        context: &Context,
        command_pool: &CommandPool,
        description: &ImageDescription,
    ) -> Result<Self> {
        let image = description.as_image(context.allocator.clone())?;
        image.upload_data(context, command_pool, description)?;
        let view = Self::create_image_view(context.logical_device.clone(), &image, description)?;
        let sampler = Self::create_sampler(context.logical_device.clone(), description.mip_levels)?;

        let texture = Self {
            image,
            view,
            sampler,
        };

        Ok(texture)
    }

    fn create_image_view(
        device: Arc<LogicalDevice>,
        image: &Image,
        description: &ImageDescription,
    ) -> Result<ImageView> {
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(1)
            .level_count(description.mip_levels);

        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image.handle)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(description.format)
            .components(vk::ComponentMapping::default())
            .subresource_range(subresource_range.build());

        ImageView::new(device, create_info)
    }

    fn create_sampler(device: Arc<LogicalDevice>, mip_levels: u32) -> Result<Sampler> {
        let sampler_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(true)
            .max_anisotropy(16.0)
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .compare_op(vk::CompareOp::ALWAYS)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(mip_levels as _);
        Sampler::new(device, sampler_info)
    }
}
