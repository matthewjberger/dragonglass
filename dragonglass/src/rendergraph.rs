use crate::{
    adapters::{Framebuffer, RenderPass},
    context::Device,
    resources::AllocatedImage,
    resources::{Image, ImageView, Sampler},
};
use anyhow::{bail, ensure, Context, Result};
use ash::vk;
use petgraph::prelude::*;
use std::{collections::HashMap, fmt, sync::Arc};
use vk_mem::Allocator;

#[derive(Default)]
pub struct RenderGraph {
    graph: Graph<Node, ()>,
    passes: HashMap<String, Pass>,
    images: HashMap<String, Box<dyn Image>>,
    image_views: HashMap<String, ImageView>,
    samplers: HashMap<String, Sampler>,
    framebuffers: HashMap<String, Framebuffer>,
}

impl RenderGraph {
    pub const BACKBUFFER_PREFIX: &'static str = "backbuffer";
    pub const RESOLVE_SUFFIX: &'static str = "resolve";
    pub const DEPTH_STENCIL: &'static str = "depth_stencil";

    pub fn backbuffer_name(index: usize) -> String {
        format!("{} {}", Self::BACKBUFFER_PREFIX, index)
    }

    pub fn new<'a>(
        passes: &[&'a str],
        images: Vec<ImageNode>,
        links: &[(&'a str, &'a str)],
    ) -> Result<Self> {
        let mut graph: Graph<Node, ()> = Graph::new();
        let mut index_map = HashMap::new();

        for pass in passes.iter() {
            let pass_index = graph.add_node(Node::Pass(PassNode::new(pass)));
            index_map.insert((*pass).to_string(), pass_index);
        }

        for image in images.into_iter() {
            let name = image.name.to_string();
            let image_index = graph.add_node(Node::Image(image));
            index_map.insert(name, image_index);
        }

        for (src_name, dst_name) in links.iter() {
            let src_index = *index_map
                .get(&(*src_name).to_string())
                .context("Failed to get source node index for a rendergraph link!")?;
            let dst_index = *index_map
                .get(&(*dst_name).to_string())
                .context("Failed to get destination node index for a rendergraph link!")?;
            graph.add_edge(src_index, dst_index, ());
        }

        Ok(Self {
            graph,
            ..Default::default()
        })
    }

    pub fn build(&mut self, device: Arc<Device>, allocator: Arc<Allocator>) -> Result<()> {
        self.process_images(device.clone(), allocator)?;
        self.process_passes(device.clone())?;

        let default_sampler = create_default_sampler(device)?;
        self.samplers.insert("default".to_string(), default_sampler);

        Ok(())
    }

    fn process_images(&mut self, device: Arc<Device>, allocator: Arc<Allocator>) -> Result<()> {
        for index in self.graph.node_indices() {
            if let Node::Image(image_node) = &self.graph[index] {
                let allocation_result =
                    Self::allocate_image(image_node, device.clone(), allocator.clone())?;
                if let Some((image, image_view)) = allocation_result {
                    self.images
                        .insert(image_node.name.to_string(), Box::new(image));
                    self.image_views
                        .insert(image_node.name.to_string(), image_view);
                }
            }
        }
        Ok(())
    }

    fn process_passes(&mut self, device: Arc<Device>) -> Result<()> {
        for index in self.graph.node_indices() {
            if let Node::Pass(pass_node) = &self.graph[index] {
                let pass = self.create_pass(index, device.clone())?;
                if !pass.presents_to_backbuffer {
                    let attachments = self.framebuffer_attachments(index)?;
                    let framebuffer = pass.create_framebuffer(device.clone(), &attachments)?;
                    self.framebuffers
                        .insert(pass_node.name.to_string(), framebuffer);
                }
                self.passes.insert(pass_node.name.to_string(), pass);
            }
        }
        Ok(())
    }

    fn framebuffer_attachments(&self, index: NodeIndex) -> Result<Vec<vk::ImageView>> {
        let mut attachments = Vec::new();
        for child_index in self.child_node_indices(index)?.into_iter() {
            if let Node::Image(image_node) = &self.graph[child_index] {
                if image_node.is_backbuffer() {
                    continue;
                }
                let error_message =
                    format!("Failed to get an image view with the name '{}' to use as a framebuffer attachment", image_node.name);
                let handle = self
                    .image_views
                    .get(&image_node.name)
                    .context(error_message)?
                    .handle;
                attachments.push(handle);
            }
        }
        Ok(attachments)
    }

    pub fn insert_backbuffer_images(
        &mut self,
        device: Arc<Device>,
        images: Vec<Box<dyn Image>>,
    ) -> Result<()> {
        let backbuffer_node_index = self
            .backbuffer_node()
            .context("Failed to find backbuffer node when inserting backbuffer images")?;

        for (index, image) in images.into_iter().enumerate() {
            let view = {
                match &self.graph[backbuffer_node_index] {
                    Node::Image(image_node) => {
                        image_node.create_image_view(device.clone(), image.handle())
                    }
                    _ => bail!("Backbuffer node is not an Image node"),
                }
            }?;

            let (final_pass_node, final_pass_index) = self.final_pass_node()?;
            let mut attachments = vec![view.handle];
            attachments.extend_from_slice(&self.framebuffer_attachments(final_pass_index)?);
            let final_pass = self.pass(&final_pass_node.name)?;
            let framebuffer = final_pass.create_framebuffer(device.clone(), &attachments)?;

            let key = format!("{} {}", Self::BACKBUFFER_PREFIX, index);
            self.images.insert(key.clone(), image);
            self.image_views.insert(key.clone(), view);
            self.framebuffers.insert(key, framebuffer);
        }

        Ok(())
    }

    fn backbuffer_node(&self) -> Option<NodeIndex> {
        for index in self.graph.node_indices().into_iter() {
            if let Node::Image(image_node) = &self.graph[index] {
                if image_node.is_backbuffer() {
                    return Some(index);
                }
            }
        }
        None
    }

    pub fn execute_pass(
        &self,
        command_buffer: vk::CommandBuffer,
        name: &str,
        backbuffer_image_index: usize,
        action: impl Fn(&Pass, vk::CommandBuffer) -> Result<()>,
    ) -> Result<()> {
        let pass = self.pass(name)?;
        let framebuffer = if pass.presents_to_backbuffer {
            self.framebuffer(&format!("backbuffer {}", backbuffer_image_index))
        } else {
            self.framebuffer(name)
        }?;
        pass.execute(command_buffer, framebuffer.handle, |command_buffer| {
            action(pass, command_buffer)
        })
    }

    pub fn pass(&self, name: &str) -> Result<&Pass> {
        let error_message = format!(
            "Attempted to access renderpass with the key '{}' that was not found in the rendergraph",
            name
        );
        self.passes.get(name).context(error_message)
    }

    pub fn pass_handle(&self, name: &str) -> Result<Arc<RenderPass>> {
        Ok(self.pass(name)?.render_pass.clone())
    }

    pub fn framebuffer(&self, name: &str) -> Result<&Framebuffer> {
        let error_message = format!(
            "Attempted to access framebuffer with the key '{}' that was not found in the rendergraph",
            name
        );
        self.framebuffers.get(name).context(error_message)
    }

    #[allow(dead_code)]
    pub fn image(&self, name: &str) -> Result<&Box<dyn Image>> {
        let error_message = format!(
            "Attempted to access image with the key '{}' that was not found in the rendergraph",
            name
        );
        self.images.get(name).context(error_message)
    }

    pub fn image_view(&self, name: &str) -> Result<&ImageView> {
        let error_message = format!(
            "Attempted to access image view with the key '{}' that was not found in the rendergraph",
            name
        );
        self.image_views.get(name).context(error_message)
    }

    pub fn sampler(&self, name: &str) -> Result<&Sampler> {
        let error_message = format!(
            "Attempted to access sampler with the key '{}' that was not found in the rendergraph",
            name
        );
        self.samplers.get(name).context(error_message)
    }

    fn final_pass_node(&self) -> Result<(&PassNode, NodeIndex)> {
        for index in self.graph.node_indices() {
            if let Node::Pass(pass) = &self.graph[index] {
                let mut outgoing_walker = self.graph.neighbors_directed(index, Outgoing).detach();
                while let Some(index) = outgoing_walker.next_node(&self.graph) {
                    if let Node::Image(image_node) = &self.graph[index] {
                        if image_node.is_backbuffer() {
                            return Ok((pass, index));
                        }
                    }
                }
            }
        }
        bail!("No pass in the rendergraph writes to the backbuffer!")
    }

    fn create_pass(&self, index: NodeIndex, device: Arc<Device>) -> Result<Pass> {
        let should_clear = self.parent_node_indices(index)?.is_empty();
        let mut pass_builder = PassBuilder::default();
        for child_index in self.child_node_indices(index)?.into_iter() {
            match &self.graph[child_index] {
                Node::Image(image_node) => {
                    let has_children = !self.child_node_indices(child_index)?.is_empty();
                    let attachment_description = image_node.attachment_description(
                        should_clear,
                        has_children,
                        image_node.force_store,
                    )?;
                    pass_builder.add_output_image(image_node, attachment_description)?;
                }
                _ => bail!("A pass cannot have another pass as an output!"),
            }
        }
        pass_builder.build(device)
    }

    fn allocate_image(
        image_node: &ImageNode,
        device: Arc<Device>,
        allocator: Arc<Allocator>,
    ) -> Result<Option<(AllocatedImage, ImageView)>> {
        if image_node.is_backbuffer() {
            // The backbuffer image, imageview, and framebuffer must be injected into the rendergraph
            return Ok(None);
        }

        let allocated_image = image_node.allocate_image(allocator)?;
        let image_view = image_node.create_image_view(device, allocated_image.handle())?;

        Ok(Some((allocated_image, image_view)))
    }

    fn parent_node_indices(&self, index: NodeIndex) -> Result<Vec<NodeIndex>> {
        let mut incoming_walker = self.graph.neighbors_directed(index, Incoming).detach();
        let mut indices = Vec::new();
        while let Some(index) = incoming_walker.next_node(&self.graph) {
            indices.push(index);
        }
        Ok(indices)
    }

    fn child_node_indices(&self, index: NodeIndex) -> Result<Vec<NodeIndex>> {
        let mut outgoing_walker = self.graph.neighbors_directed(index, Outgoing).detach();
        let mut indices = Vec::new();
        while let Some(index) = outgoing_walker.next_node(&self.graph) {
            indices.push(index);
        }
        Ok(indices)
    }
}

pub enum Node {
    Pass(PassNode),
    Image(ImageNode),
}

impl fmt::Debug for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            Self::Pass(pass) => write!(f, "{}", pass.name),
            Self::Image(image) => write!(f, "{}", image.name),
        }
    }
}

pub struct PassNode {
    pub name: String,
    pub bindpoint: vk::PipelineBindPoint,
}

impl PassNode {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            bindpoint: vk::PipelineBindPoint::GRAPHICS,
        }
    }
}

pub struct ImageNode {
    pub name: String,
    pub extent: vk::Extent2D,
    pub format: vk::Format,
    pub clear_value: vk::ClearValue,
    pub samples: vk::SampleCountFlags,
    pub force_store: bool,
}

impl ImageNode {
    pub fn is_resolve(&self) -> bool {
        self.name.ends_with(RenderGraph::RESOLVE_SUFFIX)
    }

    pub fn is_depth_stencil(&self) -> bool {
        self.name == RenderGraph::DEPTH_STENCIL
    }

    pub fn is_backbuffer(&self) -> bool {
        self.name.starts_with(RenderGraph::BACKBUFFER_PREFIX)
    }

    fn layout(&self) -> vk::ImageLayout {
        if self.is_depth_stencil() {
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
        } else {
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        }
    }

    fn attachment_description(
        &self,
        should_clear: bool,
        has_children: bool,
        force_store: bool,
    ) -> Result<vk::AttachmentDescription> {
        let load_op = if should_clear {
            vk::AttachmentLoadOp::CLEAR
        } else {
            vk::AttachmentLoadOp::DONT_CARE
        };
        let mut store_op = vk::AttachmentStoreOp::DONT_CARE;
        let mut final_layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;

        if force_store || has_children || self.is_backbuffer() {
            store_op = vk::AttachmentStoreOp::STORE;
        }

        if has_children {
            final_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        }

        if self.is_backbuffer() {
            final_layout = vk::ImageLayout::PRESENT_SRC_KHR;
        }

        if self.is_depth_stencil() {
            final_layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
        }

        let attachment_description = vk::AttachmentDescription::builder()
            .format(self.format)
            .samples(self.samples)
            .load_op(load_op)
            .store_op(store_op)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(final_layout)
            .build();

        Ok(attachment_description)
    }

    pub fn attachment_reference(&self, offset: u32) -> vk::AttachmentReference {
        vk::AttachmentReference::builder()
            .attachment(offset)
            .layout(self.layout())
            .build()
    }

    pub fn allocate_image(&self, allocator: Arc<Allocator>) -> Result<AllocatedImage> {
        let extent = vk::Extent3D::builder()
            .width(self.extent.width)
            .height(self.extent.height)
            .depth(1)
            .build();

        let create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .format(self.format)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(self.usage())
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(self.samples)
            .flags(vk::ImageCreateFlags::empty());

        let allocation_create_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::GpuOnly,
            ..Default::default()
        };

        AllocatedImage::new(allocator, &allocation_create_info, &create_info)
    }

    fn usage(&self) -> vk::ImageUsageFlags {
        let mut usage = vk::ImageUsageFlags::COLOR_ATTACHMENT;

        if !self.is_backbuffer() {
            usage |= vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC;
        }

        if self.is_depth_stencil() {
            usage = vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
        }

        usage
    }

    #[allow(dead_code)]
    fn mip_levels(&self) -> u32 {
        let shortest_side = self.extent.width.min(self.extent.height);
        1 + (shortest_side as f32).log2().floor() as u32
    }

    pub fn create_image_view(&self, device: Arc<Device>, image: vk::Image) -> Result<ImageView> {
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(self.aspect_mask())
            .level_count(1)
            .layer_count(1)
            .build();

        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(self.format)
            .components(vk::ComponentMapping::default())
            .subresource_range(subresource_range);

        ImageView::new(device, create_info)
    }

    fn aspect_mask(&self) -> vk::ImageAspectFlags {
        if self.is_depth_stencil() {
            vk::ImageAspectFlags::DEPTH
        } else {
            vk::ImageAspectFlags::COLOR
        }
    }
}

pub struct Pass {
    pub render_pass: Arc<RenderPass>,
    pub presents_to_backbuffer: bool,
    pub extent: vk::Extent2D,
    pub clear_values: Vec<vk::ClearValue>,
}

impl Pass {
    pub fn execute(
        &self,
        command_buffer: vk::CommandBuffer,
        framebuffer: vk::Framebuffer,
        action: impl Fn(vk::CommandBuffer) -> Result<()>,
    ) -> Result<()> {
        let render_area = vk::Rect2D::builder().extent(self.extent).build();
        let begin_info = vk::RenderPassBeginInfo::builder()
            .render_pass(self.render_pass.handle)
            .framebuffer(framebuffer)
            .render_area(render_area)
            .clear_values(&self.clear_values);
        self.render_pass
            .record(command_buffer, begin_info, action)?;
        Ok(())
    }

    fn create_framebuffer(
        &self,
        device: Arc<Device>,
        attachments: &[vk::ImageView],
    ) -> Result<Framebuffer> {
        let create_info = vk::FramebufferCreateInfo::builder()
            .render_pass(self.render_pass.handle)
            .attachments(attachments)
            .width(self.extent.width)
            .height(self.extent.height)
            .layers(1);
        Framebuffer::new(device, create_info)
    }
}

#[derive(Default)]
pub struct PassBuilder {
    pub attachment_descriptions: Vec<vk::AttachmentDescription>,
    pub color_attachments: Vec<vk::AttachmentReference>,
    pub depth_stencil_attachment: Option<vk::AttachmentReference>,
    pub resolve_attachments: Vec<vk::AttachmentReference>,
    pub clear_values: Vec<vk::ClearValue>,
    pub extents: Vec<vk::Extent2D>,
    pub bindpoint: vk::PipelineBindPoint,
    pub presents_to_backbuffer: bool,
}

impl PassBuilder {
    pub fn add_output_image(
        &mut self,
        image: &ImageNode,
        attachment_description: vk::AttachmentDescription,
    ) -> Result<()> {
        self.attachment_descriptions.push(attachment_description);
        self.add_attachment(image)?;
        self.clear_values.push(image.clear_value);
        self.extents.push(image.extent);
        if image.is_backbuffer() {
            self.presents_to_backbuffer = true;
        }
        Ok(())
    }

    fn attachment_offset(&self) -> usize {
        let number_of_color_attachments = self.color_attachments.iter().count();
        let number_of_resolve_attachments = self.resolve_attachments.iter().count();
        let has_depth_attachment = self.depth_stencil_attachment.is_some();
        number_of_color_attachments
            + number_of_resolve_attachments
            + if has_depth_attachment { 1 } else { 0 }
    }

    pub fn add_attachment(&mut self, image: &ImageNode) -> Result<()> {
        let offset = self.attachment_offset();
        let attachment_reference = image.attachment_reference(offset as _);

        if image.is_depth_stencil() {
            ensure!(
                self.depth_stencil_attachment.is_none(),
                "Multiple depth attachments were specified for a single pass!"
            );
            self.depth_stencil_attachment = Some(attachment_reference);
        } else if image.is_resolve() {
            self.resolve_attachments.push(attachment_reference);
        } else {
            self.color_attachments.push(attachment_reference);
        }
        Ok(())
    }

    pub fn build(self, device: Arc<Device>) -> Result<Pass> {
        let mut subpass_description = vk::SubpassDescription::builder()
            .pipeline_bind_point(self.bindpoint)
            .color_attachments(&self.color_attachments);

        if !self.resolve_attachments.is_empty() {
            subpass_description =
                subpass_description.resolve_attachments(&self.resolve_attachments);
        }

        if let Some(depth_stencil_reference) = self.depth_stencil_attachment.as_ref() {
            subpass_description =
                subpass_description.depth_stencil_attachment(depth_stencil_reference);
        }

        let subpass_descriptions = [subpass_description.build()];
        let create_info = vk::RenderPassCreateInfo::builder()
            .attachments(&self.attachment_descriptions)
            .subpasses(&subpass_descriptions);

        let render_pass = Arc::new(RenderPass::new(device, &create_info)?);

        let extent = self.minimum_extent();
        let Self { clear_values, .. } = self;

        Ok(Pass {
            render_pass,
            presents_to_backbuffer: self.presents_to_backbuffer,
            extent,
            clear_values,
        })
    }

    fn minimum_extent(&self) -> vk::Extent2D {
        let minimum_width = self
            .extents
            .iter()
            .map(|extent| extent.width)
            .min()
            .unwrap_or(0);
        let minimum_height = self
            .extents
            .iter()
            .map(|extent| extent.height)
            .min()
            .unwrap_or(0);
        vk::Extent2D::builder()
            .width(minimum_width)
            .height(minimum_height)
            .build()
    }
}

fn create_default_sampler(device: Arc<Device>) -> Result<Sampler> {
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
