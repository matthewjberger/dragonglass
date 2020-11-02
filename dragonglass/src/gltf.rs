use anyhow::{Context, Result};
use gltf::animation::{util::ReadOutputs, Interpolation};
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
    pub skin: Option<Skin>,
    pub light: Option<Light>,
}

pub struct Light {
    pub name: String,
    pub color: glm::Vec3,
    pub intensity: f32,
    pub range: f32,
    pub kind: gltf::khr_lights_punctual::Kind,
}

pub struct Skin {
    pub name: String,
    pub joints: Vec<Joint>,
}

pub struct Joint {
    pub target_node: usize,
    pub inverse_bind_matrix: glm::Mat4,
}

#[derive(Debug)]
pub struct Mesh {
    pub name: String,
    pub primitives: Vec<Primitive>,
}

#[derive(Debug)]
pub struct Primitive {
    pub first_vertex: usize,
    pub first_index: usize,
    pub number_of_vertices: usize,
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
    pub uv_1: glm::Vec2,
    pub joint_0: glm::Vec4,
    pub weight_0: glm::Vec4,
    pub color_0: glm::Vec3,
}

#[derive(Debug)]
pub struct Animation {
    pub name: String,
    pub time: f32,
    channels: Vec<Channel>,
    max_animation_time: f32,
}

#[derive(Debug)]
pub struct Channel {
    target_node: usize,
    inputs: Vec<f32>,
    transformations: TransformationSet,
    _interpolation: Interpolation,
}

#[derive(Debug)]
pub enum TransformationSet {
    Translations(Vec<glm::Vec3>),
    Rotations(Vec<glm::Vec4>),
    Scales(Vec<glm::Vec3>),
    MorphTargetWeights(Vec<f32>),
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
        graph_node(&child, graph, index);
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
    pub gltf: gltf::Document,
    pub textures: Vec<gltf::image::Data>,
    pub nodes: Vec<Node>,
    pub scenes: Vec<Scene>,
    pub animations: Vec<Animation>,
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
        let animations = Self::load_animations(&gltf, &buffers)?;

        Ok(Self {
            gltf,
            textures,
            nodes,
            scenes,
            animations,
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
                    skin: Self::load_skin(&node, buffers),
                    light: Self::load_light(&node),
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
        let first_vertex = geometry.vertices.len();
        let number_of_indices = Self::load_primitive_indices(primitive, buffers, geometry)?;
        let number_of_vertices = Self::load_primitive_vertices(primitive, buffers, geometry)?;
        Ok(Primitive {
            first_index,
            first_vertex,
            number_of_indices,
            number_of_vertices,
            material_index: primitive.material().index(),
        })
    }

    fn load_primitive_vertices(
        primitive: &gltf::Primitive,
        buffers: &[gltf::buffer::Data],
        geometry: &mut Geometry,
    ) -> Result<usize> {
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
        let uv_1 = reader
            .read_tex_coords(1)
            .map_or(vec![glm::vec2(0.0, 0.0); number_of_vertices], map_to_vec2);

        let convert_joints = |joints: gltf::mesh::util::ReadJoints<'_>| -> Vec<glm::Vec4> {
            joints
                .into_u16()
                .map(|joint| glm::vec4(joint[0] as _, joint[1] as _, joint[2] as _, joint[3] as _))
                .collect::<Vec<_>>()
        };

        let joints_0 = reader.read_joints(0).map_or(
            vec![glm::vec4(0.0, 0.0, 0.0, 0.0); number_of_vertices],
            convert_joints,
        );

        let convert_weights = |weights: gltf::mesh::util::ReadWeights<'_>| -> Vec<glm::Vec4> {
            weights.into_f32().map(glm::Vec4::from).collect::<Vec<_>>()
        };

        let weights_0 = reader.read_weights(0).map_or(
            vec![glm::vec4(1.0, 0.0, 0.0, 0.0); number_of_vertices],
            convert_weights,
        );

        let convert_colors = |colors: gltf::mesh::util::ReadColors<'_>| -> Vec<glm::Vec3> {
            colors
                .into_rgb_f32()
                .map(glm::Vec3::from)
                .collect::<Vec<_>>()
        };

        let colors_0 = reader.read_colors(0).map_or(
            vec![glm::vec3(1.0, 1.0, 1.0); number_of_vertices],
            convert_colors,
        );

        for (index, position) in positions.into_iter().enumerate() {
            geometry.vertices.push(Vertex {
                position,
                normal: normals[index],
                uv_0: uv_0[index],
                uv_1: uv_1[index],
                joint_0: joints_0[index],
                weight_0: weights_0[index],
                color_0: colors_0[index],
            });
        }

        Ok(number_of_vertices)
    }

    fn load_primitive_indices(
        primitive: &gltf::Primitive,
        buffers: &[gltf::buffer::Data],
        geometry: &mut Geometry,
    ) -> Result<usize> {
        let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
        let vertex_count = geometry.vertices.len();
        if let Some(read_indices) = reader.read_indices().take() {
            let indices = read_indices
                .into_u32()
                .map(|x| x + vertex_count as u32)
                .collect::<Vec<_>>();
            let number_of_indices = indices.len();
            geometry.indices.extend_from_slice(&indices);
            Ok(number_of_indices)
        } else {
            Ok(0)
        }
    }

    fn load_animations(
        gltf: &gltf::Document,
        buffers: &[gltf::buffer::Data],
    ) -> Result<Vec<Animation>> {
        let mut animations = Vec::new();
        for animation in gltf.animations() {
            let name = animation.name().unwrap_or(DEFAULT_NAME).to_string();
            let mut channels = Vec::new();
            for channel in animation.channels() {
                let sampler = channel.sampler();
                let _interpolation = sampler.interpolation();
                let target_node = channel.target().node().index();
                let reader = channel.reader(|buffer| Some(&buffers[buffer.index()]));

                let inputs = reader
                    .read_inputs()
                    .context("Failed to read animation channel inputs!")?
                    .collect::<Vec<_>>();

                let outputs = reader
                    .read_outputs()
                    .context("Failed to read animation channel outputs!")?;

                let transformations: TransformationSet;
                match outputs {
                    ReadOutputs::Translations(translations) => {
                        let translations = translations.map(glm::Vec3::from).collect::<Vec<_>>();
                        transformations = TransformationSet::Translations(translations);
                    }
                    ReadOutputs::Rotations(rotations) => {
                        let rotations = rotations
                            .into_f32()
                            .map(glm::Vec4::from)
                            .collect::<Vec<_>>();
                        transformations = TransformationSet::Rotations(rotations);
                    }
                    ReadOutputs::Scales(scales) => {
                        let scales = scales.map(glm::Vec3::from).collect::<Vec<_>>();
                        transformations = TransformationSet::Scales(scales);
                    }
                    ReadOutputs::MorphTargetWeights(weights) => {
                        let morph_target_weights = weights.into_f32().collect::<Vec<_>>();
                        transformations =
                            TransformationSet::MorphTargetWeights(morph_target_weights);
                    }
                }
                channels.push(Channel {
                    target_node,
                    inputs,
                    transformations,
                    _interpolation,
                });
            }

            let max_animation_time = channels
                .iter()
                .flat_map(|channel| channel.inputs.iter().copied())
                .fold(0.0, f32::max);

            animations.push(Animation {
                channels,
                time: 0.0,
                max_animation_time,
                name,
            });
        }
        Ok(animations)
    }

    fn load_skin(node: &gltf::Node, buffers: &[gltf::buffer::Data]) -> Option<Skin> {
        match node.skin() {
            Some(skin) => {
                let reader = skin.reader(|buffer| Some(&buffers[buffer.index()]));

                let inverse_bind_matrices = reader
                    .read_inverse_bind_matrices()
                    .map_or(Vec::new(), |matrices| {
                        matrices.map(glm::Mat4::from).collect::<Vec<_>>()
                    });

                let joints = Self::load_joints(&skin, &inverse_bind_matrices);

                let name = skin.name().unwrap_or(DEFAULT_NAME).to_string();

                Some(Skin { joints, name })
            }
            None => None,
        }
    }

    fn load_joints(skin: &gltf::Skin, inverse_bind_matrices: &[glm::Mat4]) -> Vec<Joint> {
        skin.joints()
            .enumerate()
            .map(|(index, joint_node)| {
                let inverse_bind_matrix = *inverse_bind_matrices
                    .get(index)
                    .unwrap_or(&glm::Mat4::identity());
                Joint {
                    inverse_bind_matrix,
                    target_node: joint_node.index(),
                }
            })
            .collect()
    }

    fn load_light(node: &gltf::Node) -> Option<Light> {
        match node.light() {
            Some(light) => Some(Light {
                name: light.name().unwrap_or(DEFAULT_NAME).to_string(),
                color: glm::make_vec3(&light.color()),
                intensity: light.intensity(),
                range: light.range().unwrap_or(-1.0), // if no range is present, range is assumed to be infinite
                kind: light.kind(),
            }),
            None => None,
        }
    }

    pub fn animate(&mut self, index: usize, step: f32) {
        if self.animations.get(index).is_none() {
            log::warn!("No animation at index: {}. Skipping...", index);
            return;
        }
        let mut animation = &mut self.animations[index];

        animation.time += step;

        // TODO: Allow for specifying a specific animation by name
        if animation.time > animation.max_animation_time {
            animation.time = 0.0;
        }
        if animation.time < 0.0 {
            animation.time = animation.max_animation_time;
        }
        for channel in animation.channels.iter_mut() {
            let mut input_iter = channel.inputs.iter().enumerate().peekable();
            while let Some((previous_key, previous_time)) = input_iter.next() {
                if let Some((next_key, next_time)) = input_iter.peek() {
                    let next_key = *next_key;
                    let next_time = **next_time;
                    let previous_time = *previous_time;

                    if animation.time < previous_time || animation.time > next_time {
                        continue;
                    }

                    let interpolation =
                        (animation.time - previous_time) / (next_time - previous_time);

                    // TODO: Interpolate with other methods
                    // Only Linear interpolation is used for now
                    match &channel.transformations {
                        TransformationSet::Translations(translations) => {
                            let start = translations[previous_key];
                            let end = translations[next_key];
                            let translation_vec = glm::mix(&start, &end, interpolation);
                            self.nodes[channel.target_node].transform =
                                glm::translation(&translation_vec);
                        }
                        TransformationSet::Rotations(rotations) => {
                            let start = rotations[previous_key];
                            let end = rotations[next_key];
                            let start_quat = glm::make_quat(start.as_slice());
                            let end_quat = glm::make_quat(end.as_slice());
                            let rotation_quat =
                                glm::quat_slerp(&start_quat, &end_quat, interpolation);
                            self.nodes[channel.target_node].transform =
                                glm::quat_to_mat4(&rotation_quat); // TODO: maybe normalize??
                        }
                        TransformationSet::Scales(scales) => {
                            let start = scales[previous_key];
                            let end = scales[next_key];
                            let scale_vec = glm::mix(&start, &end, interpolation);
                            self.nodes[channel.target_node].transform = glm::scaling(&scale_vec);
                        }
                        TransformationSet::MorphTargetWeights(_weights) => unimplemented!(),
                    }
                }
            }
        }
    }
}
