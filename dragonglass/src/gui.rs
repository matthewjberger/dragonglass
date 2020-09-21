use super::{
    core::{Context, LogicalDevice},
    render::{
        CommandPool, DescriptorPool, DescriptorSetLayout, GeometryBuffer, GraphicsPipeline,
        GraphicsPipelineSettings, GraphicsPipelineSettingsBuilder, Image, ImageDescription,
        ImageDescriptionBuilder, ImageView, PipelineLayout, RenderPass, Sampler, ShaderCache,
        ShaderPathSet, ShaderPathSetBuilder,
    },
};
use crate::device::byte_slice_from;
use anyhow::{anyhow, Result};
use ash::{version::DeviceV1_0, vk};
use imgui::{Context as ImguiContext, DrawCmd, DrawCmdParams, DrawData};
use nalgebra_glm as glm;
use std::{mem, sync::Arc};

pub struct PushConstantBlockGui {
    pub projection: glm::Mat4,
}

pub struct GuiRenderer {
    pub pipeline: Option<GraphicsPipeline>,
    pub pipeline_layout: Option<PipelineLayout>,
    pub geometry_buffer: Option<GeometryBuffer>,
    pub descriptor_pool: DescriptorPool,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub descriptor_set: vk::DescriptorSet,
    pub font_texture: Texture,
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
        let device = context.logical_device.clone();
        let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
        let descriptor_pool = Self::descriptor_pool(device.clone())?;
        let descriptor_set =
            descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];

        let mut rendering = Self {
            pipeline: None,
            pipeline_layout: None,
            geometry_buffer: None,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_set,
            font_texture: Self::font_texture(context, imgui, pool)?,
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

    fn geometry_buffer(
        context: &Context,
        pool: &CommandPool,
        draw_data: &DrawData,
    ) -> Result<GeometryBuffer> {
        let vertices = draw_data
            .draw_lists()
            .flat_map(|draw_list| draw_list.vtx_buffer())
            .map(|vertex| *vertex)
            .collect::<Vec<_>>();

        let indices = draw_data
            .draw_lists()
            .flat_map(|draw_list| draw_list.idx_buffer())
            .map(|index| *index as u32)
            .collect::<Vec<_>>();

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

        Ok(geometry_buffer)
    }

    fn shader_paths() -> Result<ShaderPathSet> {
        let shader_path_set = ShaderPathSetBuilder::default()
            .vertex("dragonglass/shaders/gui/gui.vert.spv")
            .fragment("dragonglass/shaders/gui/gui.frag.spv")
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
        let push_constant_range = vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .size(mem::size_of::<PushConstantBlockGui>() as u32)
            .build();
        let settings = GraphicsPipelineSettingsBuilder::default()
            .shader_set(shader_set)
            .render_pass(render_pass)
            .vertex_state_info(vertex_state_info.build())
            .descriptor_set_layout(descriptor_set_layout)
            .push_constant_range(push_constant_range)
            .blended(true)
            .depth_test_enabled(false)
            .depth_write_enabled(false)
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        Ok(settings)
    }

    fn font_texture(
        context: &Context,
        imgui: &mut ImguiContext,
        pool: &CommandPool,
    ) -> Result<Texture> {
        let mut fonts = imgui.fonts();
        let atlas = fonts.build_rgba32_texture();
        let description = ImageDescriptionBuilder::default()
            .format(vk::Format::R8G8B8A8_UNORM)
            .width(atlas.width)
            .height(atlas.height)
            .mip_levels(1)
            .pixels(atlas.data.to_vec())
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        Texture::new(context.clone(), pool, &description)
    }

    fn vertex_attributes() -> [vk::VertexInputAttributeDescription; 3] {
        let float_size = std::mem::size_of::<f32>();
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
            .offset((2 * float_size) as _)
            .build();

        let color_description = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(2)
            .format(vk::Format::R8G8B8A8_UNORM)
            .offset((4 * float_size) as _)
            .build();

        [
            position_description,
            tex_coord_description,
            color_description,
        ]
    }

    fn vertex_input_descriptions() -> [vk::VertexInputBindingDescription; 1] {
        let vertex_input_binding_description = vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride((5 * std::mem::size_of::<f32>()) as _)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build();
        [vertex_input_binding_description]
    }

    fn descriptor_pool(device: Arc<LogicalDevice>) -> Result<DescriptorPool> {
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

    fn descriptor_set_layout(device: Arc<LogicalDevice>) -> Result<DescriptorSetLayout> {
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

    fn update_descriptor_set(&mut self) {
        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(self.font_texture.view.handle)
            .sampler(self.font_texture.sampler.handle);
        let image_info_list = [image_info.build()];

        let sampler_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_info_list);

        let writes = [sampler_write.build()];
        unsafe { self.device.handle.update_descriptor_sets(&writes, &[]) }
    }

    pub fn issue_commands(
        &mut self,
        context: &Context,
        command_buffer: vk::CommandBuffer,
        pool: &CommandPool,
        draw_data: &DrawData,
    ) -> Result<()> {
        if draw_data.total_vtx_count == 0 {
            return Ok(());
        }

        // if self.geometry_buffer.is_none() {
        self.geometry_buffer = None;
        let resized_buffer = Self::geometry_buffer(context, pool, draw_data)?;
        self.geometry_buffer = Some(resized_buffer);
        // }

        // // FIXME: resize vertex and index buffers separately and append vertices
        // if draw_data.total_vtx_count as u32
        //     > self.geometry_buffer.as_ref().unwrap().number_of_vertices
        // {
        //     trace!("Resizing gui vertex buffer");
        //     self.geometry_buffer = None;
        //     let resized_buffer = Self::resize_geometry_buffer(command_pool, draw_data);
        //     self.geometry_buffer = Some(resized_buffer);
        // } else if draw_data.total_idx_count as u32
        //     > self.geometry_buffer.as_ref().unwrap().number_of_indices
        // {
        //     trace!("Resizing gui index buffer");
        //     self.geometry_buffer = None;
        //     let resized_buffer = Self::resize_geometry_buffer(command_pool, draw_data);
        //     self.geometry_buffer = Some(resized_buffer);
        // }

        self.pipeline
            .as_ref()
            .ok_or_else(|| anyhow!("Failed to get scene pipeline!"))?
            .bind(&self.device.handle, command_buffer);

        let pipeline_layout = self
            .pipeline_layout
            .as_ref()
            .ok_or_else(|| anyhow!("Failed to get scene pipeline layout!"))?
            .handle;

        let geometry_buffer = self
            .geometry_buffer
            .as_mut()
            .ok_or_else(|| anyhow!("Failed to get geometry_buffer!"))?;

        let framebuffer_width = draw_data.framebuffer_scale[0] * draw_data.display_size[0];
        let framebuffer_height = draw_data.framebuffer_scale[1] * draw_data.display_size[1];

        let projection = glm::ortho_zo(0.0, framebuffer_width, 0.0, framebuffer_height, -1.0, 1.0);

        let viewport = vk::Viewport {
            width: framebuffer_width,
            height: framebuffer_height,
            max_depth: 1.0,
            ..Default::default()
        };
        let viewports = [viewport];

        unsafe {
            self.device.handle.cmd_push_constants(
                command_buffer,
                pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                byte_slice_from(&PushConstantBlockGui { projection }),
            );
        }

        unsafe {
            self.device
                .handle
                .cmd_set_viewport(command_buffer, 0, &viewports);
        }

        geometry_buffer.bind(&self.device.handle, command_buffer);

        // Render draw lists
        // Adapted from: https://github.com/adrien-ben/imgui-rs-vulkan-renderer
        let mut index_offset = 0;
        let mut vertex_offset = 0;
        let clip_offset = draw_data.display_pos;
        let clip_scale = draw_data.framebuffer_scale;
        for draw_list in draw_data.draw_lists() {
            for command in draw_list.commands() {
                match command {
                    DrawCmd::Elements {
                        count,
                        cmd_params:
                            DrawCmdParams {
                                clip_rect,
                                texture_id: _texture_id,
                                vtx_offset,
                                idx_offset,
                            },
                    } => {
                        unsafe {
                            let clip_x = (clip_rect[0] - clip_offset[0]) * clip_scale[0];
                            let clip_y = (clip_rect[1] - clip_offset[1]) * clip_scale[1];
                            let clip_w = (clip_rect[2] - clip_offset[0]) * clip_scale[0] - clip_x;
                            let clip_h = (clip_rect[3] - clip_offset[1]) * clip_scale[1] - clip_y;
                            let scissors = [vk::Rect2D {
                                offset: vk::Offset2D {
                                    x: clip_x as _,
                                    y: clip_y as _,
                                },
                                extent: vk::Extent2D {
                                    width: clip_w as _,
                                    height: clip_h as _,
                                },
                            }];
                            self.device
                                .handle
                                .cmd_set_scissor(command_buffer, 0, &scissors);
                        }

                        // TODO: Create a map of texture ids to descriptor sets
                        unsafe {
                            self.device.handle.cmd_bind_descriptor_sets(
                                command_buffer,
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline_layout,
                                0,
                                &[self.descriptor_set],
                                &[],
                            )
                        };

                        unsafe {
                            self.device.handle.cmd_draw_indexed(
                                command_buffer,
                                count as _,
                                1,
                                index_offset + idx_offset as u32,
                                vertex_offset + vtx_offset as i32,
                                0,
                            )
                        };
                    }
                    _ => (),
                }
            }
            index_offset += draw_list.idx_buffer().len() as u32;
            vertex_offset += draw_list.vtx_buffer().len() as i32;
        }

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
