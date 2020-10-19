use crate::{
    adapters::{
        DescriptorPool, DescriptorSetLayout, Framebuffer, GraphicsPipeline,
        GraphicsPipelineSettings, GraphicsPipelineSettingsBuilder, PipelineLayout, RenderPass,
    },
    context::{Context, LogicalDevice},
    rendergraph::{ImageNode, RenderGraph},
    resources::{Image, RawImage, ShaderCache, ShaderPathSet, ShaderPathSetBuilder},
    swapchain::{create_swapchain, Swapchain, SwapchainProperties},
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ash::{version::DeviceV1_0, vk};
use std::sync::Arc;
use vk_mem::Allocator;

pub struct RenderPath {
    pub rendergraph: RenderGraph,
    pub swapchain: Swapchain,
    pub swapchain_properties: SwapchainProperties,
    // pub pipeline: PostProcessingPipeline,
    device: Arc<LogicalDevice>,
    allocator: Arc<Allocator>,
}

impl RenderPath {
    pub fn new(
        context: &Context,
        dimensions: &[u32; 2],
        shader_cache: &mut ShaderCache,
    ) -> Result<Self> {
        let (swapchain, swapchain_properties) = create_swapchain(context, dimensions)?;

        let rendergraph = Self::create_rendergraph(
            context.logical_device.clone(),
            context.allocator.clone(),
            &swapchain,
            &swapchain_properties,
        )?;

        // let pipeline = PostProcessingPipeline::new(
        //     context,
        //     rendergraph.final_pass()?.render_pass,
        //     shader_cache,
        //     &offscreen.color_target,
        // )?;

        let path = Self {
            rendergraph,
            swapchain,
            swapchain_properties,
            // pipeline,
            device: context.logical_device.clone(),
            allocator: context.allocator.clone(),
        };
        Ok(path)
    }

    pub fn create_rendergraph(
        device: Arc<LogicalDevice>,
        allocator: Arc<Allocator>,
        swapchain: &Swapchain,
        swapchain_properties: &SwapchainProperties,
    ) -> Result<RenderGraph> {
        let offscreen = "offscreen";
        let depth_stencil = "depth_stencil";
        let backbuffer = "backbuffer 0";
        let mut rendergraph = RenderGraph::new(
            &[offscreen],
            vec![
                ImageNode {
                    name: depth_stencil.to_owned(),
                    extent: swapchain_properties.extent,
                    format: vk::Format::D24_UNORM_S8_UINT,
                    clear_value: vk::ClearValue {
                        depth_stencil: vk::ClearDepthStencilValue {
                            depth: 1.0,
                            stencil: 0,
                        },
                    },
                },
                ImageNode {
                    name: backbuffer.to_owned(),
                    extent: swapchain_properties.extent,
                    format: swapchain_properties.surface_format.format,
                    clear_value: vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.39, 0.58, 0.93, 1.0], // Cornflower blue
                        },
                    },
                },
            ],
            &[
                // (offscreen, color),
                (offscreen, depth_stencil),
                // (color, postprocessing),
                // (postprocessing, backbuffer),
                (offscreen, backbuffer),
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
}

// pub struct PostProcessingPipeline {
//     pub pipeline: Option<GraphicsPipeline>,
//     pub pipeline_layout: PipelineLayout,
//     pub descriptor_pool: DescriptorPool,
//     pub descriptor_set_layout: Arc<DescriptorSetLayout>,
//     pub descriptor_set: vk::DescriptorSet,
//     device: Arc<LogicalDevice>,
// }

// impl PostProcessingPipeline {
//     pub fn new(
//         context: &Context,
//         render_pass: Arc<RenderPass>,
//         shader_cache: &mut ShaderCache,
//         color_target: impl Image,
//     ) -> Result<Self> {
//         let device = context.logical_device.clone();
//         let descriptor_set_layout = Arc::new(Self::descriptor_set_layout(device.clone())?);
//         let descriptor_pool = Self::descriptor_pool(device.clone())?;
//         let descriptor_set =
//             descriptor_pool.allocate_descriptor_sets(descriptor_set_layout.handle, 1)?[0];
//         let settings = Self::settings(
//             device.clone(),
//             shader_cache,
//             render_pass,
//             descriptor_set_layout.clone(),
//         )?;
//         let (pipeline, pipeline_layout) = settings.create_pipeline(device.clone())?;
//         let mut rendering = Self {
//             pipeline: Some(pipeline),
//             pipeline_layout,
//             descriptor_pool,
//             descriptor_set_layout,
//             descriptor_set,
//             device,
//         };
//         rendering.update_descriptor_set(color_target);
//         Ok(rendering)
//     }

//     fn shader_paths() -> Result<ShaderPathSet> {
//         let shader_path_set = ShaderPathSetBuilder::default()
//             .vertex("assets/shaders/postprocessing/fullscreen_triangle.vert.spv")
//             .fragment("assets/shaders/postprocessing/postprocess.frag.spv")
//             .build()
//             .map_err(|error| anyhow!("{}", error))?;
//         Ok(shader_path_set)
//     }

//     fn settings(
//         device: Arc<LogicalDevice>,
//         shader_cache: &mut ShaderCache,
//         render_pass: Arc<RenderPass>,
//         descriptor_set_layout: Arc<DescriptorSetLayout>,
//     ) -> Result<GraphicsPipelineSettings> {
//         let shader_paths = Self::shader_paths()?;
//         let shader_set = shader_cache.create_shader_set(device, &shader_paths)?;
//         let settings = GraphicsPipelineSettingsBuilder::default()
//             .shader_set(shader_set)
//             .render_pass(render_pass)
//             .vertex_state_info(vk::PipelineVertexInputStateCreateInfo::builder().build())
//             .descriptor_set_layout(descriptor_set_layout)
//             .build()
//             .map_err(|error| anyhow!("{}", error))?;
//         Ok(settings)
//     }

//     fn descriptor_pool(device: Arc<LogicalDevice>) -> Result<DescriptorPool> {
//         let sampler_pool_size = vk::DescriptorPoolSize::builder()
//             .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
//             .descriptor_count(1)
//             .build();
//         let pool_sizes = [sampler_pool_size];

//         let pool_info = vk::DescriptorPoolCreateInfo::builder()
//             .pool_sizes(&pool_sizes)
//             .max_sets(1);

//         DescriptorPool::new(device, pool_info)
//     }

//     fn descriptor_set_layout(device: Arc<LogicalDevice>) -> Result<DescriptorSetLayout> {
//         let sampler_binding = vk::DescriptorSetLayoutBinding::builder()
//             .binding(0)
//             .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
//             .descriptor_count(1)
//             .stage_flags(vk::ShaderStageFlags::FRAGMENT)
//             .build();
//         let bindings = [sampler_binding];

//         let create_info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);
//         DescriptorSetLayout::new(device, create_info)
//     }

//     fn update_descriptor_set(&mut self, target: impl Image) {
//         let image_info = vk::DescriptorImageInfo::builder()
//             .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
//             .image_view(target.view.handle)
//             .sampler(target.sampler.handle);
//         let image_info_list = [image_info.build()];

//         let sampler_write = vk::WriteDescriptorSet::builder()
//             .dst_set(self.descriptor_set)
//             .dst_binding(0)
//             .dst_array_element(0)
//             .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
//             .image_info(&image_info_list);

//         let writes = &[sampler_write.build()];
//         unsafe { self.device.handle.update_descriptor_sets(writes, &[]) }
//     }

//     pub fn issue_commands(&self, command_buffer: vk::CommandBuffer) -> Result<()> {
//         let pipeline = self
//             .pipeline
//             .as_ref()
//             .context("Failed to get post-processing pipeline!")?;
//         pipeline.bind(&self.device.handle, command_buffer);

//         unsafe {
//             self.device.handle.cmd_bind_descriptor_sets(
//                 command_buffer,
//                 vk::PipelineBindPoint::GRAPHICS,
//                 self.pipeline_layout.handle,
//                 0,
//                 &[self.descriptor_set],
//                 &[],
//             );

//             self.device.handle.cmd_draw(command_buffer, 3, 1, 0, 0);
//         };

//         Ok(())
//     }
// }
