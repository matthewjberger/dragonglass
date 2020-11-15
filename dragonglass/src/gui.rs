use crate::{
    adapters::{
        CommandPool, DescriptorPool, DescriptorSetLayout, GraphicsPipeline,
        GraphicsPipelineSettingsBuilder, ImageToImageCopyBuilder, PipelineLayout, RenderPass,
    },
    context::{Context, Device},
    cube::Cube,
    rendergraph::{ImageNode, RenderGraph},
    resources::{
        transition_image, Cubemap, GeometryBuffer, ImageDescription, ImageLayoutTransitionBuilder,
        Sampler, ShaderCache, ShaderPathSet, ShaderPathSetBuilder, Texture,
    },
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use imgui::{Context as ImguiContext, DrawCmd, DrawCmdParams, DrawData};
use log::{debug, warn};
use nalgebra_glm as glm;
use std::{mem, slice, sync::Arc};

pub unsafe fn byte_slice_from<T: Sized>(data: &T) -> &[u8] {
    let data_ptr = (data as *const T) as *const u8;
    slice::from_raw_parts(data_ptr, mem::size_of::<T>())
}

pub struct PushConstantBlockGui {
    pub projection: glm::Mat4,
}

pub struct GuiRendering {
    pub descriptor_set: vk::DescriptorSet,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub descriptor_pool: DescriptorPool,
    pub font_texture: Texture,
    pub font_sampler: Sampler,
    pub pipeline: Option<GraphicsPipeline>,
    pub pipeline_layout: Option<PipelineLayout>,
    pub geometry_buffer: Option<GeometryBuffer>,
    pub device: Arc<Device>,
}

impl GuiRendering {
    pub fn new(
        context: &Context,
        imgui: &mut ImguiContext,
        command_pool: &CommandPool,
    ) -> Result<Self> {
        debug!("Creating gui renderer");
        let device = context.device.clone();
        let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
        let descriptor_pool = Self::descriptor_pool(device.clone())?;
        let descriptor_set =
            descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];

        let font_texture = {
            let mut fonts = imgui.fonts();
            let atlas_texture = fonts.build_rgba32_texture();
            let atlas_texture_description = ImageDescription {
                format: vk::Format::R8G8B8A8_UNORM,
                width: atlas_texture.width,
                height: atlas_texture.height,
                mip_levels: 1,
                pixels: atlas_texture.data.to_vec(),
            };
            Texture::new(context, command_pool, &atlas_texture_description)
        }?;
        let font_sampler = Sampler::default(device.clone())?;

        Self::update_descriptor_set(device.clone(), descriptor_set, &font_texture, &font_sampler);

        Ok(Self {
            descriptor_set,
            descriptor_set_layout,
            descriptor_pool,
            font_texture,
            font_sampler,
            pipeline: None,
            pipeline_layout: None,
            geometry_buffer: None,
            device,
        })
    }

    fn update_descriptor_set(
        device: Arc<Device>,
        descriptor_set: vk::DescriptorSet,
        texture: &Texture,
        sampler: &Sampler,
    ) {
        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(texture.view.handle)
            .sampler(sampler.handle)
            .build();
        let image_infos = [image_info];

        let sampler_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_infos)
            .build();

        let descriptor_writes = [sampler_descriptor_write];

        unsafe {
            device
                .handle
                .update_descriptor_sets(&descriptor_writes, &[])
        }
    }

    pub fn create_pipeline(
        &mut self,
        shader_cache: &mut ShaderCache,
        render_pass: Arc<RenderPass>,
    ) -> Result<()> {
        debug!("Creating gui pipeline");
        let push_constant_range = vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .size(mem::size_of::<PushConstantBlockGui>() as u32)
            .build();

        let shader_paths = ShaderPathSetBuilder::default()
            .vertex("assets/shaders/environment/gui.vert.spv")
            .fragment("assets/shaders/environment/gui.frag.spv")
            .build()
            .map_err(|error| anyhow!("{}", error))?;

        let shader_set = shader_cache.create_shader_set(self.device.clone(), &shader_paths)?;

        let mut settings = GraphicsPipelineSettingsBuilder::default();
        settings
            .render_pass(render_pass.clone())
            .vertex_inputs(Self::vertex_input_descriptions())
            .vertex_attributes(Self::vertex_attributes())
            .descriptor_set_layout(self.descriptor_set_layout.clone())
            .shader_set(shader_set)
            .depth_test_enabled(false)
            .depth_write_enabled(false)
            .front_face(vk::FrontFace::CLOCKWISE)
            .blended(true)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1)
            .push_constant_range(push_constant_range);
        let (pipeline, pipeline_layout) = settings
            .build()
            .map_err(|error| anyhow!("{}", error))?
            .create_pipeline(self.device.clone())?;

        self.pipeline = Some(pipeline);
        self.pipeline_layout = Some(pipeline_layout);
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

        let create_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&pool_sizes)
            .max_sets(1);

        DescriptorPool::new(device, create_info)
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
            .stride(20)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build();
        [vertex_input_binding_description]
    }

    fn resize_geometry_buffer(
        context: &Context,
        command_pool: &CommandPool,
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

        geometry_buffer.vertex_buffer.upload_data(
            &vertices,
            0,
            command_pool,
            context.graphics_queue(),
        )?;

        geometry_buffer
            .index_buffer
            .as_ref()
            .context("Failed to access gui index buffer!")?
            .upload_data(&indices, 0, command_pool, context.graphics_queue())?;

        Ok(geometry_buffer)
    }

    pub fn issue_commands(
        &self,
        command_pool: &CommandPool,
        command_buffer: vk::CommandBuffer,
        draw_data: &DrawData,
        context: &Context,
    ) -> Result<()> {
        if draw_data.total_vtx_count == 0 {
            return Ok(());
        }

        // if self.geometry_buffer.is_none() {
        self.geometry_buffer = None;
        let resized_buffer = Self::resize_geometry_buffer(context, command_pool, draw_data)?;
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

        if let Some(geometry_buffer) = self.geometry_buffer.as_mut() {
            if let Some(pipeline) = self.pipeline.as_ref() {
                if let Some(pipeline_layout) = self.pipeline_layout.as_ref() {
                    pipeline.bind(&self.device.handle, command_buffer);

                    let framebuffer_width =
                        draw_data.framebuffer_scale[0] * draw_data.display_size[0];
                    let framebuffer_height =
                        draw_data.framebuffer_scale[1] * draw_data.display_size[1];

                    let projection =
                        glm::ortho_zo(0.0, framebuffer_width, 0.0, framebuffer_height, -1.0, 1.0);

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
                            pipeline_layout.handle,
                            vk::ShaderStageFlags::VERTEX,
                            0,
                            byte_slice_from(&PushConstantBlockGui { projection }),
                        );
                        self.device
                            .handle
                            .cmd_set_viewport(command_buffer, 0, &viewports);
                    }

                    geometry_buffer.bind(&self.device.handle, command_buffer)?;

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
                                    let clip_x = (clip_rect[0] - clip_offset[0]) * clip_scale[0];
                                    let clip_y = (clip_rect[1] - clip_offset[1]) * clip_scale[1];
                                    let clip_w =
                                        (clip_rect[2] - clip_offset[0]) * clip_scale[0] - clip_x;
                                    let clip_h =
                                        (clip_rect[3] - clip_offset[1]) * clip_scale[1] - clip_y;
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

                                    unsafe {
                                        self.device.handle.cmd_set_scissor(
                                            command_buffer,
                                            0,
                                            &scissors,
                                        );
                                        // TODO: Create a map of texture ids to descriptor sets
                                        self.device.handle.cmd_bind_descriptor_sets(
                                            command_buffer,
                                            vk::PipelineBindPoint::GRAPHICS,
                                            pipeline_layout.handle,
                                            0,
                                            &[self.descriptor_set],
                                            &[],
                                        );
                                        self.device.handle.cmd_draw_indexed(
                                            command_buffer,
                                            count as _,
                                            1,
                                            index_offset + idx_offset as u32,
                                            vertex_offset + vtx_offset as i32,
                                            0,
                                        );
                                    }
                                }
                                _ => (),
                            }
                        }
                        index_offset += draw_list.idx_buffer().len() as u32;
                        vertex_offset += draw_list.vtx_buffer().len() as i32;
                    }
                }
            } else {
                warn!("No gui pipeline available");
                return Ok(());
            }
        }
        Ok(())
    }
}
