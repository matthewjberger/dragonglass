use crate::vulkan::world::WorldRender;
use anyhow::Result;
use dragonglass_config::Config;
use dragonglass_gui::egui::{ClippedMesh, CtxRef};
use dragonglass_vulkan::{
    ash::vk::{self, CommandBuffer},
    core::{
        CommandPool, Context, Device, Image, ImageNode, RawImage, RenderGraph, ShaderCache,
        ShaderPathSetBuilder, Swapchain, SwapchainProperties,
    },
    pbr::EnvironmentMapSet,
    render::{FullscreenRender, FullscreenUniformBuffer, SkyboxRender},
};
use dragonglass_world::{Camera, EntityStore, PerspectiveCamera, Viewport, World};
use nalgebra_glm as glm;
use std::sync::Arc;

use super::{
    gui::GuiRender,
    world::{Light, PbrPipelineData, WorldUniformBuffer},
};

pub struct Scene {
    pub environment_maps: EnvironmentMapSet,
    pub world_render: Option<WorldRender>,
    pub skybox_render: SkyboxRender,
    pub gui_render: GuiRender,
    pub fullscreen_pipeline: Option<FullscreenRender>,
    pub rendergraph: RenderGraph,
    pub transient_command_pool: CommandPool,
    pub shader_cache: ShaderCache,
    pub samples: vk::SampleCountFlags,
    context: Arc<Context>,
}

impl Scene {
    pub fn new(
        context: Arc<Context>,
        swapchain: &Swapchain,
        swapchain_properties: &SwapchainProperties,
    ) -> Result<Self> {
        let transient_command_pool = Self::transient_command_pool(
            context.device.clone(),
            context.graphics_queue(),
            context.physical_device.graphics_queue_family_index,
        )?;
        let samples = context.max_usable_samples();
        let rendergraph =
            Self::create_rendergraph(&context, swapchain, swapchain_properties, samples)?;
        let mut shader_cache = ShaderCache::default();

        let default_hdr_texture =
            dragonglass_world::Texture::from_hdr("assets/skyboxes/desert.hdr")?;
        let environment_maps = EnvironmentMapSet::new(
            &context,
            &transient_command_pool,
            &mut shader_cache,
            &default_hdr_texture,
        )?;

        let skybox_render = SkyboxRender::new(
            &context,
            &transient_command_pool,
            &environment_maps.prefilter,
        )?;

        let fullscreen_pass = rendergraph.pass_handle("fullscreen")?;
        let gui_render = GuiRender::new(context.clone(), &mut shader_cache, fullscreen_pass)?;

        let mut scene = Self {
            environment_maps,
            world_render: None,
            skybox_render,
            gui_render,
            fullscreen_pipeline: None,
            rendergraph,
            transient_command_pool,
            shader_cache,
            samples,
            context,
        };
        scene.create_pipelines()?;
        Ok(scene)
    }

    pub fn create_pipelines(&mut self) -> Result<()> {
        let fullscreen_pass = self.rendergraph.pass_handle("fullscreen")?;

        let shader_path_set = ShaderPathSetBuilder::default()
            .vertex("assets/shaders/postprocessing/fullscreen_triangle.vert.spv")
            .fragment("assets/shaders/postprocessing/postprocess.frag.spv")
            .build()?;

        self.fullscreen_pipeline = None;
        let fullscreen_pipeline = FullscreenRender::new(
            &self.context,
            fullscreen_pass.clone(),
            &mut self.shader_cache,
            self.rendergraph.image_view("color_resolve")?.handle,
            self.rendergraph.sampler("default")?.handle,
            shader_path_set,
        )?;
        self.fullscreen_pipeline = Some(fullscreen_pipeline);

        self.gui_render
            .create_pipeline(&mut self.shader_cache, fullscreen_pass)?;

        let offscreen_renderpass = self.rendergraph.pass_handle("offscreen")?;
        self.skybox_render.create_pipeline(
            &mut self.shader_cache,
            offscreen_renderpass.clone(),
            self.samples,
        )?;

        if let Some(world_render) = self.world_render.as_mut() {
            world_render.create_pipeline(
                &mut self.shader_cache,
                offscreen_renderpass,
                self.samples,
            )?;
        }

        Ok(())
    }

    fn transient_command_pool(
        device: Arc<Device>,
        queue: vk::Queue,
        queue_index: u32,
    ) -> Result<CommandPool> {
        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_index)
            .flags(vk::CommandPoolCreateFlags::TRANSIENT);
        let command_pool = CommandPool::new(device, queue, create_info)?;
        Ok(command_pool)
    }

    pub fn create_rendergraph(
        context: &Context,
        swapchain: &Swapchain,
        swapchain_properties: &SwapchainProperties,
        samples: vk::SampleCountFlags,
    ) -> Result<RenderGraph> {
        let device = context.device.clone();
        let allocator = context.allocator.clone();

        let offscreen = "offscreen";
        let fullscreen = "fullscreen";
        let color = "color";
        let color_resolve = "color_resolve";
        let offscreen_extent = vk::Extent2D::builder().width(2048).height(2048).build();
        let mut rendergraph = RenderGraph::new(
            &[offscreen, fullscreen],
            vec![
                ImageNode {
                    name: color.to_string(),
                    extent: offscreen_extent,
                    format: vk::Format::R8G8B8A8_UNORM,
                    clear_value: vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.39, 0.58, 0.92, 1.0],
                        },
                    },
                    samples,
                    force_store: false,
                    force_shader_read: false,
                },
                ImageNode {
                    name: RenderGraph::DEPTH_STENCIL.to_owned(),
                    extent: offscreen_extent,
                    format: vk::Format::D24_UNORM_S8_UINT,
                    clear_value: vk::ClearValue {
                        depth_stencil: vk::ClearDepthStencilValue {
                            depth: 1.0,
                            stencil: 0,
                        },
                    },
                    samples,
                    force_store: false,
                    force_shader_read: false,
                },
                ImageNode {
                    name: color_resolve.to_string(),
                    extent: offscreen_extent,
                    format: vk::Format::R8G8B8A8_UNORM,
                    clear_value: vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [1.0, 1.0, 1.0, 1.0],
                        },
                    },
                    samples: vk::SampleCountFlags::TYPE_1,
                    force_store: false,
                    force_shader_read: false,
                },
                ImageNode {
                    name: RenderGraph::backbuffer_name(0),
                    extent: swapchain_properties.extent,
                    format: swapchain_properties.surface_format.format,
                    clear_value: vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [1.0, 1.0, 1.0, 1.0],
                        },
                    },
                    samples: vk::SampleCountFlags::TYPE_1,
                    force_store: false,
                    force_shader_read: false,
                },
            ],
            &[
                (offscreen, color),
                (offscreen, color_resolve),
                (offscreen, RenderGraph::DEPTH_STENCIL),
                (color_resolve, fullscreen),
                (fullscreen, &RenderGraph::backbuffer_name(0)),
            ],
        )?;

        rendergraph.build(device.clone(), allocator)?;

        rendergraph.print_graph();

        let swapchain_images = swapchain
            .images()?
            .into_iter()
            .map(|handle| Box::new(RawImage(handle)) as Box<dyn Image>)
            .collect::<Vec<_>>();
        rendergraph.insert_backbuffer_images(device, swapchain_images)?;

        Ok(rendergraph)
    }

    pub fn load_world(&mut self, world: &World) -> Result<()> {
        world
            .scene
            .skybox
            .as_ref()
            .and_then(|index| world.hdr_textures.get(*index))
            .and_then(|texture| {
                self.environment_maps = EnvironmentMapSet::new(
                    &self.context,
                    &self.transient_command_pool,
                    &mut self.shader_cache,
                    texture,
                )
                .ok()?;
                self.skybox_render.update_descriptor_set(
                    self.context.device.clone(),
                    &self.environment_maps.prefilter,
                );
                Some(())
            });

        self.world_render = None;
        let offscreen_renderpass = self.rendergraph.pass_handle("offscreen")?;
        let mut rendering = WorldRender::new(
            &self.context,
            &self.transient_command_pool,
            world,
            &self.environment_maps,
        )?;
        rendering.create_pipeline(&mut self.shader_cache, offscreen_renderpass, self.samples)?;
        self.world_render = Some(rendering);

        Ok(())
    }

    pub fn recreate_rendergraph(
        &mut self,
        swapchain: &Swapchain,
        swapchain_properties: &SwapchainProperties,
    ) -> Result<()> {
        let rendergraph = Scene::create_rendergraph(
            &self.context,
            swapchain,
            swapchain_properties,
            self.samples,
        )?;
        self.rendergraph = rendergraph;
        self.create_pipelines()?;
        Ok(())
    }

    pub fn update(
        &mut self,
        world: &World,
        aspect_ratio: f32,
        gui_context: Option<&CtxRef>,
        clipped_meshes: &[ClippedMesh],
        elapsed_milliseconds: u32,
        config: &Config,
    ) -> Result<()> {
        if let Some(gui_context) = gui_context {
            self.gui_render
                .update(gui_context, &self.transient_command_pool, clipped_meshes)?;
        }

        if let Some(fullscreen_pipeline) = self.fullscreen_pipeline.as_mut() {
            let settings = &config.graphics.post_processing;
            let ubo = FullscreenUniformBuffer {
                time: elapsed_milliseconds,
                chromatic_aberration_strength: settings.chromatic_aberration.strength,
                film_grain_strength: settings.film_grain.strength,
            };
            fullscreen_pipeline.uniform_buffer.upload_data(&[ubo], 0)?;
        }

        let (projection, view) = world.active_camera_matrices(aspect_ratio)?;
        let camera_entity = world.active_camera()?;
        let camera_transform = world.entity_global_transform(camera_entity)?;

        // Maintain a perspective projection for the skybox
        let using_ortho_projection = world
            .ecs
            .entry_ref(camera_entity)?
            .get_component::<Camera>()?
            .is_orthographic();
        let skybox_projection = if using_ortho_projection {
            let camera = PerspectiveCamera {
                aspect_ratio: None,
                y_fov_rad: 70_f32.to_radians(),
                z_far: Some(1000.0),
                z_near: 0.01,
            };
            camera.matrix(aspect_ratio)
        } else {
            projection
        };

        self.skybox_render.projection = skybox_projection;
        self.skybox_render.view = view;

        if let Some(world_render) = self.world_render.as_mut() {
            world_render.pbr_pipeline_data.update_dynamic_ubo(world)?;
            let (lights, number_of_lights) = Self::load_lights(world)?;

            let mut joint_matrices = [glm::Mat4::identity(); PbrPipelineData::MAX_NUMBER_OF_JOINTS];
            joint_matrices
                .iter_mut()
                .zip(world.joint_matrices()?.into_iter())
                .for_each(|(a, b)| *a = b);

            let ubo = WorldUniformBuffer {
                view,
                projection,
                camera_position: camera_transform.decompose().translation,
                number_of_lights,
                lights,
                joint_matrices,
            };
            world_render
                .pbr_pipeline_data
                .uniform_buffer
                .upload_data(&[ubo], 0)?;
        }

        Ok(())
    }

    fn load_lights(world: &World) -> Result<([Light; PbrPipelineData::MAX_NUMBER_OF_LIGHTS], u32)> {
        let mut lights = [Light::default(); PbrPipelineData::MAX_NUMBER_OF_LIGHTS];
        let world_lights = world
            .lights()?
            .iter()
            .map(|(transform, light)| Light::from_node(transform, light))
            .collect::<Vec<_>>();
        let number_of_lights = world_lights.len() as u32;
        lights
            .iter_mut()
            .zip(world_lights)
            .for_each(|(a, b)| *a = b);
        Ok((lights, number_of_lights))
    }

    pub fn execute_passes(
        &mut self,
        command_buffer: CommandBuffer,
        world: &World,
        image_index: usize,
        aspect_ratio: f32,
        viewport: Viewport,
        clipped_meshes: &[ClippedMesh],
    ) -> Result<()> {
        let device = &self.context.device.clone();
        self.rendergraph.execute_pass(
            command_buffer,
            "offscreen",
            image_index,
            |pass, command_buffer| {
                device.update_viewport(command_buffer, pass.extent, true)?;
                self.skybox_render.issue_commands(command_buffer)?;
                if let Some(world_render) = self.world_render.as_ref() {
                    world_render.issue_commands(command_buffer, world, aspect_ratio)?;
                }
                Ok(())
            },
        )?;

        self.rendergraph.execute_pass(
            command_buffer,
            "fullscreen",
            image_index,
            |pass, command_buffer| {
                device.update_viewport(command_buffer, pass.extent, false)?;
                if let Some(fullscreen_pipeline) = self.fullscreen_pipeline.as_ref() {
                    fullscreen_pipeline.issue_commands(command_buffer)?;
                }
                self.gui_render
                    .issue_commands(viewport, command_buffer, &clipped_meshes)?;
                Ok(())
            },
        )?;

        Ok(())
    }
}
