use crate::{adapters::Framebuffer, context::LogicalDevice};
use anyhow::{anyhow, Context, Result};
use ash::vk;
use petgraph::{algo::toposort, dot::Dot, prelude::*};
use std::{collections::HashMap, fmt, sync::Arc};
use vk_mem::Allocator;

pub enum Node {
    Pass(String),
    Image(Image),
}

pub struct Image {
    pub name: String,
    pub extent: vk::Extent2D,
    pub format: vk::Format,
}

pub struct RenderGraph {
    graph: Graph<Node, ()>,
}

impl RenderGraph {
    pub fn new<'a>(
        passes: &[&'a str],
        images: Vec<Image>,
        links: &[(&'a str, &'a str)],
    ) -> Result<Self> {
        let mut graph: Graph<Node, ()> = Graph::new();
        let mut index_map = HashMap::new();

        for pass in passes.iter() {
            let pass_index = graph.add_node(Node::Pass(pass.to_string()));
            index_map.insert(pass.to_string(), pass_index);
        }

        for image in images.into_iter() {
            let name = image.name.to_string();
            let image_index = graph.add_node(Node::Image(image));
            index_map.insert(name, image_index);
        }

        for (src_name, dst_name) in links.iter() {
            let src_index = *index_map
                .get(&src_name.to_string())
                .context("Failed to get source node index for a rendergraph link!")?;
            let dst_index = *index_map
                .get(&dst_name.to_string())
                .context("Failed to get destination node index for a rendergraph link!")?;
            graph.add_edge(src_index, dst_index, ());
        }

        Ok(Self { graph })
    }
}

fn forward_rendergraph() -> Result<()> {
    let offscreen = "offscreen";
    let postprocessing = "postprocessing";
    let color = "color";
    let depth_stencil = "depth stencil";
    let backbuffer = "backbuffer";

    let rendergraph = RenderGraph::new(
        &[offscreen, postprocessing],
        vec![
            Image {
                name: color.to_string(),
                extent: vk::Extent2D::default(),
                format: vk::Format::R8G8B8A8_UNORM,
            },
            Image {
                name: depth_stencil.to_owned(),
                extent: vk::Extent2D::default(),
                format: vk::Format::R8G8B8A8_UNORM,
            },
            Image {
                name: backbuffer.to_owned(),
                extent: vk::Extent2D::default(),
                format: vk::Format::R8G8B8A8_UNORM,
            },
        ],
        &[
            (offscreen, color),
            (offscreen, depth_stencil),
            (color, postprocessing),
            (postprocessing, backbuffer),
        ],
    )?;
    Ok(())
}
