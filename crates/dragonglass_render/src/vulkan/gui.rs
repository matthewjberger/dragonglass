use crate::byte_slice_from;
use anyhow::Result;
use dragonglass_gui::egui::{ClippedMesh, CtxRef};
use dragonglass_vulkan::{
    ash::{
        self,
        vk::{self, Handle},
    },
    core::{
        CommandPool, Context, DescriptorPool, DescriptorSetLayout, Device, GeometryBuffer,
        GraphicsPipelineSettingsBuilder, ImageDescription, Pipeline, PipelineLayout, RenderPass,
        Sampler, ShaderCache, ShaderPathSetBuilder, Texture,
    },
};
use dragonglass_world::Viewport;
use log::debug;
use nalgebra_glm as glm;
use std::{mem, sync::Arc};

pub struct PushConstantBlockGui {
    pub screen_size: glm::Vec2,
}

pub struct GuiRender {
    pub descriptor_set: vk::DescriptorSet,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub descriptor_pool: DescriptorPool,
    pub font_texture: Option<Texture>,
    pub font_texture_sampler: Sampler,
    pub pipeline: Option<Pipeline>,
    pub pipeline_layout: Option<PipelineLayout>,
    pub geometry_buffer: GeometryBuffer,
    device: Arc<Device>,
}

impl GuiRender {
    pub fn new(
        context: &Context,
        shader_cache: &mut ShaderCache,
        render_pass: Arc<RenderPass>,
    ) -> Result<Self> {
        debug!("Creating gui renderer");

        let device = context.device.clone();
        let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
        let descriptor_pool = Self::create_descriptor_pool(device.clone())?;
        let descriptor_set =
            descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];

        let vertex_buffer_size = 1024 * 1024 * 4;
        let index_buffer_size = 1024 * 1024 * 4;
        let geometry_buffer = GeometryBuffer::new(
            device.clone(),
            context.allocator.clone(),
            vertex_buffer_size,
            Some(index_buffer_size),
        )?;

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
            .max_lod(vk::LOD_CLAMP_NONE);
        let font_texture_sampler = Sampler::new(context.device.clone(), sampler_info)?;

        let mut gui_renderer = Self {
            descriptor_set,
            descriptor_set_layout,
            descriptor_pool,
            font_texture: None,
            font_texture_sampler,
            pipeline: None,
            pipeline_layout: None,
            geometry_buffer,
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

        let shader_set = shader_cache.create_shader_set(self.device.clone(), &shader_paths)?;

        let mut settings = GraphicsPipelineSettingsBuilder::default();
        settings
            .render_pass(render_pass)
            .vertex_inputs(Self::vertex_input_descriptions())
            .vertex_attributes(Self::vertex_attributes())
            .descriptor_set_layout(self.descriptor_set_layout.clone())
            .shader_set(shader_set)
            .push_constant_range(push_constant_range)
            .cull_mode(vk::CullModeFlags::NONE)
            .blended(true)
            .blended_src_color_blend_factor(vk::BlendFactor::ONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .depth_test_enabled(false)
            .depth_write_enabled(false)
            .dynamic_states(vec![vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR])
            .build()
            .expect("Failed to create render pipeline settings");

        let (pipeline, pipeline_layout) = settings.build()?.create_pipeline(self.device.clone())?;
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

    pub fn update(
        &mut self,
        context: &Context,
        gui_context: &CtxRef,
        command_pool: &CommandPool,
        clipped_meshes: &[ClippedMesh],
    ) -> Result<()> {
        self.update_texture(context, gui_context, command_pool)?;
        self.update_buffers(command_pool, clipped_meshes)?;
        Ok(())
    }

    fn update_texture(
        &mut self,
        context: &Context,
        gui_context: &CtxRef,
        command_pool: &CommandPool,
    ) -> Result<()> {
        let font_texture = {
            let font_image = &gui_context.fonts().font_image();
            let data = font_image
                .pixels
                .iter()
                .flat_map(|&r| vec![r, r, r, r])
                .collect::<Vec<_>>();
            let font_texture_description = ImageDescription {
                format: vk::Format::R8G8B8A8_UNORM,
                width: font_image.width as _,
                height: font_image.height as _,
                mip_levels: 1,
                pixels: data,
            };
            Texture::new(context, command_pool, &font_texture_description)?
        };
        if let Ok(debug) = context.debug() {
            debug.name_image("egui font", font_texture.image.handle.as_raw())?;
            debug.name_image_view("egui font view", font_texture.view.handle.as_raw())?;
        }
        Self::update_descriptor_set(
            &context.device.handle,
            self.descriptor_set,
            &font_texture,
            &self.font_texture_sampler,
        );
        self.font_texture = Some(font_texture);
        Ok(())
    }

    fn update_buffers(
        &mut self,
        command_pool: &CommandPool,
        clipped_meshes: &[ClippedMesh],
    ) -> Result<()> {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for ClippedMesh(_clip_rect, mesh) in clipped_meshes.iter() {
            vertices.extend_from_slice(&mesh.vertices);
            indices.extend_from_slice(&mesh.indices);
        }
        self.geometry_buffer
            .vertex_buffer
            .upload_data(&vertices, 0, command_pool)?;
        if let Some(index_buffer) = self.geometry_buffer.index_buffer.as_mut() {
            index_buffer.upload_data(&indices, 0, command_pool)?;
        }
        Ok(())
    }

    pub fn issue_commands(
        &self,
        viewport: Viewport,
        command_buffer: vk::CommandBuffer,
        clipped_meshes: &[ClippedMesh],
    ) -> Result<()> {
        let device = self.device.clone();

        let pipeline = match self.pipeline.as_ref() {
            Some(pipeline) => pipeline,
            None => return Ok(()),
        };

        let pipeline_layout = match self.pipeline_layout.as_ref() {
            Some(pipeline_layout) => pipeline_layout,
            None => return Ok(()),
        };

        pipeline.bind(&device.handle, command_buffer);

        unsafe {
            device.handle.cmd_push_constants(
                command_buffer,
                pipeline_layout.handle,
                vk::ShaderStageFlags::VERTEX,
                0,
                byte_slice_from(&PushConstantBlockGui {
                    screen_size: glm::vec2(viewport.width, viewport.height),
                }),
            );
        }

        let viewport = vk::Viewport {
            x: viewport.x,
            y: viewport.y,
            width: viewport.width,
            height: viewport.height,
            max_depth: 1.0,
            min_depth: 0.0,
        };
        let viewports = [viewport];
        unsafe {
            device
                .handle
                .cmd_set_viewport(command_buffer, 0, &viewports);
        }

        self.geometry_buffer.bind(&device.handle, command_buffer)?;

        let mut index_offset = 0;
        let mut vertex_offset = 0;
        let scale_factor = 1.0;
        for ClippedMesh(clip_rect, mesh) in clipped_meshes.iter() {
            // Transform clip rect to physical pixels.
            let clip_min_x = scale_factor * clip_rect.min.x;
            let clip_min_y = scale_factor * clip_rect.min.y;
            let clip_max_x = scale_factor * clip_rect.max.x;
            let clip_max_y = scale_factor * clip_rect.max.y;

            // Make sure clip rect can fit within an `u32`.
            let clip_min_x = clip_min_x.clamp(0.0, viewport.width as f32);
            let clip_min_y = clip_min_y.clamp(0.0, viewport.height as f32);
            let clip_max_x = clip_max_x.clamp(clip_min_x, viewport.width as f32);
            let clip_max_y = clip_max_y.clamp(clip_min_y, viewport.height as f32);

            let clip_min_x = clip_min_x.round() as u32;
            let clip_min_y = clip_min_y.round() as u32;
            let clip_max_x = clip_max_x.round() as u32;
            let clip_max_y = clip_max_y.round() as u32;

            let width = (clip_max_x - clip_min_x).max(1);
            let height = (clip_max_y - clip_min_y).max(1);

            {
                // Clip scissor rectangle to target size.
                let x = clip_min_x.min(viewport.width as _);
                let y = clip_min_y.min(viewport.height as _);
                let width = width.min(viewport.width as u32 - x);
                let height = height.min(viewport.height as u32 - y);

                // Skip rendering with zero-sized clip areas.
                if width == 0 || height == 0 {
                    continue;
                }

                let scissors = [vk::Rect2D {
                    offset: vk::Offset2D {
                        x: x as i32,
                        y: y as i32,
                    },
                    extent: vk::Extent2D { width, height },
                }];

                unsafe {
                    device.handle.cmd_set_scissor(command_buffer, 0, &scissors);
                }

                unsafe {
                    device.handle.cmd_bind_descriptor_sets(
                        command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        pipeline_layout.handle,
                        0,
                        &[self.descriptor_set],
                        &[],
                    );

                    device.handle.cmd_draw_indexed(
                        command_buffer,
                        mesh.indices.len() as _,
                        1,
                        index_offset,
                        vertex_offset,
                        0,
                    )
                };
            }

            index_offset += mesh.indices.len() as u32;
            vertex_offset += mesh.vertices.len() as i32;
        }

        Ok(())
    }
}
