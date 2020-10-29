use crate::{
    adapters::CommandPool,
    context::{Context, Device},
    resources::{AllocatedImage, ImageDescription, ImageView, Sampler},
};
use anyhow::{Context as AshContext, Result};
use ash::vk;
use nalgebra_glm as glm;
use petgraph::{prelude::*, visit::Dfs};
use std::{path::Path, sync::Arc};

pub struct Scene {
    pub name: String,
    pub graphs: Vec<SceneGraph>,
}

pub struct Node {
    pub name: String,
    pub transform: glm::Mat4,
    pub mesh: Option<Mesh>,
}

#[derive(Debug)]
pub struct Mesh {
    pub name: String,
    pub primitives: Vec<Primitive>,
}

#[derive(Debug)]
pub struct Primitive {
    pub first_index: u32,
    pub number_of_indices: u32,
}

#[derive(Default)]
pub struct Geometry {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

pub struct Vertex {
    pub position: glm::Vec3,
    pub normal: glm::Vec3,
    pub uv_0: glm::Vec2,
}

pub type SceneGraph = Graph<usize, ()>;

pub fn create_scene_graph(node: &gltf::Node) -> SceneGraph {
    let mut node_graph = SceneGraph::new();
    graph_node(node, &mut node_graph, NodeIndex::new(0));
    node_graph
}

pub fn graph_node(gltf_node: &gltf::Node, graph: &mut SceneGraph, parent_index: NodeIndex) {
    let index = graph.add_node(gltf_node.index());
    if parent_index != index {
        graph.add_edge(parent_index, index, ());
    }
    for child in gltf_node.children() {
        graph_node(&child, graph, parent_index);
    }
}

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

const DEFAULT_NAME: &str = "<Unnamed>";

pub struct Asset {
    gltf: gltf::Document,
    textures: Vec<Texture>,
    nodes: Vec<Node>,
    scenes: Vec<Scene>,
    geometry: Geometry,
}

impl Asset {
    pub fn new<P>(context: &Context, command_pool: &CommandPool, path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let (gltf, buffers, textures) = gltf::import(path)?;

        let textures = Self::load_textures(context, command_pool, &textures)?;
        let (nodes, geometry) = Self::load_nodes(&gltf, &buffers)?;
        let scenes = Self::load_scenes(&gltf);

        Ok(Self {
            gltf,
            textures,
            nodes,
            scenes,
            geometry,
        })
    }

    pub fn traverse(&self) {
        for scene in self.scenes.iter() {
            log::info!("Dfs Scene Traversal: {}", scene.name);
            for graph in scene.graphs.iter() {
                let mut dfs = Dfs::new(graph, NodeIndex::new(0));
                while let Some(node_index) = dfs.next(&graph) {
                    log::info!("Node gltf index: {}", &graph[node_index]);
                    let node = &self.nodes[graph[node_index]];
                    if let Some(mesh) = node.mesh.as_ref() {
                        log::info!("Found mesh: {}", mesh.name);
                        for primitive in mesh.primitives.iter() {
                            log::info!("Found primitive: {:#?}", primitive);
                        }
                    }
                }
            }
        }
    }

    fn load_textures(
        context: &Context,
        command_pool: &CommandPool,
        textures: &[gltf::image::Data],
    ) -> Result<Vec<Texture>> {
        textures
            .iter()
            .map(|texture| {
                let description = ImageDescription::from_gltf(&texture)?;
                Texture::new(context, command_pool, &description)
            })
            .collect::<Result<Vec<_>>>()
    }

    fn load_scenes(gltf: &gltf::Document) -> Vec<Scene> {
        gltf.scenes()
            .map(|scene| Scene {
                name: scene.name().unwrap_or(DEFAULT_NAME).to_string(),
                graphs: scene
                    .nodes()
                    .map(|node| create_scene_graph(&node))
                    .collect(),
            })
            .collect::<Vec<_>>()
    }

    fn load_nodes(
        gltf: &gltf::Document,
        buffers: &[gltf::buffer::Data],
    ) -> Result<(Vec<Node>, Geometry)> {
        let mut geometry = Geometry::default();
        let nodes = gltf
            .nodes()
            .map(|node| {
                Ok(Node {
                    name: node.name().unwrap_or(DEFAULT_NAME).to_string(),
                    transform: node_transform(&node),
                    mesh: Self::load_mesh(&node, buffers, &mut geometry)?,
                })
            })
            .collect::<Result<_>>()?;
        Ok((nodes, geometry))
    }

    fn load_mesh(
        node: &gltf::Node,
        buffers: &[gltf::buffer::Data],
        geometry: &mut Geometry,
    ) -> Result<Option<Mesh>> {
        match node.mesh() {
            Some(mesh) => {
                let primitives = mesh
                    .primitives()
                    .map(|primitive| Self::load_primitive(&primitive, buffers, geometry))
                    .collect::<Result<Vec<_>>>()?;
                Ok(Some(Mesh {
                    name: mesh.name().unwrap_or(DEFAULT_NAME).to_string(),
                    primitives,
                }))
            }
            None => Ok(None),
        }
    }

    fn load_primitive(
        primitive: &gltf::Primitive,
        buffers: &[gltf::buffer::Data],
        geometry: &mut Geometry,
    ) -> Result<Primitive> {
        Self::load_primitive_vertices(primitive, buffers, geometry)?;
        let first_index = geometry.indices.len() as u32;
        let number_of_indices = Self::load_primitive_indices(primitive, buffers, geometry)?;
        Ok(Primitive {
            first_index,
            number_of_indices,
        })
    }

    fn load_primitive_vertices(
        primitive: &gltf::Primitive,
        buffers: &[gltf::buffer::Data],
        geometry: &mut Geometry,
    ) -> Result<()> {
        let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

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
            geometry.vertices.push(Vertex {
                position,
                normal: normals[index],
                uv_0: uv_0[index],
            });
        }

        Ok(())
    }

    fn load_primitive_indices(
        primitive: &gltf::Primitive,
        buffers: &[gltf::buffer::Data],
        geometry: &mut Geometry,
    ) -> Result<u32> {
        let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

        let vertex_count = geometry.vertices.len();
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

        geometry.indices.extend_from_slice(&primitive_indices);

        Ok(number_of_indices)
    }
}

pub struct Texture {
    pub image: AllocatedImage,
    pub view: ImageView,
    pub sampler: Sampler, // TODO: Use samplers specified in file
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
