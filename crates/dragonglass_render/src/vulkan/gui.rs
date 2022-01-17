use anyhow::Result;
use dragonglass_gui::egui::{self, ClippedMesh, CtxRef, Pos2, TextureId};
use dragonglass_vulkan::{
    ash::{
        self,
        vk::{self, Handle},
    },
    byte_slice_from,
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
    pub font_texture: Texture,
    pub font_texture_sampler: Sampler,
    pub pipeline: Option<Pipeline>,
    pub pipeline_layout: Option<PipelineLayout>,
    pub geometry_buffer: GeometryBuffer,
    device: Arc<Device>,
}

impl GuiRender {
    pub fn new(
        context: &Context,
        gui_context: &CtxRef,
        shader_cache: &mut ShaderCache,
        render_pass: Arc<RenderPass>,
        command_pool: &CommandPool,
    ) -> Result<Self> {
        debug!("Creating gui renderer");

        let device = context.device.clone();
        let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
        let descriptor_pool = Self::create_descriptor_pool(device.clone())?;
        let descriptor_set =
            descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];

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

        let font_texture_sampler = Sampler::default(context.device.clone())?;
        Self::update_descriptor_set(
            &device.handle,
            descriptor_set,
            &font_texture,
            &font_texture_sampler,
        );

        let vertex_buffer_size = 1024 * 1024 * 4;
        let index_buffer_size = 1024 * 1024 * 4;

        let geometry_buffer = GeometryBuffer::new(
            device.clone(),
            context.allocator.clone(),
            vertex_buffer_size,
            Some(index_buffer_size),
        )?;

        let mut gui_renderer = Self {
            descriptor_set,
            descriptor_set_layout,
            descriptor_pool,
            font_texture,
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

        let shader_set = shader_cache
            .create_shader_set(self.device.clone(), &shader_paths)
            .unwrap();

        let mut settings = GraphicsPipelineSettingsBuilder::default();
        settings
            .render_pass(render_pass)
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

    pub fn issue_commands(
        &self,
        command_buffer: vk::CommandBuffer,
        clipped_meshes: &[ClippedMesh],
    ) -> Result<()> {
        let viewport = Viewport::default();
        let scale_factor = 1.0;

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
                    screen_size: glm::vec2(0.0, 0.0),
                }),
            );
        }

        let mut vertex_base = 0;
        let mut index_base = 0;
        for ClippedMesh(rect, mesh) in clipped_meshes {
            if let TextureId::User(id) = mesh.texture_id {
                // TODO: Update descriptor set with user texture
            } else {
                // TODO: Update font texture
            }

            if mesh.vertices.is_empty() || mesh.indices.is_empty() {
                continue;
            }

            let v_slice = &mesh.vertices;
            let v_size = std::mem::size_of_val(&v_slice[0]);
            let v_copy_size = v_slice.len() * v_size;

            let i_slice = &mesh.indices;
            let i_size = std::mem::size_of_val(&i_slice[0]);
            let i_copy_size = i_slice.len() * i_size;

            unsafe {
                let min = rect.min;
                let min = egui::Pos2 {
                    x: min.x * scale_factor as f32,
                    y: min.y * scale_factor as f32,
                };
                let min = egui::Pos2 {
                    x: f32::clamp(min.x, 0.0, viewport.width),
                    y: f32::clamp(min.y, 0.0, viewport.height),
                };
                let max = rect.max;
                let max = egui::Pos2 {
                    x: max.x * scale_factor,
                    y: max.y * scale_factor,
                };
                let max = egui::Pos2 {
                    x: f32::clamp(max.x, min.x, viewport.width),
                    y: f32::clamp(max.y, min.y, viewport.height),
                };
                device.handle.cmd_set_scissor(
                    command_buffer,
                    0,
                    &[vk::Rect2D::builder()
                        .offset(
                            vk::Offset2D::builder()
                                .x(min.x.round() as i32)
                                .y(min.y.round() as i32)
                                .build(),
                        )
                        .extent(
                            vk::Extent2D::builder()
                                .width((max.x.round() - min.x) as u32)
                                .height((max.y.round() - min.y) as u32)
                                .build(),
                        )
                        .build()],
                );
                self.device.handle.cmd_draw_indexed(
                    command_buffer,
                    mesh.indices.len() as u32,
                    1,
                    index_base,
                    vertex_base,
                    0,
                );
            }

            vertex_base += mesh.vertices.len() as i32;
            index_base += mesh.indices.len() as u32;
        }
        Ok(())
    }
}
