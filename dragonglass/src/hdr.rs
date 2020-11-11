use crate::{
    adapters::{CommandPool, DescriptorPool, DescriptorSetLayout, GraphicsPipelineSettingsBuilder},
    context::{Context, Device},
    cube::Cube,
    rendergraph::{ImageNode, RenderGraph},
    resources::{
        Cubemap, ImageDescription, ImageLayoutTransitionBuilder, Sampler, ShaderCache,
        ShaderPathSet, ShaderPathSetBuilder, Texture,
    },
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use nalgebra_glm as glm;
use std::{mem, slice, sync::Arc};
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
    path: &str,
    shader_cache: &mut ShaderCache,
) -> Result<Cubemap> {
    let hdr_description = ImageDescription::from_hdr(path)?;
    let hdr_texture = Texture::new(context, command_pool, &hdr_description)?;
    let hdr_sampler = Sampler::default(context.device.clone())?;

    let cubemap_description = ImageDescription::empty(
        hdr_description.width,
        hdr_description.width,
        vk::Format::R32G32B32A32_SFLOAT,
    );
    let cubemap = Cubemap::new(context, command_pool, &cubemap_description)?;

    let device = context.device.clone();
    let allocator = context.allocator.clone();

    let mut rendergraph = rendergraph(device.clone(), allocator.clone(), &hdr_description)?;

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

    let push_constant_range = vk::PushConstantRange::builder()
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        .size(mem::size_of::<PushConstantHdr>() as u32)
        .build();
    let shader_paths = shader_paths()?;
    let shader_set = shader_cache.create_shader_set(device.clone(), &shader_paths)?;
    let offscreen_renderpass = rendergraph
        .passes
        .get("offscreen")
        .context("Failed to get offscreen pass to create scene")?
        .render_pass
        .clone();
    let mut settings = GraphicsPipelineSettingsBuilder::default();
    settings
        .render_pass(offscreen_renderpass)
        .vertex_inputs(Cube::vertex_inputs())
        .vertex_attributes(Cube::vertex_attributes())
        .descriptor_set_layout(descriptor_set_layout)
        .shader_set(shader_set)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1)
        .push_constant_range(push_constant_range);
    let (pipeline, pipeline_layout) = settings
        .build()
        .map_err(|error| anyhow!("{}", error))?
        .create_pipeline(device.clone())?;

    let projection = glm::perspective_zo(1.0, 90_f32.to_radians(), 0.1_f32, 10_f32);
    let matrices = cubemap_matrices();

    // Transition output cubemap image to be a transfer destination
    let transition = ImageLayoutTransitionBuilder::default()
        .graphics_queue(context.graphics_queue())
        .base_mip_level(cubemap_description.mip_levels)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
        .src_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
        .dst_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
        .build()
        .map_err(|error| anyhow!("{}", error))?;
    cubemap.image.transition(command_pool, &transition)?;

    // Create a unit cube
    let cube = Arc::new(Cube::new(context, command_pool)?);

    let dimension = hdr_description.width;
    let mut viewport = vk::Viewport::builder()
        .width(dimension as _)
        .height(dimension as _)
        .build();
    let scissor = vk::Rect2D::builder()
        .extent(hdr_description.as_extent2D())
        .build();
    let scissors = [scissor];

    /*******************/
    /* Declare initial viewport and scissor */
    for mip_level in 0..cubemap_description.mip_levels {
        for (face_index, matrix) in matrices.iter().enumerate() {
            let dimension = cubemap_description.width as f32 * 0.5_f32.powf(mip_level as _);
            viewport.width = dimension;
            viewport.height = dimension;
            let viewports = [viewport];
            let device_ptr = device.clone();
            let pipeline_layout_handle = pipeline_layout.handle.clone();
            let push_constants_hdr = PushConstantHdr {
                mvp: projection * matrix,
            };
            let pipeline_handle = pipeline.handle.clone();
            let cube_ptr = cube.clone();
            rendergraph
                .passes
                .get_mut("offscreen")
                .context("Failed to get offscreen pass to set scene callback")?
                .set_callback(move |command_buffer| {
                    unsafe {
                        device_ptr
                            .handle
                            .cmd_set_viewport(command_buffer, 0, &viewports);
                        device_ptr
                            .handle
                            .cmd_set_scissor(command_buffer, 0, &scissors);
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
                });

            command_pool.execute_once(context.graphics_queue(), |command_buffer| {
                rendergraph.execute(device.clone(), command_buffer)
            })?;
        }

        // Transition backbuffer image to be a transfer source
        let transition = ImageLayoutTransitionBuilder::default()
            .graphics_queue(context.graphics_queue())
            .base_mip_level(cubemap_description.mip_levels)
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .src_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
            .dst_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        // TODO: get backbuffer and transition
        //rendergraph.images["backbuffer"].transition(command_pool, &transition)?;

        //          Copy backbuffer to layer of output cubemap
        //               regions
        //                 src (backbuffer)
        //                     aspect_mask = color
        //                     base_array_layer = 0
        //                     mip_level = 0
        //                     layer_count = 1
        //                 dst (face of output cubemap)
        //                     aspect_mask = color
        //                     base_array_layer = face_index
        //                     mip_level = current mip level
        //                     layer_count = 1
        //                 extent is (dimension x dimension)
        //               execute copy image to image
        //                   src image = backbuffer image
        //                   dst image = output cubemap image
        //                   src layout = TRANSFER_SRC_OPTIMAL
        //                   dst layout = TRANSFER_DST_OPTIMAL
        //                   regions = regions

        // Transition backbuffer image to be a color attachment again now that transfer is over
        let transition = ImageLayoutTransitionBuilder::default()
            .graphics_queue(context.graphics_queue())
            .base_mip_level(cubemap_description.mip_levels)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .src_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
            .dst_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        // TODO: get backbuffer and transition
        //rendergraph.images["backbuffer"].transition(command_pool, &transition)?;
    }

    // Transition output cubemap image to be read by the fragment shader
    let transition = ImageLayoutTransitionBuilder::default()
        .graphics_queue(context.graphics_queue())
        .base_mip_level(cubemap_description.mip_levels)
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
        .dst_access_mask(vk::AccessFlags::HOST_WRITE | vk::AccessFlags::TRANSFER_WRITE)
        .src_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
        .dst_stage_mask(vk::PipelineStageFlags::ALL_COMMANDS)
        .build()
        .map_err(|error| anyhow!("{}", error))?;
    cubemap.image.transition(command_pool, &transition)?;

    Ok(cubemap)
}

fn rendergraph(
    device: Arc<Device>,
    allocator: Arc<Allocator>,
    hdr_description: &ImageDescription,
) -> Result<RenderGraph> {
    let offscreen = "offscreen";
    let mut rendergraph = RenderGraph::new(
        &[offscreen],
        vec![ImageNode {
            name: RenderGraph::backbuffer_name(0),
            extent: hdr_description.as_extent2D(),
            format: hdr_description.format,
            clear_value: vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [1.0, 1.0, 1.0, 1.0],
                },
            },
            samples: vk::SampleCountFlags::TYPE_1,
        }],
        &[(offscreen, &RenderGraph::backbuffer_name(0))],
    )?;
    rendergraph.build(device, allocator)?;
    rendergraph
        .passes
        .get_mut(offscreen)
        .context("Failed to get offscreen pass to flip viewport on!")?
        .flip_viewport = true;
    // TODO: Make viewport flipping easier to configure
    // TODO: get backbuffer that isn't presenting.
    // TODO: make rendergraph create backbuffer if not specified
    // rendergraph.insert_backbuffer_images(device, swapchain_images)?;

    // TODO: Need to modify rendergraph to support backbuffer that doesn't present
    // Create a render graph
    //          offscreen_pass
    //          /
    //     backbuffer (doesn't present)
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

fn update_descriptor_set(
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
        glm::look_at(&origin, &right, &up),
        glm::look_at(&origin, &left, &up),
        glm::look_at(&origin, &up, &forward),
        glm::look_at(&origin, &down, &left),
        glm::look_at(&origin, &forward, &up),
        glm::look_at(&origin, &backward, &up),
    ]
}
