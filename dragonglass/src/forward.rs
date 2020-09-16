use super::{
    core::{Context, LogicalDevice},
    render::{Framebuffer, Image, ImageView, RenderPass, Swapchain, SwapchainProperties},
};
use anyhow::Result;
use ash::vk;
use std::sync::Arc;
use vk_mem::Allocator;

pub struct RenderTarget {
    _image: Image,
    view: ImageView,
}

pub struct ForwardSwapchain {
    pub depth_target: RenderTarget,
    pub color_target: RenderTarget,
    pub render_pass: Arc<RenderPass>,
    pub framebuffers: Vec<Framebuffer>,
    pub swapchain: Swapchain,
    pub swapchain_properties: SwapchainProperties,
}

impl ForwardSwapchain {
    pub fn new(context: Arc<Context>, dimensions: &[u32; 2]) -> Result<Self> {
        let (swapchain, swapchain_properties) = context.create_swapchain(dimensions)?;
        let surface_format = swapchain_properties.surface_format.format;
        let depth_format = Self::depth_format(context.clone())?;
        let render_pass = Self::render_pass(context.clone(), surface_format, depth_format)?;
        let extent = swapchain_properties.extent;
        let depth_target = Self::depth_target(context.clone(), depth_format, extent)?;
        let color_target = Self::color_target(context.clone(), surface_format, extent)?;
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
        context: Arc<Context>,
        color_format: vk::Format,
        depth_format: vk::Format,
    ) -> Result<RenderPass> {
        let color_attachment_description = vk::AttachmentDescription::builder()
            .format(color_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

        let depth_attachment_description = vk::AttachmentDescription::builder()
            .format(depth_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);

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

        let subpass_dependency = vk::SubpassDependency::builder()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            );
        let subpass_dependencies = [subpass_dependency.build()];

        let create_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attachment_descriptions)
            .subpasses(&subpass_descriptions)
            .dependencies(&subpass_dependencies);

        RenderPass::new(context.logical_device.clone(), &create_info)
    }

    fn depth_target(
        context: Arc<Context>,
        format: vk::Format,
        extent: vk::Extent2D,
    ) -> Result<RenderTarget> {
        let image = Self::depth_image(context.allocator.clone(), extent, format)?;
        let view = Self::depth_image_view(context.logical_device.clone(), &image, format)?;
        let target = RenderTarget {
            _image: image,
            view,
        };
        Ok(target)
    }

    fn depth_format(context: Arc<Context>) -> Result<vk::Format> {
        context.determine_depth_format(
            vk::ImageTiling::OPTIMAL,
            vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT,
        )
    }

    fn depth_image(
        allocator: Arc<Allocator>,
        swapchain_extent: vk::Extent2D,
        depth_format: vk::Format,
    ) -> Result<Image> {
        let image_create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(vk::Extent3D {
                width: swapchain_extent.width,
                height: swapchain_extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .format(depth_format)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1)
            .flags(vk::ImageCreateFlags::empty());

        let image_allocation_create_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::GpuOnly,
            ..Default::default()
        };

        Image::new(allocator, &image_allocation_create_info, &image_create_info)
    }

    fn depth_image_view(
        device: Arc<LogicalDevice>,
        depth_image: &Image,
        depth_format: vk::Format,
    ) -> Result<ImageView> {
        let create_info = vk::ImageViewCreateInfo::builder()
            .image(depth_image.handle)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(depth_format)
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

    fn color_target(
        context: Arc<Context>,
        format: vk::Format,
        extent: vk::Extent2D,
    ) -> Result<RenderTarget> {
        let image = Self::color_image(context.allocator.clone(), extent, format)?;
        let view = Self::color_image_view(context.logical_device.clone(), &image, format)?;
        let target = RenderTarget {
            _image: image,
            view,
        };
        Ok(target)
    }

    fn color_image(
        allocator: Arc<Allocator>,
        swapchain_extent: vk::Extent2D,
        color_format: vk::Format,
    ) -> Result<Image> {
        let image_create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(vk::Extent3D {
                width: swapchain_extent.width,
                height: swapchain_extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .format(color_format)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(
                vk::ImageUsageFlags::TRANSIENT_ATTACHMENT | vk::ImageUsageFlags::COLOR_ATTACHMENT,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1)
            .flags(vk::ImageCreateFlags::empty());

        let image_allocation_create_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::GpuOnly,
            ..Default::default()
        };

        Image::new(allocator, &image_allocation_create_info, &image_create_info)
    }

    fn color_image_view(
        device: Arc<LogicalDevice>,
        color_image: &Image,
        color_format: vk::Format,
    ) -> Result<ImageView> {
        let create_info = vk::ImageViewCreateInfo::builder()
            .image(color_image.handle)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(color_format)
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
}
