use anyhow::{Context, Result};
use nalgebra_glm as glm;
use petgraph::prelude::*;

pub struct Scene {
    name: String,
    nodes: Vec<NodeGraph>,
}

pub type NodeGraph = Graph<Node, ()>;

#[derive(Default)]
pub struct Node {
    name: String,
    transform: glm::Mat4,
    mesh: Option<Mesh>,
}

#[derive(Default)]
pub struct Mesh {
    name: String,
    primitives: Vec<Primitive>,
}

#[derive(Default)]
pub struct Primitive {
    first_index: u32,
    number_of_indices: u32,
}

pub struct Vertex {
    position: glm::Vec3,
}

const DEFAULT_NAME: &'static str = "<Unnamed>";

pub fn load_gltf(path: &str) -> Result<Asset> {
    let (gltf, buffers, textures) = gltf::import(&path)?;
    let mut asset = Asset {
        buffers,
        textures,
        ..Default::default()
    };

    let mut node_graphs = Vec::new();
    for gltf_scene in gltf.scenes() {
        let mut node_graph = NodeGraph::new();
        for gltf_node in gltf_scene.nodes() {
            load_node(&gltf_node, &mut asset, &mut node_graph, NodeIndex::new(0))?;
        }
        node_graphs.push(node_graph);
    }
    Ok(Asset::default())
}

fn load_node(
    gltf_node: &gltf::Node,
    asset: &mut Asset,
    graph: &mut NodeGraph,
    parent_index: NodeIndex,
) -> Result<()> {
    let node = Node {
        name: gltf_node.name().unwrap_or(DEFAULT_NAME).to_string(),
        mesh: load_mesh(gltf_node, asset)?,
        ..Default::default()
    };

    let index = graph.add_node(node);
    if parent_index != index {
        graph.add_edge(parent_index, index, ());
    }

    for child in gltf_node.children() {
        load_node(&child, asset, graph, index)?;
    }

    Ok(())
}

fn load_mesh(gltf_node: &gltf::Node, asset: &mut Asset) -> Result<Option<Mesh>> {
    let stride = std::mem::size_of::<Vertex>();
    let vertex_count = asset.vertices.len();

    match gltf_node.mesh() {
        Some(gltf_mesh) => {
            let mut primitives = Vec::new();
            for gltf_primitive in gltf_mesh.primitives() {
                primitives.push(load_primitive(&gltf_primitive, asset)?);
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

fn load_primitive(gltf_primitive: &gltf::Primitive, asset: &mut Asset) -> Result<Primitive> {
    let Asset {
        buffers, vertices, ..
    } = asset;

    let reader = gltf_primitive.reader(|buffer| Some(&buffers[buffer.index()]));

    reader
        .read_positions()
        .context("Failed to read vertex positions from the model. Vertex positions are required.")?
        .map(glm::Vec3::from)
        .into_iter()
        .for_each(|position| {
            vertices.push(Vertex { position });
        });

    let vertex_count = asset.vertices.len();
    let first_index = asset.indices.len() as u32;
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

    asset.indices.extend_from_slice(&primitive_indices);

    Ok(Primitive {
        first_index,
        number_of_indices,
    })
}

#[derive(Default)]
pub struct Asset {
    buffers: Vec<gltf::buffer::Data>,
    textures: Vec<gltf::image::Data>,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub scenes: Vec<Scene>,
}
