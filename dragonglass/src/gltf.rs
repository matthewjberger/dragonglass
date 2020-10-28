use crate::{
    adapters::CommandPool,
    context::{Context, Device},
    resources::{AllocatedImage, ImageDescription, ImageView, Sampler},
};
use anyhow::{Context as AshContext, Result};
use ash::vk;
use nalgebra_glm as glm;
use petgraph::prelude::*;
use std::sync::Arc;

pub struct Scene {
    pub name: String,
    pub node_graphs: Vec<NodeGraph>,
}

pub type NodeGraph = Graph<Node, ()>;

#[derive(Default)]
pub struct Node {
    pub name: String,
    pub transform: glm::Mat4,
    pub mesh: Option<Mesh>,
}

#[derive(Default)]
pub struct Mesh {
    pub name: String,
    pub primitives: Vec<Primitive>,
}

#[derive(Default)]
pub struct Primitive {
    pub first_index: u32,
    pub number_of_indices: u32,
}

pub struct Vertex {
    pub position: glm::Vec3,
    pub normal: glm::Vec3,
    pub uv_0: glm::Vec2,
}

const DEFAULT_NAME: &str = "<Unnamed>";

fn node_transform(gltf_node: &gltf::Node) -> glm::Mat4 {
    let transform = gltf_node
        .transform()
        .matrix()
        .iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    glm::make_mat4x4(&transform)
}

pub fn global_transform(graph: &NodeGraph, index: NodeIndex) -> glm::Mat4 {
    let transform = graph[index].transform;
    let mut incoming_walker = graph.neighbors_directed(index, Incoming).detach();
    match incoming_walker.next_node(graph) {
        Some(parent_index) => transform * global_transform(graph, parent_index),
        None => transform,
    }
}

#[derive(Default)]
pub struct Asset {
    buffers: Vec<gltf::buffer::Data>,
    pub textures: Vec<Texture>,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub scenes: Vec<Scene>,
}

impl Asset {
    pub fn new(context: &Context, command_pool: &CommandPool, path: &str) -> Result<Self> {
        let (gltf, buffers, textures) = gltf::import(&path)?;

        let textures = textures
            .into_iter()
            .map(|texture| {
                let description = ImageDescription::from_gltf(&texture);
                Texture::new(context, command_pool, &description)
            })
            .collect::<Result<Vec<_>>>()?;

        let mut asset = Self {
            buffers,
            textures,
            ..Default::default()
        };

        for gltf_scene in gltf.scenes() {
            asset.load_scene(&gltf_scene)?;
        }

        Ok(asset)
    }

    fn load_scene(&mut self, gltf_scene: &gltf::Scene) -> Result<()> {
        let mut node_graphs = Vec::new();
        for gltf_node in gltf_scene.nodes() {
            let mut node_graph = NodeGraph::new();
            self.load_node(&gltf_node, &mut node_graph, NodeIndex::new(0))?;
            node_graphs.push(node_graph);
        }

        let scene = Scene {
            name: gltf_scene.name().unwrap_or(DEFAULT_NAME).to_string(),
            node_graphs,
        };
        self.scenes.push(scene);
        Ok(())
    }

    fn load_node(
        &mut self,
        gltf_node: &gltf::Node,
        graph: &mut NodeGraph,
        parent_index: NodeIndex,
    ) -> Result<()> {
        let node = Node {
            name: gltf_node.name().unwrap_or(DEFAULT_NAME).to_string(),
            transform: node_transform(gltf_node),
            mesh: self.load_mesh(gltf_node)?,
        };

        let index = graph.add_node(node);
        if parent_index != index {
            graph.add_edge(parent_index, index, ());
        }

        for child in gltf_node.children() {
            self.load_node(&child, graph, index)?;
        }

        Ok(())
    }

    fn load_mesh(&mut self, gltf_node: &gltf::Node) -> Result<Option<Mesh>> {
        match gltf_node.mesh() {
            Some(gltf_mesh) => {
                let mut primitives = Vec::new();
                for gltf_primitive in gltf_mesh.primitives() {
                    primitives.push(self.load_primitive(&gltf_primitive)?);
                }

                let mesh = Mesh {
                    name: gltf_mesh.name().unwrap_or(DEFAULT_NAME).to_string(),
                    primitives,
                };

                Ok(Some(mesh))
            }
            None => Ok(None),
        }
    }

    fn load_primitive(&mut self, gltf_primitive: &gltf::Primitive) -> Result<Primitive> {
        self.load_primitive_vertices(gltf_primitive)?;
        let first_index = self.indices.len() as u32;
        let number_of_indices = self.load_primitive_indices(gltf_primitive)?;
        Ok(Primitive {
            first_index,
            number_of_indices,
        })
    }

    fn load_primitive_vertices(&mut self, gltf_primitive: &gltf::Primitive) -> Result<()> {
        let Self {
            buffers, vertices, ..
        } = self;

        let reader = gltf_primitive.reader(|buffer| Some(&buffers[buffer.index()]));

        let positions = reader
            .read_positions()
            .context(
                "Failed to read vertex positions from the model. Vertex positions are required.",
            )?
            .map(glm::Vec3::from);
        let number_of_vertices = positions.len();

        let normals = reader.read_normals().map_or(
            vec![glm::vec3(0.0, 0.0, 0.0); number_of_vertices],
            |normals| normals.map(glm::Vec3::from).collect::<Vec<_>>(),
        );

        let map_to_vec2 = |coords: gltf::mesh::util::ReadTexCoords<'_>| -> Vec<glm::Vec2> {
            coords.into_f32().map(glm::Vec2::from).collect::<Vec<_>>()
        };
        let uv_0 = reader
            .read_tex_coords(0)
            .map_or(vec![glm::vec2(0.0, 0.0); number_of_vertices], map_to_vec2);

        for (index, position) in positions.into_iter().enumerate() {
            vertices.push(Vertex {
                position,
                normal: normals[index],
                uv_0: uv_0[index],
            });
        }

        Ok(())
    }

    fn load_primitive_indices(&mut self, gltf_primitive: &gltf::Primitive) -> Result<u32> {
        let Self { buffers, .. } = self;

        let reader = gltf_primitive.reader(|buffer| Some(&buffers[buffer.index()]));

        let vertex_count = self.vertices.len();
        let primitive_indices = reader
            .read_indices()
            .map(|indices| {
                indices
                    .into_u32()
                    .map(|x| x + vertex_count as u32)
                    .collect::<Vec<_>>()
            })
            .context("Failed to read indices!")?;

        let number_of_indices = primitive_indices.len() as u32;
        self.indices.extend_from_slice(&primitive_indices);

        Ok(number_of_indices)
    }
}

pub struct Texture {
    pub image: AllocatedImage,
    pub view: ImageView,
    pub sampler: Sampler,
}

impl Texture {
    pub fn new(
        context: &Context,
        command_pool: &CommandPool,
        description: &ImageDescription,
    ) -> Result<Self> {
        let image = description.as_image(context.allocator.clone())?;
        image.upload_data(context, command_pool, description)?;
        let view = Self::image_view(context.device.clone(), &image, description)?;
        let sampler = Self::sampler(context.device.clone(), description.mip_levels)?;
        let texture = Self {
            image,
            view,
            sampler,
        };
        Ok(texture)
    }

    fn image_view(
        device: Arc<Device>,
        image: &AllocatedImage,
        description: &ImageDescription,
    ) -> Result<ImageView> {
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(1)
            .level_count(description.mip_levels);

        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image.handle)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(description.format)
            .components(vk::ComponentMapping::default())
            .subresource_range(subresource_range.build());

        ImageView::new(device, create_info)
    }

    fn sampler(device: Arc<Device>, mip_levels: u32) -> Result<Sampler> {
        let sampler_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(true)
            .max_anisotropy(16.0)
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .compare_op(vk::CompareOp::ALWAYS)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(mip_levels as _);
        Sampler::new(device, sampler_info)
    }
}
