use crate::{
    core::LogicalDevice,
    forward::ForwardSwapchain,
    render::{AllocatedImage, Framebuffer, Image, ImageView, RenderPass, Sampler, Swapchain},
};
use anyhow::{anyhow, bail, ensure, Context as AnyhowContext, Result};
use ash::vk;
use derive_builder::Builder;
use petgraph::{algo::toposort, dot::Dot, prelude::*};
use std::{collections::HashMap, fmt, sync::Arc};
use vk_mem::Allocator;

pub struct RenderGraph {
    pub graph: Graph<Node, ()>,
    // pub passes: HashMap<String, Pass<'static>>,
    // pub images: HashMap<String, ImageResource>,
    pub framebuffers: HashMap<String, Framebuffer>,
    device: Arc<LogicalDevice>,
    allocator: Arc<Allocator>,
}

pub enum Node {
    /// Represent execution of shaders with a given pipeline state
    /// Passes consume and produce resources
    Pass(String),

    /// TODO: Support buffers as well
    Image(ImageResourceDescription),
}

impl fmt::Debug for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            Self::Pass(pass_name) => write!(f, "{}", pass_name),
            Self::Image(description) => write!(f, "{}", description.name),
        }
    }
}
