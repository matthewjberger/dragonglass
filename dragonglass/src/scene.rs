use crate::{
    adapters::{
        CommandPool, DescriptorPool, DescriptorSetLayout, GraphicsPipeline,
        GraphicsPipelineSettings, GraphicsPipelineSettingsBuilder, PipelineLayout, RenderPass,
    },
    context::{Context, Device},
    gltf::Asset,
    gltf_rendering::AssetRendering,
    hdr::hdr_cubemap,
    rendergraph::{ImageNode, RenderGraph},
    resources::Cubemap,
    resources::Sampler,
    resources::{Image, RawImage, ShaderCache, ShaderPathSet, ShaderPathSetBuilder},
    skybox::SkyboxRendering,
    swapchain::{Swapchain, SwapchainProperties},
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use std::{path::Path, sync::Arc};

pub struct Scene {
    pub asset_rendering: Option<AssetRendering>,
    pub skybox_rendering: SkyboxRendering,
    pub fullscreen_pipeline: Option<FullscreenPipeline>,
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
        swapchain: &Swapchain,
        swapchain_properties: &SwapchainProperties,
    ) -> Result<Self> {
        let transient_command_pool = Self::transient_command_pool(
            context.device.clone(),
            context.physical_device.graphics_queue_index,
        )?;
        let samples = context.max_usable_samples();
        let rendergraph =
            Self::create_rendergraph(context, swapchain, swapchain_properties, samples)?;
        let mut shader_cache = ShaderCache::default();

        let (skybox, skybox_sampler) = hdr_cubemap(
            context,
            &transient_command_pool,
            "assets/skyboxes/walk_of_fame.hdr",
            &mut shader_cache,
        )?;

        let skybox_rendering = SkyboxRendering::new(
            context,
            &transient_command_pool,
            skybox.view.handle,
            skybox_sampler.handle,
        )?;

        let mut scene = Self {
            asset_rendering: None,
            skybox_rendering,
            fullscreen_pipeline: None,
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
        self.fullscreen_pipeline = None;
        let fullscreen_pipeline = FullscreenPipeline::new(
            context,
            self.rendergraph.pass_handle("fullscreen")?,
            &mut self.shader_cache,
            self.rendergraph.image_view("color_resolve")?.handle,
            self.rendergraph.sampler("default")?.handle,
        )?;
        self.fullscreen_pipeline = Some(fullscreen_pipeline);

        let offscreen_renderpass = self.rendergraph.pass_handle("offscreen")?;
        self.skybox_rendering.create_pipeline(
            &mut self.shader_cache,
            offscreen_renderpass.clone(),
            self.samples,
        )?;

        if let Some(asset_rendering) = self.asset_rendering.as_mut() {
            asset_rendering.create_pipeline(
                &mut self.shader_cache,
                offscreen_renderpass,
                self.samples,
            )?;
        }

        Ok(())
    }

    fn transient_command_pool(device: Arc<Device>, queue_index: u32) -> Result<CommandPool> {
        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_index)
            .flags(vk::CommandPoolCreateFlags::TRANSIENT);
        let command_pool = CommandPool::new(device, create_info)?;
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
        self.skybox_rendering.update_descriptor_set(
            context.device.clone(),
            skybox.view.handle,
            self.skybox_sampler.handle,
        );
        self.skybox = skybox;
        self.skybox_sampler = skybox_sampler;
        Ok(())
    }

    pub fn load_asset(&mut self, context: &Context, asset: &Asset) -> Result<()> {
        self.asset_rendering = None;
        let offscreen_renderpass = self.rendergraph.pass_handle("offscreen")?;
        let mut rendering = AssetRendering::new(context, &self.transient_command_pool, asset)?;
        rendering.create_pipeline(&mut self.shader_cache, offscreen_renderpass, self.samples)?;
        self.asset_rendering = Some(rendering);
        Ok(())
    }
}

pub struct FullscreenPipeline {
    pub pipeline: Option<GraphicsPipeline>,
    pub pipeline_layout: PipelineLayout,
    pub descriptor_pool: DescriptorPool,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub descriptor_set: vk::DescriptorSet,
    device: Arc<Device>,
}

impl FullscreenPipeline {
    pub fn new(
        context: &Context,
        render_pass: Arc<RenderPass>,
        shader_cache: &mut ShaderCache,
        color_target: vk::ImageView,
        sampler: vk::Sampler,
    ) -> Result<Self> {
        let device = context.device.clone();
        let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
        let descriptor_pool = Self::descriptor_pool(device.clone())?;
        let descriptor_set =
            descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];
        let settings = Self::settings(
            device.clone(),
            shader_cache,
            render_pass,
            descriptor_set_layout.clone(),
        )?;
        let (pipeline, pipeline_layout) = settings.create_pipeline(device.clone())?;
        let mut rendering = Self {
            pipeline: Some(pipeline),
            pipeline_layout,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_set,
            device,
        };
        rendering.update_descriptor_set(color_target, sampler);
        Ok(rendering)
    }

    fn shader_paths() -> Result<ShaderPathSet> {
        let shader_path_set = ShaderPathSetBuilder::default()
            .vertex("assets/shaders/postprocessing/fullscreen_triangle.vert.spv")
            .fragment("assets/shaders/postprocessing/postprocess.frag.spv")
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        Ok(shader_path_set)
    }

    fn settings(
        device: Arc<Device>,
        shader_cache: &mut ShaderCache,
        render_pass: Arc<RenderPass>,
        descriptor_set_layout: Arc<DescriptorSetLayout>,
    ) -> Result<GraphicsPipelineSettings> {
        let shader_paths = Self::shader_paths()?;
        let shader_set = shader_cache.create_shader_set(device, &shader_paths)?;
        let settings = GraphicsPipelineSettingsBuilder::default()
            .shader_set(shader_set)
            .render_pass(render_pass)
            .vertex_inputs(Vec::new())
            .vertex_attributes(Vec::new())
            .descriptor_set_layout(descriptor_set_layout)
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        Ok(settings)
    }

    fn descriptor_pool(device: Arc<Device>) -> Result<DescriptorPool> {
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

    fn descriptor_set_layout(device: Arc<Device>) -> Result<DescriptorSetLayout> {
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

    fn update_descriptor_set(&mut self, target: vk::ImageView, sampler: vk::Sampler) {
        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(target)
            .sampler(sampler);
        let image_info_list = [image_info.build()];

        let sampler_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_info_list);

        let writes = &[sampler_write.build()];
        unsafe { self.device.handle.update_descriptor_sets(writes, &[]) }
    }

    pub fn issue_commands(&self, command_buffer: vk::CommandBuffer) -> Result<()> {
        let pipeline = self
            .pipeline
            .as_ref()
            .context("Failed to get fullscreen pipeline!")?;
        pipeline.bind(&self.device.handle, command_buffer);

        unsafe {
            self.device.handle.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout.handle,
                0,
                &[self.descriptor_set],
                &[],
            );

            self.device.handle.cmd_draw(command_buffer, 3, 1, 0, 0);
        };

        Ok(())
    }
}
