use super::{
    core::{Context, LogicalDevice},
    render::{
        CommandPool, CpuToGpuBuffer, DescriptorPool, DescriptorSetLayout, GeometryBuffer,
        GraphicsPipeline, GraphicsPipelineSettings, GraphicsPipelineSettingsBuilder, Image,
        ImageDescription, ImageView, PipelineLayout, RenderPass, Sampler, ShaderCache,
        ShaderPathSet, ShaderPathSetBuilder,
    },
};
use anyhow::{anyhow, Result};
use ash::{version::DeviceV1_0, vk};
use imgui::{Context as ImguiContext, DrawCmd, DrawCmdParams, DrawData};
use log::{debug, warn};
use nalgebra_glm as glm;
use std::{mem, sync::Arc};
use vk_mem::Allocator;

pub struct PushConstantBlockGui {
    pub projection: glm::Mat4,
}

pub struct GuiRenderer {
    pub pipeline: Option<GraphicsPipeline>,
    pub pipeline_layout: Option<PipelineLayout>,
    pub geometry_buffer: GeometryBuffer,
    pub descriptor_pool: DescriptorPool,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub descriptor_set: vk::DescriptorSet,
    // pub font_texture: Texture,
    number_of_indices: usize,
    device: Arc<LogicalDevice>,
}

impl GuiRenderer {
    pub fn new(
        context: &Context,
        pool: &CommandPool,
        render_pass: Arc<RenderPass>,
        shader_cache: &mut ShaderCache,
        imgui: &mut ImguiContext,
    ) -> Result<Self> {
        todo!()

        // let device = context.logical_device.clone();
        // let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
        // let descriptor_pool = Self::descriptor_pool(device.clone())?;
        // let descriptor_set =
        //     descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];
        // let (geometry_buffer, number_of_indices) = Self::geometry_buffer(context, pool)?;

        // let font_texture = {
        //     let mut fonts = imgui.fonts();
        //     let atlas_texture = fonts.build_rgba32_texture();
        //     let atlas_texture_description = TextureDescription {
        //         format: vk::Format::R8G8B8A8_UNORM,
        //         width: atlas_texture.width,
        //         height: atlas_texture.height,
        //         mip_levels: 1,
        //         pixels: atlas_texture.data.to_vec(),
        //     };
        //     TextureBundle::new(context.clone(), &command_pool, &atlas_texture_description).unwrap()
        // };

        // let mut rendering = Self {
        //     pipeline: None,
        //     pipeline_layout: None,
        //     geometry_buffer,
        //     descriptor_pool,
        //     descriptor_set_layout,
        //     descriptor_set,
        //     font_texture: Self::load_image(context, pool)?,
        //     number_of_indices,
        //     device,
        // };

        // rendering.create_pipeline(render_pass, shader_cache)?;
        // rendering.update_descriptor_set(&font_texture);

        // Ok(rendering)
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
