use anyhow::{Context, Result};
use nalgebra_glm as glm;
use petgraph::prelude::*;
use std::path::Path;

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
    pub first_index: usize,
    pub number_of_indices: usize,
    pub material_index: Option<usize>,
}

#[derive(Default)]
pub struct Geometry {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Copy, Clone)]
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

pub fn global_transform(graph: &SceneGraph, index: NodeIndex, nodes: &[Node]) -> glm::Mat4 {
    let transform = nodes[graph[index]].transform;
    let mut incoming_walker = graph.neighbors_directed(index, Incoming).detach();
    match incoming_walker.next_node(graph) {
        Some(parent_index) => global_transform(graph, parent_index, nodes) * transform,
        None => transform,
    }
}

const DEFAULT_NAME: &str = "<Unnamed>";

pub struct Asset {
    gltf: gltf::Document,
    pub textures: Vec<gltf::image::Data>,
    pub nodes: Vec<Node>,
    pub scenes: Vec<Scene>,
    pub geometry: Geometry,
}

impl Asset {
    pub fn new<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let (gltf, buffers, textures) = gltf::import(path)?;

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

    pub fn material_at_index(&self, index: usize) -> Result<gltf::Material> {
        let error_message = format!("Failed to lookup gltf asset material at index: {}", index);
        self.gltf.materials().nth(index).context(error_message)
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
        // Indices must be loaded before vertices in this case
        // because the number of vertices is used to offset indices
        let first_index = geometry.indices.len();
        let number_of_indices = Self::load_primitive_indices(primitive, buffers, geometry)?;
        Self::load_primitive_vertices(primitive, buffers, geometry)?;
        Ok(Primitive {
            first_index,
            number_of_indices,
            material_index: primitive.material().index(),
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
    ) -> Result<usize> {
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

        let number_of_indices = primitive_indices.len();
        geometry.indices.extend_from_slice(&primitive_indices);

        Ok(number_of_indices)
    }
}
