use crate::{
    adapters::{Framebuffer, RenderPass},
    context::LogicalDevice,
};
use anyhow::{anyhow, bail, ensure, Context, Result};
use ash::vk;
use petgraph::{
    algo::toposort,
    dot::{Config, Dot},
    prelude::*,
};
use std::{collections::HashMap, fmt, sync::Arc};
use vk_mem::Allocator;

pub fn forward_rendergraph() -> Result<RenderGraph> {
    let offscreen = "offscreen";
    let postprocessing = "postprocessing";
    let color = "color";
    let depth_stencil = "depth_stencil";
    let backbuffer = "backbuffer";
    let offscreen_extent = vk::Extent2D::builder().width(2048).height(2048).build();
    let swapchain_extent = vk::Extent2D::builder().width(800).height(600).build();
    let swapchain_format = vk::Format::R8G8B8A8_UNORM;
    let rendergraph = RenderGraph::new(
        &[offscreen, postprocessing],
        vec![
            ImageNode {
                name: color.to_string(),
                extent: offscreen_extent,
                format: vk::Format::R8G8B8A8_UNORM,
                clear_value: vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.39, 0.58, 0.93, 1.0], // Cornflower blue
                    },
                },
            },
            ImageNode {
                name: depth_stencil.to_owned(),
                extent: offscreen_extent,
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
                extent: swapchain_extent,
                format: swapchain_format,
                clear_value: vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [1.0, 1.0, 1.0, 1.0],
                    },
                },
            },
        ],
        &[
            (offscreen, color),
            (offscreen, depth_stencil),
            (color, postprocessing),
            (postprocessing, backbuffer),
        ],
    )?;
    Ok(rendergraph)
}

pub struct RenderGraph {
    graph: Graph<Node, ()>,
    passes: HashMap<String, Pass<'static>>,
}

impl RenderGraph {
    pub fn new<'a>(
        passes: &[&'a str],
        images: Vec<ImageNode>,
        links: &[(&'a str, &'a str)],
    ) -> Result<Self> {
        let mut graph: Graph<Node, ()> = Graph::new();
        let mut index_map = HashMap::new();

        for pass in passes.iter() {
            let pass_index = graph.add_node(Node::Pass(PassNode::new(pass)));
            index_map.insert(pass.to_string(), pass_index);
        }

        for image in images.into_iter() {
            let name = image.name.to_string();
            let image_index = graph.add_node(Node::Image(image));
            index_map.insert(name, image_index);
        }

        for (src_name, dst_name) in links.into_iter() {
            let src_index = *index_map
                .get(&src_name.to_string())
                .context("Failed to get source node index for a rendergraph link!")?;
            let dst_index = *index_map
                .get(&dst_name.to_string())
                .context("Failed to get destination node index for a rendergraph link!")?;
            graph.add_edge(src_index, dst_index, ());
        }

        Ok(Self {
            graph,
            passes: HashMap::new(),
        })
    }

    pub fn build(&mut self, device: Arc<LogicalDevice>) -> Result<()> {
        // hash the array of passes
        // check if we already have a cached graph with the same hash
        // if not, construct a new one
        // Graph construction only happens on app init, window resize, shader hot-reload,
        // or if rendering logic changes

        log::info!(
            "Full graph: {:#?}",
            Dot::with_config(&self.graph, &[Config::EdgeNoLabel])
        );

        for index in self.graph.node_indices() {
            match &self.graph[index] {
                Node::Pass(pass_node) => {
                    let pass = self.create_pass(index, device.clone())?;
                    self.passes.insert(pass_node.name.to_string(), pass);
                }
                Node::Image(image_node) => {}
            }
        }

        Ok(())
    }

    pub fn execute(&self) -> Result<()> {
        self.execute_at_index(0)
    }

    pub fn execute_at_index(&self, index: usize) -> Result<()> {
        let indices = toposort(&self.graph, None).map_err(|_| {
            anyhow!("A cycle was detected in the rendergraph. Skipping execution...")
        })?;

        for index in indices.into_iter() {}
        Ok(())
    }

    fn create_pass<'a>(&self, index: NodeIndex, device: Arc<LogicalDevice>) -> Result<Pass<'a>> {
        let should_clear = self.parent_nodes(index)?.is_empty();
        let mut pass_builder = PassBuilder::default();
        for child_index in self.child_nodes(index)?.into_iter() {
            match &self.graph[child_index] {
                Node::Image(image_node) => {
                    let has_children = self.child_nodes(child_index)?.is_empty();
                    let attachment_description =
                        image_node.attachment_description(should_clear, has_children)?;
                    pass_builder.visit_image_node(&image_node, attachment_description)?;
                }
                _ => bail!("A pass cannot have another pass as an output!"),
            }
        }
        pass_builder.build(device.clone())
    }

    fn parent_nodes(&self, index: NodeIndex) -> Result<Vec<NodeIndex>> {
        let mut incoming_walker = self.graph.neighbors_directed(index, Incoming).detach();
        let mut indices = Vec::new();
        while let Some(index) = incoming_walker.next_node(&self.graph) {
            indices.push(index);
        }
        Ok(indices)
    }

    fn child_nodes(&self, index: NodeIndex) -> Result<Vec<NodeIndex>> {
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
}

impl ImageNode {
    pub fn is_depth_stencil(&self) -> bool {
        self.name == "depth_stencil"
    }

    pub fn is_backbuffer(&self) -> bool {
        self.name == "backbuffer"
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
    ) -> Result<vk::AttachmentDescription> {
        let load_op = if should_clear {
            vk::AttachmentLoadOp::CLEAR
        } else {
            vk::AttachmentLoadOp::DONT_CARE
        };
        let mut store_op = vk::AttachmentStoreOp::DONT_CARE;
        let mut final_layout = vk::ImageLayout::UNDEFINED;

        if !has_children {
            store_op = vk::AttachmentStoreOp::STORE;
            final_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        }

        if self.is_backbuffer() {
            store_op = vk::AttachmentStoreOp::STORE;
            final_layout = vk::ImageLayout::PRESENT_SRC_KHR;
        }

        if self.is_depth_stencil() {
            final_layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
        }

        let attachment_description = vk::AttachmentDescription::builder()
            .format(self.format)
            .samples(vk::SampleCountFlags::TYPE_1)
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
}

pub struct Pass<'a> {
    pub render_pass: RenderPass,
    extent: vk::Extent2D,
    clear_values: Vec<vk::ClearValue>,
    callback: Box<dyn Fn(vk::CommandBuffer) -> Result<()> + 'a>,
}

impl<'a> Pass<'a> {
    pub fn set_callback(&mut self, callback: impl Fn(vk::CommandBuffer) -> Result<()> + 'a) {
        self.callback = Box::new(callback);
    }

    fn execute(
        &self,
        command_buffer: vk::CommandBuffer,
        framebuffer: vk::Framebuffer,
    ) -> Result<()> {
        let render_area = vk::Rect2D::builder().extent(self.extent).build();
        let begin_info = vk::RenderPassBeginInfo::builder()
            .render_pass(self.render_pass.handle)
            .framebuffer(framebuffer)
            .render_area(render_area)
            .clear_values(&self.clear_values);
        self.render_pass
            .record(command_buffer, begin_info, &self.callback)?;
        Ok(())
    }
}

#[derive(Default)]
pub struct PassBuilder {
    pub attachment_descriptions: Vec<vk::AttachmentDescription>,
    pub color_references: Vec<vk::AttachmentReference>,
    pub depth_reference: Option<vk::AttachmentReference>,
    pub clear_values: Vec<vk::ClearValue>,
    pub extents: Vec<vk::Extent2D>,
    pub bindpoint: vk::PipelineBindPoint,
}

impl PassBuilder {
    pub fn visit_image_node(
        &mut self,
        image: &ImageNode,
        attachment_description: vk::AttachmentDescription,
    ) -> Result<()> {
        self.attachment_descriptions.push(attachment_description);
        self.add_attachment(&image)?;
        self.clear_values.push(image.clear_value);
        self.extents.push(image.extent);
        Ok(())
    }

    pub fn add_attachment(&mut self, image: &ImageNode) -> Result<()> {
        let mut offset = self.color_references.iter().count() as _;
        if self.depth_reference.is_some() {
            offset += 1;
        }
        let attachment_reference = image.attachment_reference(offset);
        if image.is_depth_stencil() {
            ensure!(
                self.depth_reference.is_none(),
                "Multiple depth attachments were specified for a single pass!"
            );
            self.depth_reference = Some(attachment_reference);
        } else {
            self.color_references.push(attachment_reference);
        }
        Ok(())
    }

    pub fn build<'a>(self, device: Arc<LogicalDevice>) -> Result<Pass<'a>> {
        let subpass_description = vk::SubpassDescription::builder()
            .pipeline_bind_point(self.bindpoint)
            .color_attachments(&self.color_references);
        let subpass_descriptions = [subpass_description.build()];
        let create_info = vk::RenderPassCreateInfo::builder()
            .attachments(&self.attachment_descriptions)
            .subpasses(&subpass_descriptions);
        // TODO: Add subpass dependencies where necessary
        // .dependencies(&subpass_dependencies);

        let render_pass = RenderPass::new(device, &create_info)?;

        let extent = self.minimum_extent();
        let Self { clear_values, .. } = self;

        Ok(Pass {
            render_pass,
            extent,
            clear_values,
            callback: Box::new(|_| Ok(())),
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
