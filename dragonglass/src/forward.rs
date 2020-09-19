use super::{
    core::{Context, LogicalDevice},
    render::{Framebuffer, Image, ImageView, RenderPass, Sampler, Swapchain, SwapchainProperties},
};
use anyhow::Result;
use ash::vk;
use std::sync::Arc;
use vk_mem::Allocator;

// TODO: Make format, image, and view for render targets a trait

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
            .usage(
                vk::ImageUsageFlags::TRANSIENT_ATTACHMENT
                    | vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::SAMPLED,
            )
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

pub struct ForwardSwapchain {
    pub depth_target: DepthRenderTarget,
    pub color_target: ColorRenderTarget,
    pub render_pass: Arc<RenderPass>,
    pub framebuffers: Vec<Framebuffer>,
    pub swapchain: Swapchain,
    pub swapchain_properties: SwapchainProperties,
}

impl ForwardSwapchain {
    pub fn new(context: &Context, dimensions: &[u32; 2]) -> Result<Self> {
        let (swapchain, swapchain_properties) = context.create_swapchain(dimensions)?;
        let surface_format = swapchain_properties.surface_format.format;

        let extent = swapchain_properties.extent;
        let extent = vk::Extent3D::builder()
            .width(extent.width)
            .height(extent.height)
            .depth(1)
            .build();

        let allocator = context.allocator.clone();
        let device = context.logical_device.clone();

        let depth_format = context.determine_depth_format(
            vk::ImageTiling::OPTIMAL,
            vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT,
        )?;

        let depth_target =
            DepthRenderTarget::new(device.clone(), allocator.clone(), extent, depth_format)?;
        let color_target =
            ColorRenderTarget::new(device.clone(), allocator.clone(), extent, surface_format)?;
        let render_pass = Self::render_pass(
            context.logical_device.clone(),
            surface_format,
            depth_target.format,
        )?;
        let framebuffers = swapchain
            .images
            .iter()
            .map(|image| [image.view.handle, depth_target.view.handle])
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
            depth_target,
            color_target,
            render_pass: Arc::new(render_pass),
            framebuffers,
            swapchain,
            swapchain_properties,
        };
        Ok(forward_swapchain)
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

        RenderPass::new(device.clone(), &create_info)
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

    fn subpass_dependencies() -> [vk::SubpassDependency; 1] {
        let subpass_dependency = vk::SubpassDependency::builder()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            );
        [subpass_dependency.build()]
    }
}

pub struct OffscreenBuffer {
    pub depth_target: DepthRenderTarget,
    pub color_target: ColorRenderTarget,
    pub render_pass: Arc<RenderPass>,
    pub framebuffer: Framebuffer,
}

impl OffscreenBuffer {
    pub const DIMENSION: u32 = 2048;
    pub const FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;

    pub fn new(context: &Context) -> Result<Self> {
        let extent = vk::Extent3D::builder()
            .width(Self::DIMENSION)
            .height(Self::DIMENSION)
            .depth(1)
            .build();
        let depth_format = context.determine_depth_format(
            vk::ImageTiling::OPTIMAL,
            vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT,
        )?;
        let device = context.logical_device.clone();
        let allocator = context.allocator.clone();
        let depth_target =
            DepthRenderTarget::new(device.clone(), allocator.clone(), extent, depth_format)?;
        let color_target = ColorRenderTarget::new(device.clone(), allocator, extent, Self::FORMAT)?;
        let render_pass = Self::create_render_pass(device.clone(), Self::FORMAT, depth_format)?;
        let render_pass = Arc::new(render_pass);
        let attachments = [color_target.view.handle, depth_target.view.handle];

        let create_info = vk::FramebufferCreateInfo::builder()
            .render_pass(render_pass.handle)
            .attachments(&attachments)
            .width(Self::DIMENSION)
            .height(Self::DIMENSION)
            .layers(1);

        let framebuffer = Framebuffer::new(device, create_info)?;

        let offscreen_buffer = Self {
            depth_target,
            color_target,
            render_pass,
            framebuffer,
        };

        Ok(offscreen_buffer)
    }

    fn create_render_pass(
        device: Arc<LogicalDevice>,
        format: vk::Format,
        depth_format: vk::Format,
    ) -> Result<RenderPass> {
        let color_attachment_description = vk::AttachmentDescription::builder()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .build();

        let depth_attachment_description = vk::AttachmentDescription::builder()
            .format(depth_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .build();

        let attachment_descriptions = [color_attachment_description, depth_attachment_description];

        let color_attachment_reference = vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();
        let color_attachment_references = [color_attachment_reference];

        let depth_attachment_reference = vk::AttachmentReference::builder()
            .attachment(1)
            .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);

        let subpass_description = vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attachment_references)
            .depth_stencil_attachment(&depth_attachment_reference)
            .build();
        let subpass_descriptions = [subpass_description];
        let subpass_dependencies = [
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
        ];

        let create_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attachment_descriptions)
            .subpasses(&subpass_descriptions)
            .dependencies(&subpass_dependencies);

        RenderPass::new(device, &create_info)
    }
}
