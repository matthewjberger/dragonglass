use crate::vulkan::core::{
    CommandPool, Context, DescriptorPool, DescriptorSetLayout, Device, GeometryBuffer,
    GraphicsPipeline, PipelineLayout, RenderPass,
};
use anyhow::Result;
use ash::vk;
use std::{collections::HashMap, mem, sync::Arc};

struct PipelineDescription {
    render_pass: Arc<RenderPass>,
    vertex_shader: Option<String>,
    fragment_shader: Option<String>,
    push_constants: Option<(usize, vk::ShaderStageFlags)>,
    vertex_attributes: Vec<vk::Format>,
    descriptors: Vec<(vk::DescriptorType, usize, vk::ShaderStageFlags)>,
}

impl PipelineDescription {
    pub fn new(render_pass: Arc<RenderPass>) -> Result<Self> {
        Ok(Self {
            render_pass,
            vertex_shader: None,
            fragment_shader: None,
            push_constants: None,
            vertex_attributes: Vec::new(),
            descriptors: Vec::new(),
        })
    }

    pub fn with_vertex_shader(&mut self, path: &str) -> &mut Self {
        self.vertex_shader = Some(path.to_string());
        self
    }

    pub fn with_fragment_shader(&mut self, path: &str) -> &mut Self {
        self.fragment_shader = Some(path.to_string());
        self
    }

    pub fn with_push_constants<T>(&mut self, stage_flags: vk::ShaderStageFlags) -> &mut Self {
        self.push_constants = Some((mem::size_of::<T>(), stage_flags));
        self
    }

    pub fn with_vertex_attributes(&mut self, attributes: Vec<vk::Format>) -> &mut Self {
        self.vertex_attributes = attributes;
        self
    }

    pub fn with_descriptor_layout(
        &mut self,
        descriptors: Vec<(vk::DescriptorType, usize, vk::ShaderStageFlags)>,
    ) -> &mut Self {
        self.descriptors = descriptors;
        self
    }

    pub fn build(self) -> (GraphicsPipeline, PipelineLayout) {
        todo!()
    }
}

pub struct RenderPath {
    pipelines: HashMap<String, (GraphicsPipeline, PipelineLayout)>,
    descriptor_pool: DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    descriptor_set_layout: DescriptorSetLayout,
    geometry: GeometryBuffer,
    device: Arc<Device>,
}

struct RenderPathBuilder<T> {
    vertices: Vec<T>,
    indices: Vec<u32>,
    pipelines: HashMap<String, (GraphicsPipeline, PipelineLayout)>,
}

impl<T: Copy> RenderPathBuilder<T> {
    pub fn new(render_pass: Arc<RenderPass>) -> Result<Self> {
        Ok(Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            pipelines: HashMap::new(),
        })
    }

    pub fn with_vertices(&mut self, vertices: Vec<T>) -> &mut Self {
        self.vertices = vertices;
        self
    }

    pub fn with_indices(&mut self, indices: Vec<u32>) -> &mut Self {
        self.indices = indices;
        self
    }

    pub fn with_pipeline(
        &mut self,
        name: String,
        pipeline: (GraphicsPipeline, PipelineLayout),
    ) -> &mut Self {
        self.pipelines.insert(name, pipeline);
        self
    }

    pub fn build(self, context: &Context, command_pool: &CommandPool) -> RenderPath {
        todo!()
    }
}
