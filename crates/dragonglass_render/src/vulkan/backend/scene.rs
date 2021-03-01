use crate::vulkan::{
    core::{
        CommandPool, Context, Cubemap, Device, Image, ImageNode, RawImage, RenderGraph, Sampler,
        ShaderCache, Swapchain, SwapchainProperties,
    },
    pbr::hdr_cubemap,
    render::{FullscreenRender, GuiRender, SkyboxRender, WorldRender},
};
use anyhow::Result;
use ash::vk;
use dragonglass_world::World;
use imgui::Context as ImguiContext;
use std::{path::Path, sync::Arc};

pub struct Scene {
    pub world_render: Option<WorldRender>,
    pub skybox_render: SkyboxRender,
    pub fullscreen_pipeline: Option<FullscreenRender>,
    pub gui_render: GuiRender,
    pub rendergraph: RenderGraph,
    pub transient_command_pool: CommandPool,
    pub shader_cache: ShaderCache,
    pub samples: vk::SampleCountFlags,
    pub skybox: Cubemap,
    pub skybox_sampler: Sampler,
}

impl Scene {
    pub fn new(
        context: &Context,
        imgui: &mut ImguiContext,
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
            Self::create_rendergraph(context, swapchain, swapchain_properties, samples)?;
        let mut shader_cache = ShaderCache::default();

        let (skybox, skybox_sampler) = hdr_cubemap(
            context,
            &transient_command_pool,
            "assets/skyboxes/desert.hdr",
            &mut shader_cache,
        )?;

        let skybox_render = SkyboxRender::new(
            context,
            &transient_command_pool,
            skybox.view.handle,
            skybox_sampler.handle,
        )?;

        let fullscreen_pass = rendergraph.pass_handle("fullscreen")?;
        let gui_render = GuiRender::new(
            context,
            &mut shader_cache,
            fullscreen_pass,
            imgui,
            &transient_command_pool,
        )?;

        let mut scene = Self {
            world_render: None,
            skybox_render,
            fullscreen_pipeline: None,
            gui_render,
            rendergraph,
            transient_command_pool,
            shader_cache,
            samples,
            skybox,
            skybox_sampler,
        };
        scene.create_pipelines(context)?;
        Ok(scene)
    }

    pub fn create_pipelines(&mut self, context: &Context) -> Result<()> {
        let fullscreen_pass = self.rendergraph.pass_handle("fullscreen")?;

        self.fullscreen_pipeline = None;
        let fullscreen_pipeline = FullscreenRender::new(
            context,
            fullscreen_pass.clone(),
            &mut self.shader_cache,
            self.rendergraph.image_view("color_resolve")?.handle,
            self.rendergraph.sampler("default")?.handle,
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

        let swapchain_images = swapchain
            .images()?
            .into_iter()
            .map(|handle| Box::new(RawImage(handle)) as Box<dyn Image>)
            .collect::<Vec<_>>();
        rendergraph.insert_backbuffer_images(device, swapchain_images)?;

        Ok(rendergraph)
    }

    pub fn load_skybox(&mut self, context: &Context, path: impl AsRef<Path>) -> Result<()> {
        let (skybox, skybox_sampler) = hdr_cubemap(
            context,
            &self.transient_command_pool,
            path,
            &mut self.shader_cache,
        )?;
        self.skybox_render.update_descriptor_set(
            context.device.clone(),
            skybox.view.handle,
            self.skybox_sampler.handle,
        );
        self.skybox = skybox;
        self.skybox_sampler = skybox_sampler;
        Ok(())
    }

    pub fn load_world(&mut self, context: &Context, world: &World) -> Result<()> {
        self.world_render = None;
        let offscreen_renderpass = self.rendergraph.pass_handle("offscreen")?;
        let mut rendering = WorldRender::new(
            context,
            &self.transient_command_pool,
            &mut self.shader_cache,
            world,
        )?;
        rendering.create_pipeline(&mut self.shader_cache, offscreen_renderpass, self.samples)?;
        self.world_render = Some(rendering);
        Ok(())
    }
}
