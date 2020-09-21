use super::{
    core::{Context, LogicalDevice},
    render::{
        DescriptorPool, DescriptorSetLayout, Framebuffer, GraphicsPipeline,
        GraphicsPipelineSettings, GraphicsPipelineSettingsBuilder, Image, ImageView,
        PipelineLayout, RenderPass, Sampler, ShaderCache, ShaderPathSet, ShaderPathSetBuilder,
        Swapchain, SwapchainProperties,
    },
};
use anyhow::{anyhow, Result};
use ash::{version::DeviceV1_0, vk};
use std::sync::Arc;
use vk_mem::Allocator;

// TODO: Move this into main renderer
pub struct RenderPath {
    pub offscreen: OffscreenBuffer,
    pub swapchain: ForwardSwapchain,
    pub pipeline: PostProcessingPipeline,
    device: Arc<LogicalDevice>,
}

impl RenderPath {
    pub fn new(
        context: &Context,
        dimensions: &[u32; 2],
        shader_cache: &mut ShaderCache,
    ) -> Result<Self> {
        let offscreen = OffscreenBuffer::new(context)?;
        let swapchain = ForwardSwapchain::new(context, dimensions)?;
        let pipeline = PostProcessingPipeline::new(
            context,
            swapchain.render_pass.clone(),
            shader_cache,
            &offscreen.color_target,
        )?;
        let path = Self {
            offscreen,
            swapchain,
            pipeline,
            device: context.logical_device.clone(),
        };
        Ok(path)
    }

    pub fn record_renderpass<O, F>(
        &self,
        command_buffer: vk::CommandBuffer,
        image_index: usize,
        offscreen_action: O,
        mut final_action: F,
    ) -> Result<()>
    where
        O: FnMut(vk::CommandBuffer) -> Result<()>,
        F: FnMut(vk::CommandBuffer) -> Result<()>,
    {
        let offscreen_extent = vk::Extent2D::builder()
            .width(OffscreenBuffer::DIMENSION)
            .height(OffscreenBuffer::DIMENSION)
            .build();
        self.update_viewport(command_buffer, offscreen_extent)?;
        self.offscreen
            .record_renderpass(command_buffer, offscreen_action)?;

        let swapchain_extent = self.swapchain.swapchain_properties.extent;
        self.update_viewport(command_buffer, swapchain_extent)?;
        self.swapchain
            .record_renderpass(command_buffer, image_index, |command_buffer| {
                self.pipeline.issue_commands(command_buffer)?;
                final_action(command_buffer)
            })?;

        Ok(())
    }

    fn update_viewport(
        &self,
        command_buffer: vk::CommandBuffer,
        extent: vk::Extent2D,
    ) -> Result<()> {
        let viewport = vk::Viewport::builder()
            .y(extent.height as _)
            .width(extent.width as _)
            .height((-1.0 * extent.height as f32) as _)
            .max_depth(1.0)
            .build();
        let viewports = [viewport];

        let scissor = vk::Rect2D::builder().extent(extent).build();
        let scissors = [scissor];

        unsafe {
            self.device
                .handle
                .cmd_set_viewport(command_buffer, 0, &viewports);
            self.device
                .handle
                .cmd_set_scissor(command_buffer, 0, &scissors);
        }

        Ok(())
    }
}

pub struct ForwardSwapchain {
    pub render_pass: Arc<RenderPass>,
    pub framebuffers: Vec<Framebuffer>,
    pub swapchain: Swapchain,
    pub swapchain_properties: SwapchainProperties,
    device: Arc<LogicalDevice>,
}

impl ForwardSwapchain {
    pub fn new(context: &Context, dimensions: &[u32; 2]) -> Result<Self> {
        let (swapchain, swapchain_properties) = context.create_swapchain(dimensions)?;
        let surface_format = swapchain_properties.surface_format.format;

        let render_pass = Self::render_pass(context.logical_device.clone(), surface_format)?;

        let framebuffers = swapchain
            .images
            .iter()
            .map(|image| [image.view.handle])
            .map(|attachments| {
                let create_info = vk::FramebufferCreateInfo::builder()
                    .render_pass(render_pass.handle)
                    .attachments(&attachments)
                    .width(swapchain_properties.extent.width)
                    .height(swapchain_properties.extent.height)
                    .layers(1);
                Framebuffer::new(context.logical_device.clone(), create_info)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let forward_swapchain = Self {
            render_pass: Arc::new(render_pass),
            framebuffers,
            swapchain,
            swapchain_properties,
            device: context.logical_device.clone(),
        };
        Ok(forward_swapchain)
    }

    fn render_pass(device: Arc<LogicalDevice>, color_format: vk::Format) -> Result<RenderPass> {
        let color_attachment_description = Self::color_attachment_description(color_format);
        let attachment_descriptions = [color_attachment_description.build()];

        let color_attachment_reference = vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let color_attachment_references = [color_attachment_reference.build()];

        let subpass_description = vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attachment_references);
        let subpass_descriptions = [subpass_description.build()];

        let subpass_dependencies = Self::subpass_dependencies();

        let create_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attachment_descriptions)
            .subpasses(&subpass_descriptions)
            .dependencies(&subpass_dependencies);

        RenderPass::new(device, &create_info)
    }

    fn color_attachment_description<'a>(
        format: vk::Format,
    ) -> vk::AttachmentDescriptionBuilder<'a> {
        vk::AttachmentDescription::builder()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
    }

    fn subpass_dependencies() -> [vk::SubpassDependency; 2] {
        [
            vk::SubpassDependency::builder()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(vk::PipelineStageFlags::BOTTOM_OF_PIPE)
                .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags::MEMORY_READ)
                .dst_access_mask(
                    vk::AccessFlags::COLOR_ATTACHMENT_READ
                        | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                )
                .build(),
            vk::SubpassDependency::builder()
                .src_subpass(0)
                .dst_subpass(vk::SUBPASS_EXTERNAL)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_stage_mask(vk::PipelineStageFlags::BOTTOM_OF_PIPE)
                .src_access_mask(
                    vk::AccessFlags::COLOR_ATTACHMENT_READ
                        | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                )
                .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                .build(),
        ]
    }

    fn framebuffer_at(&self, image_index: usize) -> Result<vk::Framebuffer> {
        let framebuffer = self
            .framebuffers
            .get(image_index)
            .ok_or_else(|| {
                anyhow!(
                    "No framebuffer was found for the forward swapchain at image index: {}",
                    image_index
                )
            })?
            .handle;
        Ok(framebuffer)
    }

    fn record_renderpass<T>(
        &self,
        command_buffer: vk::CommandBuffer,
        image_index: usize,
        action: T,
    ) -> Result<()>
    where
        T: FnMut(vk::CommandBuffer) -> Result<()>,
    {
        let extent = self.swapchain_properties.extent;
        let render_area = vk::Rect2D::builder().extent(extent).build();

        let clear_values = Self::clear_values();
        let begin_info = vk::RenderPassBeginInfo::builder()
            .render_pass(self.render_pass.handle)
            .framebuffer(self.framebuffer_at(image_index)?)
            .render_area(render_area)
            .clear_values(&clear_values);

        RenderPass::record(self.device.clone(), command_buffer, begin_info, action)?;
        Ok(())
    }

    fn clear_values() -> Vec<vk::ClearValue> {
        let color = vk::ClearColorValue {
            float32: [0.39, 0.58, 0.93, 1.0], // Cornflower blue
        };
        vec![vk::ClearValue { color }]
    }
}

pub struct DepthRenderTarget {
    _image: Image,
    pub view: ImageView,
    pub format: vk::Format,
}

impl DepthRenderTarget {
    fn new(
        device: Arc<LogicalDevice>,
        allocator: Arc<Allocator>,
        extent: vk::Extent3D,
        format: vk::Format,
    ) -> Result<Self> {
        let image = Self::image(allocator, extent, format)?;
        let view = Self::view(device, &image, format)?;
        let target = Self {
            _image: image,
            view,
            format,
        };
        Ok(target)
    }

    fn image(allocator: Arc<Allocator>, extent: vk::Extent3D, format: vk::Format) -> Result<Image> {
        let create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .format(format)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1)
            .flags(vk::ImageCreateFlags::empty());

        let allocation_create_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::GpuOnly,
            ..Default::default()
        };

        Image::new(allocator, &allocation_create_info, &create_info)
    }

    fn view(device: Arc<LogicalDevice>, image: &Image, format: vk::Format) -> Result<ImageView> {
        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image.handle)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .components(vk::ComponentMapping {
                r: vk::ComponentSwizzle::IDENTITY,
                g: vk::ComponentSwizzle::IDENTITY,
                b: vk::ComponentSwizzle::IDENTITY,
                a: vk::ComponentSwizzle::IDENTITY,
            })
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        ImageView::new(device, create_info)
    }
}

pub struct ColorRenderTarget {
    _image: Image,
    pub view: ImageView,
    pub format: vk::Format,
    pub sampler: Sampler,
}

impl ColorRenderTarget {
    fn new(
        device: Arc<LogicalDevice>,
        allocator: Arc<Allocator>,
        extent: vk::Extent3D,
        format: vk::Format,
    ) -> Result<Self> {
        let image = Self::image(allocator, extent, format)?;
        let view = Self::view(device.clone(), &image, format)?;
        let sampler = Self::sampler(device)?;
        let target = Self {
            _image: image,
            view,
            format,
            sampler,
        };
        Ok(target)
    }

    fn image(allocator: Arc<Allocator>, extent: vk::Extent3D, format: vk::Format) -> Result<Image> {
        let create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .format(format)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1)
            .flags(vk::ImageCreateFlags::empty());

        let allocation_create_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::GpuOnly,
            ..Default::default()
        };

        Image::new(allocator, &allocation_create_info, &create_info)
    }

    fn view(device: Arc<LogicalDevice>, image: &Image, format: vk::Format) -> Result<ImageView> {
        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image.handle)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .components(vk::ComponentMapping {
                r: vk::ComponentSwizzle::IDENTITY,
                g: vk::ComponentSwizzle::IDENTITY,
                b: vk::ComponentSwizzle::IDENTITY,
                a: vk::ComponentSwizzle::IDENTITY,
            })
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        ImageView::new(device, create_info)
    }

    fn sampler(device: Arc<LogicalDevice>) -> Result<Sampler> {
        let sampler_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .anisotropy_enable(true)
            .max_anisotropy(1.0)
            .border_color(vk::BorderColor::INT_OPAQUE_WHITE)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .compare_op(vk::CompareOp::ALWAYS)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(1.0);
        Sampler::new(device, sampler_info)
    }
}

pub struct OffscreenBuffer {
    pub color_target: ColorRenderTarget,
    pub depth_target: DepthRenderTarget,
    pub render_pass: Arc<RenderPass>,
    pub framebuffer: Framebuffer,
    device: Arc<LogicalDevice>,
}

impl OffscreenBuffer {
    pub const DIMENSION: u32 = 2048;
    pub const FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;

    pub fn new(context: &Context) -> Result<Self> {
        let allocator = context.allocator.clone();
        let device = context.logical_device.clone();

        let extent = vk::Extent3D::builder()
            .width(Self::DIMENSION)
            .height(Self::DIMENSION)
            .depth(1)
            .build();

        let depth_format = context.determine_depth_format(
            vk::ImageTiling::OPTIMAL,
            vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT,
        )?;

        let depth_target =
            DepthRenderTarget::new(device.clone(), allocator.clone(), extent, depth_format)?;
        let color_target = ColorRenderTarget::new(device.clone(), allocator, extent, Self::FORMAT)?;

        let render_pass = Self::render_pass(
            context.logical_device.clone(),
            Self::FORMAT,
            depth_target.format,
        )?;
        let render_pass = Arc::new(render_pass);

        let attachments = [color_target.view.handle, depth_target.view.handle];
        let create_info = vk::FramebufferCreateInfo::builder()
            .render_pass(render_pass.handle)
            .attachments(&attachments)
            .width(Self::DIMENSION)
            .height(Self::DIMENSION)
            .layers(1);
        let framebuffer = Framebuffer::new(device.clone(), create_info)?;

        let buffer = Self {
            depth_target,
            color_target,
            render_pass,
            framebuffer,
            device,
        };

        Ok(buffer)
    }

    fn render_pass(
        device: Arc<LogicalDevice>,
        color_format: vk::Format,
        depth_format: vk::Format,
    ) -> Result<RenderPass> {
        let color_attachment_description = Self::color_attachment_description(color_format);
        let depth_attachment_description = Self::depth_attachment_description(depth_format);
        let attachment_descriptions = [
            color_attachment_description.build(),
            depth_attachment_description.build(),
        ];

        let color_attachment_reference = vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let color_attachment_references = [color_attachment_reference.build()];

        let depth_attachment_reference = vk::AttachmentReference::builder()
            .attachment(1)
            .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);

        let subpass_description = vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attachment_references)
            .depth_stencil_attachment(&depth_attachment_reference);
        let subpass_descriptions = [subpass_description.build()];

        let subpass_dependencies = Self::subpass_dependencies();

        let create_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attachment_descriptions)
            .subpasses(&subpass_descriptions)
            .dependencies(&subpass_dependencies);

        RenderPass::new(device, &create_info)
    }

    fn color_attachment_description<'a>(
        format: vk::Format,
    ) -> vk::AttachmentDescriptionBuilder<'a> {
        vk::AttachmentDescription::builder()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
    }

    fn depth_attachment_description<'a>(
        format: vk::Format,
    ) -> vk::AttachmentDescriptionBuilder<'a> {
        vk::AttachmentDescription::builder()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
    }

    fn subpass_dependencies() -> [vk::SubpassDependency; 2] {
        [
            vk::SubpassDependency::builder()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(vk::PipelineStageFlags::BOTTOM_OF_PIPE)
                .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags::MEMORY_READ)
                .dst_access_mask(
                    vk::AccessFlags::COLOR_ATTACHMENT_READ
                        | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                )
                .build(),
            vk::SubpassDependency::builder()
                .src_subpass(0)
                .dst_subpass(vk::SUBPASS_EXTERNAL)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_stage_mask(vk::PipelineStageFlags::BOTTOM_OF_PIPE)
                .src_access_mask(
                    vk::AccessFlags::COLOR_ATTACHMENT_READ
                        | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                )
                .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                .build(),
        ]
    }

    fn record_renderpass<T>(&self, command_buffer: vk::CommandBuffer, action: T) -> Result<()>
    where
        T: FnMut(vk::CommandBuffer) -> Result<()>,
    {
        let extent = vk::Extent2D::builder()
            .width(Self::DIMENSION)
            .height(Self::DIMENSION)
            .build();
        let render_area = vk::Rect2D::builder().extent(extent).build();

        let clear_values = Self::clear_values();
        let begin_info = vk::RenderPassBeginInfo::builder()
            .render_pass(self.render_pass.handle)
            .framebuffer(self.framebuffer.handle)
            .render_area(render_area)
            .clear_values(&clear_values);

        RenderPass::record(self.device.clone(), command_buffer, begin_info, action)?;
        Ok(())
    }

    fn clear_values() -> Vec<vk::ClearValue> {
        let color = vk::ClearColorValue {
            float32: [0.39, 0.58, 0.93, 1.0], // Cornflower blue
        };
        let depth_stencil = vk::ClearDepthStencilValue {
            depth: 1.0,
            stencil: 0,
        };
        vec![vk::ClearValue { color }, vk::ClearValue { depth_stencil }]
    }
}

pub struct PostProcessingPipeline {
    pub pipeline: Option<GraphicsPipeline>,
    pub pipeline_layout: PipelineLayout,
    pub descriptor_pool: DescriptorPool,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
    pub descriptor_set: vk::DescriptorSet,
    device: Arc<LogicalDevice>,
}

impl PostProcessingPipeline {
    pub fn new(
        context: &Context,
        render_pass: Arc<RenderPass>,
        shader_cache: &mut ShaderCache,
        color_target: &ColorRenderTarget, // TODO: Make this a trait
    ) -> Result<Self> {
        let device = context.logical_device.clone();
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
        rendering.update_descriptor_set(color_target);
        Ok(rendering)
    }

    fn shader_paths() -> Result<ShaderPathSet> {
        let shader_path_set = ShaderPathSetBuilder::default()
            .vertex("dragonglass/shaders/postprocessing/fullscreen_triangle.vert.spv")
            .fragment("dragonglass/shaders/postprocessing/postprocess.frag.spv")
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
        let settings = GraphicsPipelineSettingsBuilder::default()
            .shader_set(shader_set)
            .render_pass(render_pass)
            .vertex_state_info(vk::PipelineVertexInputStateCreateInfo::builder().build())
            .descriptor_set_layout(descriptor_set_layout)
            .build()
            .map_err(|error| anyhow!("{}", error))?;
        Ok(settings)
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

    fn update_descriptor_set(&mut self, target: &ColorRenderTarget) {
        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(target.view.handle)
            .sampler(target.sampler.handle);
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
            .ok_or_else(|| anyhow!("Failed to get post-processing pipeline!"))?;
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
