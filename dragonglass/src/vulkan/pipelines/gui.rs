use crate::{
    renderer::byte_slice_from,
    vulkan::core::{
        CommandPool, Context, DescriptorPool, DescriptorSetLayout, Device, GeometryBuffer,
        GraphicsPipelineSettingsBuilder, ImageDescription, Pipeline, PipelineLayout, RenderPass,
        Sampler, ShaderCache, ShaderPathSetBuilder, Texture,
    },
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use imgui::{Context as ImguiContext, DrawCmd, DrawCmdParams, DrawData, DrawVert};
use log::debug;
use nalgebra_glm as glm;
use std::{mem, sync::Arc};
use vk_mem::Allocator;

pub struct PushConstantBlockGui {
    pub projection: glm::Mat4,
}

pub struct GuiRender {
    pub descriptor_set: vk::DescriptorSet,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub descriptor_pool: DescriptorPool,
    pub font_texture: Texture,
    pub font_texture_sampler: Sampler,
    pub pipeline: Option<Pipeline>,
    pub pipeline_layout: Option<PipelineLayout>,
    pub geometry_buffer: Option<GeometryBuffer>,
    device: Arc<Device>,
}

impl GuiRender {
    pub fn new(
        context: &Context,
        shader_cache: &mut ShaderCache,
        render_pass: Arc<RenderPass>,
        imgui: &mut ImguiContext,
        command_pool: &CommandPool,
    ) -> Result<Self> {
        debug!("Creating gui renderer");

        let device = context.device.clone();
        let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
        let descriptor_pool = Self::create_descriptor_pool(device.clone())?;
        let descriptor_set =
            descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];

        // TODO: Move texture loading out of this class
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
            Texture::new(context, command_pool, &atlas_texture_description)?
        };

        let font_texture_sampler = Sampler::default(context.device.clone())?;
        Self::update_descriptor_set(
            &device.handle,
            descriptor_set,
            &font_texture,
            &font_texture_sampler,
        );

        let mut gui_renderer = Self {
            descriptor_set,
            descriptor_set_layout,
            descriptor_pool,
            font_texture,
            font_texture_sampler,
            pipeline: None,
            pipeline_layout: None,
            geometry_buffer: None,
            device,
        };
        gui_renderer.create_pipeline(shader_cache, render_pass)?;
        Ok(gui_renderer)
    }

    fn update_descriptor_set(
        device: &ash::Device,
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

        unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) }
    }

    pub fn create_pipeline(
        &mut self,
        shader_cache: &mut ShaderCache,
        render_pass: Arc<RenderPass>,
    ) -> Result<()> {
        let push_constant_range = vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .size(mem::size_of::<PushConstantBlockGui>() as u32)
            .build();

        let shader_paths = ShaderPathSetBuilder::default()
            .vertex("assets/shaders/gui/gui.vert.spv")
            .fragment("assets/shaders/gui/gui.frag.spv")
            .build()
            .unwrap();

        let shader_set = shader_cache
            .create_shader_set(self.device.clone(), &shader_paths)
            .unwrap();

        let mut settings = GraphicsPipelineSettingsBuilder::default();
        settings
            .render_pass(render_pass.clone())
            .vertex_inputs(Self::vertex_input_descriptions())
            .vertex_attributes(Self::vertex_attributes())
            .descriptor_set_layout(self.descriptor_set_layout.clone())
            .shader_set(shader_set)
            .push_constant_range(push_constant_range)
            .blended(true)
            .front_face(vk::FrontFace::CLOCKWISE)
            .depth_test_enabled(false)
            .depth_write_enabled(false)
            .dynamic_states(vec![vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR])
            .build()
            .expect("Failed to create render pipeline settings");

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

        let layout_create_info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);

        DescriptorSetLayout::new(device, layout_create_info)
    }

    fn create_descriptor_pool(device: Arc<Device>) -> Result<DescriptorPool> {
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

    pub fn resize_geometry_buffer(
        &mut self,
        allocator: Arc<Allocator>,
        command_pool: &CommandPool,
        draw_data: &DrawData,
    ) -> Result<()> {
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
            allocator,
            (vertices.len() * std::mem::size_of::<DrawVert>()) as _,
            Some((indices.len() * std::mem::size_of::<u32>()) as _),
        )?;

        geometry_buffer
            .vertex_buffer
            .upload_data(&vertices, 0, command_pool)?;

        geometry_buffer
            .index_buffer
            .as_ref()
            .context("Failed to access cube index buffer!")?
            .upload_data(&indices, 0, command_pool)?;

        self.geometry_buffer = Some(geometry_buffer);
        Ok(())
    }

    pub fn issue_commands(
        &self,
        command_buffer: vk::CommandBuffer,
        draw_data: &DrawData,
    ) -> Result<()> {
        if draw_data.total_vtx_count == 0 {
            return Ok(());
        }

        let device = self.device.clone();

        let geometry_buffer = match self.geometry_buffer.as_ref() {
            Some(geometry_buffer) => geometry_buffer,
            None => return Ok(()),
        };

        let pipeline = match self.pipeline.as_ref() {
            Some(pipeline) => pipeline,
            None => return Ok(()),
        };

        let pipeline_layout = match self.pipeline_layout.as_ref() {
            Some(pipeline_layout) => pipeline_layout,
            None => return Ok(()),
        };

        pipeline.bind(&device.handle, command_buffer);

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
            device.handle.cmd_push_constants(
                command_buffer,
                pipeline_layout.handle,
                vk::ShaderStageFlags::VERTEX,
                0,
                byte_slice_from(&PushConstantBlockGui { projection }),
            );
        }

        unsafe {
            device
                .handle
                .cmd_set_viewport(command_buffer, 0, &viewports);
        }

        geometry_buffer.bind(&device.handle, command_buffer)?;

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
                            device.handle.cmd_set_scissor(command_buffer, 0, &scissors);
                        }

                        // TODO: Create a map of texture ids to descriptor sets
                        unsafe {
                            device.handle.cmd_bind_descriptor_sets(
                                command_buffer,
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline_layout.handle,
                                0,
                                &[self.descriptor_set],
                                &[],
                            )
                        };

                        unsafe {
                            device.handle.cmd_draw_indexed(
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
