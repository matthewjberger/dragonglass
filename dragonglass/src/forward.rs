use super::{
    core::{Context, LogicalDevice},
    render::{Framebuffer, Image, ImageView, RenderPass, Swapchain, SwapchainProperties},
};
use anyhow::Result;
use ash::vk;
use std::sync::Arc;
use vk_mem::Allocator;

pub struct ForwardSwapchain {
    pub swapchain: Swapchain,
    pub swapchain_properties: SwapchainProperties,
    pub render_pass: RenderPass,
    pub depth_image: Image,
    pub depth_image_view: ImageView,
    pub color_image: Image,
    pub color_image_view: ImageView,
    pub framebuffers: Vec<Framebuffer>,
}

impl ForwardSwapchain {
    pub fn new(context: Arc<Context>, dimensions: &[u32; 2]) -> Result<Self> {
        let depth_format = context.determine_depth_format(
            vk::ImageTiling::OPTIMAL,
            vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT,
        )?;

        let (swapchain, swapchain_properties) = context.create_swapchain(dimensions)?;

        let render_pass = Self::create_render_pass(
            context.logical_device.clone(),
            swapchain_properties.surface_format.format,
            depth_format,
        )?;

        let depth_image = Self::create_depth_image(
            context.allocator.clone(),
            swapchain_properties.extent,
            depth_format,
        )?;

        let depth_image_view = Self::create_depth_image_view(
            context.logical_device.clone(),
            &depth_image,
            depth_format,
        )?;

        let color_image = Self::create_color_image(
            context.allocator.clone(),
            swapchain_properties.extent,
            swapchain_properties.surface_format.format,
        )?;

        let color_image_view = Self::create_color_image_view(
            context.logical_device.clone(),
            &color_image,
            swapchain_properties.surface_format.format,
        )?;

        let framebuffers = swapchain
            .images
            .iter()
            .map(|image| {
                [
                    color_image_view.handle,
                    depth_image_view.handle,
                    image.view.handle,
                ]
            })
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
            swapchain,
            swapchain_properties,
            render_pass,
            depth_image,
            depth_image_view,
            color_image,
            color_image_view,
            framebuffers,
        };

        Ok(forward_swapchain)
    }

    pub fn create_render_pass(
        device: Arc<LogicalDevice>,
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

        RenderPass::new(device, &create_info)
    }

    fn create_depth_image(
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

    fn create_depth_image_view(
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

    fn create_color_image(
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

    fn create_color_image_view(
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
