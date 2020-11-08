use crate::{
    adapters::{
        CommandPool, DescriptorPool, DescriptorSetLayout, GraphicsPipeline,
        GraphicsPipelineSettings, GraphicsPipelineSettingsBuilder, PipelineLayout, RenderPass,
    },
    context::{Context, Device},
    cube::Cube,
    cube::CubeRendering,
    gltf::Asset,
    gltf_rendering::AssetRendering,
    rendergraph::{ImageNode, RenderGraph},
    resources::{Image, RawImage, ShaderCache, ShaderPathSet, ShaderPathSetBuilder},
    swapchain::{Swapchain, SwapchainProperties},
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub struct Scene {
    pub transient_command_pool: CommandPool,
    pub shader_cache: ShaderCache,
    pub asset_rendering: Option<Rc<RefCell<AssetRendering>>>,
    pub cube_rendering: Rc<RefCell<CubeRendering>>,
    pub rendergraph: RenderGraph,
    pub pipeline: Rc<RefCell<PostProcessingPipeline>>,
    pub samples: vk::SampleCountFlags,
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

        let mut rendergraph =
            Self::create_rendergraph(context, swapchain, swapchain_properties, samples)?;

        let mut shader_cache = ShaderCache::default();

        let cube = Cube::new(context, &transient_command_pool)?;
        let mut cube_rendering = CubeRendering::new(context.device.clone(), cube);

        let offscreen_renderpass = rendergraph
            .passes
            .get("offscreen")
            .context("Failed to get offscreen pass to create scene")?
            .render_pass
            .clone();
        // cube_rendering.wireframe_enabled = true;
        cube_rendering.create_pipeline(&mut shader_cache, offscreen_renderpass, samples)?;

        let cube_rendering = Rc::new(RefCell::new(cube_rendering));

        let pipeline = PostProcessingPipeline::new(
            context,
            rendergraph.final_pass()?.render_pass.clone(),
            &mut shader_cache,
            rendergraph.image_views["color_resolve"].handle,
            rendergraph.samplers["default"].handle,
        )?;
        let pipeline = Rc::new(RefCell::new(pipeline));

        let pipeline_ptr = pipeline.clone();
        rendergraph
            .passes
            .get_mut("postprocessing")
            .context("Failed to get postprocessing pass to set callback")?
            .set_callback(move |command_buffer| {
                pipeline_ptr.borrow().issue_commands(command_buffer)
            });

        let path = Self {
            transient_command_pool,
            shader_cache,
            asset_rendering: None,
            cube_rendering,
            rendergraph,
            pipeline,
            samples,
        };

        Ok(path)
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
        let postprocessing = "postprocessing";
        let color = "color";
        let color_resolve = "color_resolve";
        let offscreen_extent = vk::Extent2D::builder().width(2048).height(2048).build();
        let mut rendergraph = RenderGraph::new(
            &[offscreen, postprocessing],
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
                },
            ],
            &[
                (offscreen, color),
                (offscreen, color_resolve),
                (offscreen, RenderGraph::DEPTH_STENCIL),
                (color_resolve, postprocessing),
                (postprocessing, &RenderGraph::backbuffer_name(0)),
            ],
        )?;

        rendergraph.build(device.clone(), allocator)?;

        rendergraph
            .passes
            .get_mut(offscreen)
            .context("Failed to get offscreen pass to flip viewport on!")?
            .flip_viewport = true;

        let swapchain_images = swapchain
            .images()?
            .into_iter()
            .map(|handle| Box::new(RawImage(handle)) as Box<dyn Image>)
            .collect::<Vec<_>>();
        rendergraph.insert_backbuffer_images(device, swapchain_images)?;

        Ok(rendergraph)
    }

    pub fn load_asset(&mut self, context: &Context, asset: Rc<RefCell<Asset>>) -> Result<()> {
        let offscreen_renderpass = self
            .rendergraph
            .passes
            .get("offscreen")
            .context("Failed to get offscreen pass to create scene")?
            .render_pass
            .clone();

        let mut rendering = AssetRendering::new(context, &self.transient_command_pool, asset)?;
        rendering.create_pipeline(&mut self.shader_cache, offscreen_renderpass, self.samples)?;

        self.asset_rendering = None;
        let asset_rendering = Rc::new(RefCell::new(rendering));
        let asset_rendering_ptr = asset_rendering.clone();
        self.asset_rendering = Some(asset_rendering);

        let cube_rendering_ptr = self.cube_rendering.clone();
        self.rendergraph
            .passes
            .get_mut("offscreen")
            .context("Failed to get offscreen pass to set scene callback")?
            .set_callback(move |command_buffer| {
                // asset_rendering_ptr
                //     .borrow()
                //     .issue_commands(command_buffer)?;
                cube_rendering_ptr.borrow().issue_commands(command_buffer)
            });

        Ok(())
    }
}

pub struct PostProcessingPipeline {
    pub pipeline: Option<GraphicsPipeline>,
    pub pipeline_layout: PipelineLayout,
    pub descriptor_pool: DescriptorPool,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub descriptor_set: vk::DescriptorSet,
    device: Arc<Device>,
}

impl PostProcessingPipeline {
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
            .context("Failed to get post-processing pipeline!")?;
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
