use crate::{
    adapters::GraphicsPipeline,
    adapters::PipelineLayout,
    adapters::RenderPass,
    adapters::{
        CommandPool, DescriptorPool, DescriptorSetLayout, GraphicsPipelineSettingsBuilder,
        ImageToImageCopyBuilder,
    },
    context::{Context, Device},
    cube::Cube,
    rendergraph::{ImageNode, RenderGraph},
    resources::{
        transition_image, Cubemap, ImageDescription, ImageLayoutTransitionBuilder, Sampler,
        ShaderCache, ShaderPathSet, ShaderPathSetBuilder, Texture,
    },
};
use anyhow::{anyhow, Result};
use ash::{version::DeviceV1_0, vk};
use nalgebra_glm as glm;
use std::{mem, path::Path, slice, sync::Arc};
use vk_mem::Allocator;

pub unsafe fn byte_slice_from<T: Sized>(data: &T) -> &[u8] {
    let data_ptr = (data as *const T) as *const u8;
    slice::from_raw_parts(data_ptr, mem::size_of::<T>())
}

#[allow(dead_code)]
struct PushConstantHdr {
    mvp: glm::Mat4,
}

pub fn hdr_cubemap(
    context: &Context,
    command_pool: &CommandPool,
    path: impl AsRef<Path>,
    shader_cache: &mut ShaderCache,
) -> Result<(Cubemap, Sampler)> {
    let hdr_description = ImageDescription::from_hdr(path)?;
    let hdr_texture = Texture::new(context, command_pool, &hdr_description)?;

    let sampler_info = vk::SamplerCreateInfo::builder()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .anisotropy_enable(true)
        .max_anisotropy(16.0)
        .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
        .unnormalized_coordinates(false)
        .compare_enable(false)
        .compare_op(vk::CompareOp::ALWAYS)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .mip_lod_bias(0.0)
        .min_lod(0.0)
        .max_lod(0.0);
    let hdr_sampler = Sampler::new(context.device.clone(), sampler_info)?;

    let cubemap_description = ImageDescription::empty(
        hdr_description.width,
        hdr_description.width,
        vk::Format::R32G32B32A32_SFLOAT,
    );
    let cubemap = Cubemap::new(context, command_pool, &cubemap_description)?;

    let sampler_info = vk::SamplerCreateInfo::builder()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .anisotropy_enable(true)
        .max_anisotropy(16.0)
        .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
        .unnormalized_coordinates(false)
        .compare_enable(false)
        .compare_op(vk::CompareOp::ALWAYS)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .mip_lod_bias(0.0)
        .min_lod(0.0)
        .max_lod(cubemap_description.mip_levels as _);
    let cubemap_sampler = Sampler::new(context.device.clone(), sampler_info)?;

    let device = context.device.clone();
    let allocator = context.allocator.clone();

    let rendergraph = rendergraph(device.clone(), allocator.clone(), &hdr_description)?;

    let descriptor_set_layout = Arc::new(descriptor_set_layout(device.clone())?);
    let descriptor_pool = descriptor_pool(device.clone())?;
    let descriptor_set =
        descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];
    update_descriptor_set(
        &device.handle,
        descriptor_set,
        hdr_texture.view.handle,
        hdr_sampler.handle,
    );

    transition_cubemap_to_transfer_dst(
        context.graphics_queue(),
        command_pool,
        cubemap.image.handle,
        cubemap_description.mip_levels,
    )?;

    let cube = Arc::new(Cube::new(context, command_pool)?);
    let projection = glm::perspective_zo(1.0, 90_f32.to_radians(), 0.1_f32, 10_f32);
    let matrices = cubemap_matrices();
    let offscreen_renderpass = rendergraph.pass_handle("offscreen")?;
    let (pipeline, pipeline_layout) = pipeline(
        device.clone(),
        shader_cache,
        descriptor_set_layout.clone(),
        offscreen_renderpass,
    )?;

    for mip_level in 0..cubemap_description.mip_levels {
        for (face_index, matrix) in matrices.iter().enumerate() {
            let dimension = cubemap_description.width as f32 * 0.5_f32.powf(mip_level as _);
            let extent = vk::Extent2D::builder()
                .width(dimension as _)
                .height(dimension as _)
                .build();

            let device_ptr = device.clone();
            let pipeline_layout_handle = pipeline_layout.handle.clone();
            let push_constants_hdr = PushConstantHdr {
                mvp: projection * matrix,
            };

            let pipeline_handle = pipeline.handle.clone();
            let cube_ptr = cube.clone();

            command_pool.execute_once(context.graphics_queue(), |command_buffer| {
                rendergraph.execute_pass(command_buffer, "offscreen", 0, |_, command_buffer| {
                    device.update_viewport(command_buffer, extent, true)?;
                    unsafe {
                        device_ptr.handle.cmd_push_constants(
                            command_buffer,
                            pipeline_layout_handle,
                            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                            0,
                            byte_slice_from(&push_constants_hdr),
                        );
                        device_ptr.handle.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            pipeline_handle,
                        );
                        device_ptr.handle.cmd_bind_descriptor_sets(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            pipeline_layout_handle,
                            0,
                            &[descriptor_set],
                            &[],
                        );
                    }
                    cube_ptr.draw(&device_ptr.handle, command_buffer)?;
                    Ok(())
                })?;
                Ok(())
            })?;

            let color_image = rendergraph.image("color")?.handle();
            transition_backbuffer_to_transfer_src(
                context.graphics_queue(),
                command_pool,
                color_image,
            )?;

            let src_subresource = vk::ImageSubresourceLayers::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_array_layer(0)
                .mip_level(0)
                .layer_count(1)
                .build();

            let dst_subresource = vk::ImageSubresourceLayers::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_array_layer(face_index as _)
                .mip_level(mip_level)
                .layer_count(1)
                .build();

            let extent = vk::Extent3D::builder()
                .width(dimension as _)
                .height(dimension as _)
                .depth(1)
                .build();

            let region = vk::ImageCopy::builder()
                .src_subresource(src_subresource)
                .dst_subresource(dst_subresource)
                .extent(extent)
                .build();

            let copy_info = ImageToImageCopyBuilder::default()
                .graphics_queue(context.graphics_queue())
                .source(color_image)
                .destination(cubemap.image.handle)
                .source_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .destination_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .regions(vec![region])
                .build()
                .map_err(|error| anyhow!("{}", error))?;
            command_pool.copy_image_to_image(&copy_info)?;

            transition_backbuffer_to_color_attachment(
                context.graphics_queue(),
                command_pool,
                color_image,
            )?;
        }
    }

    transition_cubemap_to_shader_read(
        context.graphics_queue(),
        command_pool,
        cubemap.image.handle,
        cubemap_description.mip_levels,
    )?;

    Ok((cubemap, cubemap_sampler))
}

fn rendergraph(
    device: Arc<Device>,
    allocator: Arc<Allocator>,
    hdr_description: &ImageDescription,
) -> Result<RenderGraph> {
    let offscreen = "offscreen";
    let color = "color";
    let mut rendergraph = RenderGraph::new(
        &[offscreen],
        vec![ImageNode {
            name: color.to_string(),
            extent: vk::Extent2D::builder()
                .width(hdr_description.width)
                .height(hdr_description.width) // Width instead of height to make it a square
                .build(),
            format: hdr_description.format,
            clear_value: vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [1.0, 1.0, 1.0, 1.0],
                },
            },
            samples: vk::SampleCountFlags::TYPE_1,
        }],
        &[(offscreen, color)],
    )?;
    rendergraph.build(device, allocator)?;
    Ok(rendergraph)
}

fn descriptor_set_layout(device: Arc<Device>) -> Result<DescriptorSetLayout> {
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

    let create_info = vk::DescriptorPoolCreateInfo::builder()
        .pool_sizes(&pool_sizes)
        .max_sets(1);

    DescriptorPool::new(device, create_info)
}

pub fn update_descriptor_set(
    device: &ash::Device,
    descriptor_set: vk::DescriptorSet,
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
        .dst_set(descriptor_set)
        .dst_binding(0)
        .dst_array_element(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(&image_infos)
        .build();

    let descriptor_writes = vec![sampler_descriptor_write];

    unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) }
}

fn shader_paths() -> Result<ShaderPathSet> {
    let shader_path_set = ShaderPathSetBuilder::default()
        .vertex("assets/shaders/environment/filtercube.vert.spv")
        .fragment("assets/shaders/environment/equirectangular_to_cubemap.frag.spv")
        .build()
        .map_err(|error| anyhow!("{}", error))?;
    Ok(shader_path_set)
}

fn cubemap_matrices() -> [glm::Mat4; 6] {
    let origin = glm::vec3(0.0, 0.0, 0.0);
    let up = glm::vec3(0.0, 1.0, 0.0);
    let down = glm::vec3(0.0, -1.0, 0.0);
    let left = glm::vec3(-1.0, 0.0, 0.0);
    let right = glm::vec3(1.0, 0.0, 0.0);
    let forward = glm::vec3(0.0, 0.0, 1.0);
    let backward = glm::vec3(0.0, 0.0, -1.0);
    [
        glm::look_at(&origin, &right, &down),
        glm::look_at(&origin, &left, &down),
        glm::look_at(&origin, &down, &backward),
        glm::look_at(&origin, &up, &forward),
        glm::look_at(&origin, &forward, &down),
        glm::look_at(&origin, &backward, &down),
    ]
}

fn pipeline(
    device: Arc<Device>,
    shader_cache: &mut ShaderCache,
    descriptor_set_layout: Arc<DescriptorSetLayout>,
    render_pass: Arc<RenderPass>,
) -> Result<(GraphicsPipeline, PipelineLayout)> {
    let push_constant_range = vk::PushConstantRange::builder()
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        .size(mem::size_of::<PushConstantHdr>() as u32)
        .build();
    let shader_paths = shader_paths()?;
    let shader_set = shader_cache.create_shader_set(device.clone(), &shader_paths)?;
    let mut settings = GraphicsPipelineSettingsBuilder::default();
    settings
        .render_pass(render_pass)
        .vertex_inputs(Cube::vertex_inputs())
        .vertex_attributes(Cube::vertex_attributes())
        .descriptor_set_layout(descriptor_set_layout)
        .shader_set(shader_set)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1)
        .push_constant_range(push_constant_range);
    settings
        .build()
        .map_err(|error| anyhow!("{}", error))?
        .create_pipeline(device)
}

fn transition_cubemap_to_transfer_dst(
    graphics_queue: vk::Queue,
    command_pool: &CommandPool,
    cubemap_image: vk::Image,
    mip_levels: u32,
) -> Result<()> {
    let transition = ImageLayoutTransitionBuilder::default()
        .graphics_queue(graphics_queue)
        .base_mip_level(0)
        .level_count(mip_levels)
        .layer_count(6)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
        .src_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
        .dst_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
        .build()
        .map_err(|error| anyhow!("{}", error))?;
    transition_image(cubemap_image, command_pool, &transition)?;
    Ok(())
}

fn transition_backbuffer_to_transfer_src(
    graphics_queue: vk::Queue,
    command_pool: &CommandPool,
    color_image: vk::Image,
) -> Result<()> {
    let transition = ImageLayoutTransitionBuilder::default()
        .graphics_queue(graphics_queue)
        .base_mip_level(0)
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
        .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
        .src_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
        .dst_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
        .build()
        .map_err(|error| anyhow!("{}", error))?;
    transition_image(color_image, command_pool, &transition)?;
    Ok(())
}

fn transition_backbuffer_to_color_attachment(
    graphics_queue: vk::Queue,
    command_pool: &CommandPool,
    color_image: vk::Image,
) -> Result<()> {
    let transition = ImageLayoutTransitionBuilder::default()
        .graphics_queue(graphics_queue)
        .base_mip_level(0)
        .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
        .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .src_access_mask(vk::AccessFlags::TRANSFER_READ)
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
        .src_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
        .dst_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
        .build()
        .map_err(|error| anyhow!("{}", error))?;
    transition_image(color_image, command_pool, &transition)?;
    Ok(())
}

fn transition_cubemap_to_shader_read(
    graphics_queue: vk::Queue,
    command_pool: &CommandPool,
    cubemap_image: vk::Image,
    mip_levels: u32,
) -> Result<()> {
    let transition = ImageLayoutTransitionBuilder::default()
        .graphics_queue(graphics_queue)
        .base_mip_level(0)
        .level_count(mip_levels)
        .layer_count(6)
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
        .dst_access_mask(vk::AccessFlags::HOST_WRITE | vk::AccessFlags::TRANSFER_WRITE)
        .src_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
        .dst_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
        .build()
        .map_err(|error| anyhow!("{}", error))?;
    transition_image(cubemap_image, command_pool, &transition)?;
    Ok(())
}
