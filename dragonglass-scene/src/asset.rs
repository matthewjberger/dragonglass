use anyhow::{Context, Result};
use nalgebra_glm as glm;
use ncollide3d::{bounding_volume::AABB, na::Point3};
use petgraph::prelude::*;
use serde::{Deserialize, Serialize};
use std::ops::{Index, IndexMut};

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Scene {
    pub name: String,
    pub graphs: Vec<SceneGraph>,
}

impl Scene {
    // TODO: Add a unit test for this
    pub fn scene_aabb(&self, nodes: &[Node]) -> Result<AABB<f32>> {
        let mut aabb: AABB<f32> = AABB::new(Point3::origin(), Point3::origin());
        for graph in self.graphs.iter() {
            graph.walk(|node_index| {
                let index = graph[node_index];
                let node = &nodes[index];
                if let Some(mesh) = node.mesh.as_ref() {
                    for primitive in mesh.primitives.iter() {
                        let bounding_box = &primitive.aabb;
                        aabb.take_point(bounding_box.mins);
                        aabb.take_point(bounding_box.maxs);
                    }
                }
                Ok(())
            })?;
        }
        Ok(aabb)
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub transform: Transform,
    pub camera: Option<Camera>,
    pub mesh: Option<Mesh>,
    pub skin: Option<Skin>,
    pub light: Option<Light>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Transform {
    pub translation: glm::Vec3,
    pub rotation: glm::Quat,
    pub scale: glm::Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: glm::vec3(0.0, 0.0, 0.0),
            rotation: glm::Quat::identity(),
            scale: glm::vec3(1.0, 1.0, 1.0),
        }
    }
}

impl Transform {
    pub fn new(translation: glm::Vec3, rotation: glm::Quat, scale: glm::Vec3) -> Self {
        Self {
            translation,
            rotation,
            scale,
        }
    }

    pub fn matrix(&self) -> glm::Mat4 {
        glm::translation(&self.translation)
            * glm::quat_to_mat4(&self.rotation)
            * glm::scaling(&self.scale)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Light {
    pub name: String,
    pub color: glm::Vec3,
    pub intensity: f32,
    pub range: f32,
    pub kind: LightKind,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum LightKind {
    Directional,
    Point,
    Spot {
        inner_cone_angle: f32,
        outer_cone_angle: f32,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Camera {
    pub name: String,
    pub projection: Projection,
}

impl Camera {
    fn matrix(&self, viewport_aspect_ratio: f32) -> glm::Mat4 {
        match &self.projection {
            Projection::Perspective(camera) => camera.matrix(viewport_aspect_ratio),
            Projection::Orthographic(camera) => camera.matrix(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Projection {
    Perspective(PerspectiveCamera),
    Orthographic(OrthographicCamera),
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct PerspectiveCamera {
    pub aspect_ratio: Option<f32>,
    pub y_fov_deg: f32,
    pub z_far: Option<f32>,
    pub z_near: f32,
}

impl PerspectiveCamera {
    fn matrix(&self, viewport_aspect_ratio: f32) -> glm::Mat4 {
        let aspect_ratio = if let Some(aspect_ratio) = self.aspect_ratio {
            aspect_ratio
        } else {
            viewport_aspect_ratio
        };

        if let Some(z_far) = self.z_far {
            let fov = self.y_fov_deg.to_radians();
            glm::perspective_zo(aspect_ratio, fov, z_far, self.z_near)
        } else {
            glm::infinite_perspective_rh_zo(aspect_ratio, self.y_fov_deg.to_radians(), self.z_near)
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct OrthographicCamera {
    pub x_mag: f32,
    pub y_mag: f32,
    pub z_far: f32,
    pub z_near: f32,
}

impl OrthographicCamera {
    pub fn matrix(&self) -> glm::Mat4 {
        let z_sum = self.z_near + self.z_far;
        let z_diff = self.z_near - self.z_far;
        glm::Mat4::new(
            1.0 / self.x_mag,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0 / self.y_mag,
            0.0,
            0.0,
            0.0,
            0.0,
            2.0 / z_diff,
            0.0,
            0.0,
            0.0,
            z_sum / z_diff,
            1.0,
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Skin {
    pub name: String,
    pub joints: Vec<Joint>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Joint {
    pub target_node: usize,
    pub inverse_bind_matrix: glm::Mat4,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Mesh {
    pub name: String,
    pub primitives: Vec<Primitive>,
    pub weights: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Primitive {
    pub first_vertex: usize,
    pub first_index: usize,
    pub number_of_vertices: usize,
    pub number_of_indices: usize,
    pub material_index: Option<usize>,
    pub morph_targets: Vec<MorphTarget>,
    pub aabb: AABB<f32>,
}

impl Default for Primitive {
    fn default() -> Self {
        Self {
            first_vertex: 0,
            first_index: 0,
            number_of_vertices: 0,
            number_of_indices: 0,
            material_index: None,
            morph_targets: Vec::new(),
            aabb: AABB::new_invalid(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MorphTarget {
    pub positions: Vec<glm::Vec4>,
    pub normals: Vec<glm::Vec4>,
    pub tangents: Vec<glm::Vec4>,
}

impl MorphTarget {
    pub fn total_length(&self) -> usize {
        self.positions.len() + self.normals.len() + self.tangents.len()
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Geometry {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct Vertex {
    pub position: glm::Vec3,
    pub normal: glm::Vec3,
    pub uv_0: glm::Vec2,
    pub uv_1: glm::Vec2,
    pub joint_0: glm::Vec4,
    pub weight_0: glm::Vec4,
    pub color_0: glm::Vec3,
}

impl Default for Vertex {
    fn default() -> Self {
        Self {
            position: glm::Vec3::default(),
            normal: glm::Vec3::default(),
            uv_0: glm::Vec2::default(),
            uv_1: glm::Vec2::default(),
            joint_0: glm::Vec4::default(),
            weight_0: glm::Vec4::default(),
            color_0: glm::vec3(1.0, 1.0, 1.0),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Animation {
    pub name: String,
    pub time: f32,
    pub channels: Vec<Channel>,
    pub max_animation_time: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Channel {
    pub target_node: usize,
    pub inputs: Vec<f32>,
    pub transformations: TransformationSet,
    pub _interpolation: Interpolation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Interpolation {
    Linear,
    Step,
    CubicSpline,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TransformationSet {
    Translations(Vec<glm::Vec3>),
    Rotations(Vec<glm::Vec4>),
    Scales(Vec<glm::Vec3>),
    MorphTargetWeights(Vec<f32>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    pub base_color_factor: glm::Vec4,
    pub emissive_factor: glm::Vec3,
    pub color_texture_index: i32,
    pub color_texture_set: i32,
    pub metallic_roughness_texture_index: i32,
    pub metallic_roughness_texture_set: i32, // B channel - metalness values. G channel - roughness values
    pub normal_texture_index: i32,
    pub normal_texture_set: i32,
    pub normal_texture_scale: f32,
    pub occlusion_texture_index: i32,
    pub occlusion_texture_set: i32, // R channel - occlusion values
    pub occlusion_strength: f32,
    pub emissive_texture_index: i32,
    pub emissive_texture_set: i32,
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub is_unlit: bool,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            name: "<Unnamed>".to_string(),
            base_color_factor: glm::vec4(1.0, 1.0, 1.0, 1.0),
            emissive_factor: glm::Vec3::identity(),
            color_texture_index: -1,
            color_texture_set: -1,
            metallic_roughness_texture_index: -1,
            metallic_roughness_texture_set: -1,
            normal_texture_index: -1,
            normal_texture_set: -1,
            normal_texture_scale: 1.0,
            occlusion_texture_index: -1,
            occlusion_texture_set: -1,
            occlusion_strength: 1.0,
            emissive_texture_index: -1,
            emissive_texture_set: -1,
            metallic_factor: 1.0,
            roughness_factor: 0.0,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.0,
            is_unlit: false,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum AlphaMode {
    Opaque = 1,
    Mask,
    Blend,
}

impl Default for AlphaMode {
    fn default() -> Self {
        Self::Opaque
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Texture {
    pub pixels: Vec<u8>,
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub sampler: Sampler,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Format {
    R8,
    R8G8,
    R8G8B8,
    R8G8B8A8,
    B8G8R8,
    B8G8R8A8,
    R16,
    R16G16,
    R16G16B16,
    R16G16B16A16,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Sampler {
    pub name: String,
    pub min_filter: Filter,
    pub mag_filter: Filter,
    pub wrap_s: WrappingMode,
    pub wrap_t: WrappingMode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WrappingMode {
    ClampToEdge,
    MirroredRepeat,
    Repeat,
}

impl Default for WrappingMode {
    fn default() -> Self {
        Self::ClampToEdge
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Filter {
    Nearest,
    Linear,
}

impl Default for Filter {
    fn default() -> Self {
        Self::Nearest
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneGraph(pub Graph<usize, ()>);

impl Default for SceneGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl SceneGraph {
    pub fn new() -> Self {
        Self(Graph::<usize, ()>::new())
    }

    pub fn number_of_nodes(&self) -> usize {
        self.0.raw_nodes().len()
    }

    pub fn add_node(&mut self, node: usize) -> NodeIndex {
        self.0.add_node(node)
    }

    pub fn add_edge(&mut self, parent_node: NodeIndex, node: NodeIndex) {
        let _edge_index = self.0.add_edge(parent_node, node, ());
    }

    pub fn parent_of(&self, index: NodeIndex) -> Option<NodeIndex> {
        let mut incoming_walker = self.0.neighbors_directed(index, Incoming).detach();
        incoming_walker.next_node(&self.0)
    }

    pub fn walk(&self, mut action: impl FnMut(NodeIndex) -> Result<()>) -> Result<()> {
        let mut dfs = Dfs::new(&self.0, NodeIndex::new(0));
        while let Some(node_index) = dfs.next(&self.0) {
            action(node_index)?;
        }
        Ok(())
    }

    pub fn global_transform(&self, index: NodeIndex, nodes: &[Node]) -> glm::Mat4 {
        let transform = nodes[self[index]].transform.matrix();
        let mut incoming_walker = self.0.neighbors_directed(index, Incoming).detach();
        match incoming_walker.next_node(&self.0) {
            Some(parent_index) => self.global_transform(parent_index, nodes) * transform,
            None => transform,
        }
    }

    pub fn find_node(&self, weight: usize) -> Option<NodeIndex> {
        match self.0.node_indices().find(|i| self[*i] == weight) {
            Some(index) => Some(index),
            None => None,
        }
    }

    pub fn offset_by(&mut self, offset: usize) {
        for index in self.0.node_indices() {
            self[index] += offset;
        }
    }
}

impl Index<NodeIndex> for SceneGraph {
    type Output = usize;

    fn index(&self, index: NodeIndex) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<NodeIndex> for SceneGraph {
    fn index_mut(&mut self, index: NodeIndex) -> &mut Self::Output {
        &mut self.0[index]
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Asset {
    pub nodes: Vec<Node>,
    pub scenes: Vec<Scene>,
    pub animations: Vec<Animation>,
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
    pub geometry: Geometry,
}

impl Asset {
    pub fn material_at_index(&self, index: usize) -> Result<&Material> {
        let error_message = format!("Failed to lookup asset material at index: {}", index);
        self.materials.get(index).context(error_message)
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
                            self.nodes[channel.target_node].transform.translation = translation_vec;
                        }
                        TransformationSet::Rotations(rotations) => {
                            let start = rotations[previous_key];
                            let end = rotations[next_key];
                            let start_quat = glm::make_quat(start.as_slice());
                            let end_quat = glm::make_quat(end.as_slice());
                            let rotation_quat =
                                glm::quat_slerp(&start_quat, &end_quat, interpolation);
                            self.nodes[channel.target_node].transform.rotation = rotation_quat;
                        }
                        TransformationSet::Scales(scales) => {
                            let start = scales[previous_key];
                            let end = scales[next_key];
                            let scale_vec = glm::mix(&start, &end, interpolation);
                            self.nodes[channel.target_node].transform.scale = scale_vec;
                        }
                        TransformationSet::MorphTargetWeights(animation_weights) => {
                            match self.nodes[channel.target_node].mesh.as_mut() {
                                Some(mesh) => {
                                    let number_of_mesh_weights = mesh.weights.len();
                                    if animation_weights.len() % number_of_mesh_weights != 0 {
                                        log::warn!("Animation channel's weights are not a multiple of the mesh's weights: (channel) {} % (mesh) {} != 0", number_of_mesh_weights, animation_weights.len());
                                        continue;
                                    }
                                    let weights = animation_weights
                                        .as_slice()
                                        .chunks(number_of_mesh_weights)
                                        .collect::<Vec<_>>();
                                    let start = weights[previous_key];
                                    let end = weights[next_key];
                                    for index in 0..number_of_mesh_weights {
                                        mesh.weights[index] = glm::lerp_scalar(
                                            start[index],
                                            end[index],
                                            interpolation,
                                        );
                                    }
                                }
                                None => {
                                    log::warn!("Animation channel's target node animates morph target weights, but node has no mesh!");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn joint_matrices(&self) -> Result<Vec<glm::Mat4>> {
        let mut offset = 0;
        let first_scene = self.scenes.first().context("Failed to find a scene")?;

        let mut number_of_joints = 0;
        for graph in first_scene.graphs.iter() {
            graph.walk(|node_index| {
                let node_offset = graph[node_index];
                if let Some(skin) = self.nodes[node_offset].skin.as_ref() {
                    number_of_joints += skin.joints.len();
                }
                Ok(())
            })?;
        }

        let mut joint_matrices = vec![glm::Mat4::identity(); number_of_joints];
        for graph in first_scene.graphs.iter() {
            graph.walk(|node_index| {
                let node_offset = graph[node_index];
                let node_transform = graph.global_transform(node_index, &self.nodes);
                if let Some(skin) = self.nodes[node_offset].skin.as_ref() {
                    for joint in skin.joints.iter() {
                        let joint_transform = {
                            let mut transform = glm::Mat4::identity();
                            for graph in first_scene.graphs.iter() {
                                if let Some(index) = graph.find_node(joint.target_node) {
                                    transform = graph.global_transform(index, &self.nodes);
                                }
                            }
                            transform
                        };

                        joint_matrices[offset] = glm::inverse(&node_transform)
                            * joint_transform
                            * joint.inverse_bind_matrix;

                        offset += 1;
                    }
                }
                Ok(())
            })?;
        }
        Ok(joint_matrices)
    }

    pub fn morph_targets(&self) -> Result<Vec<glm::Vec4>> {
        let first_scene = self.scenes.first().context("Failed to find a scene")?;
        let number_of_morph_targets = self
            .nodes
            .iter()
            .filter_map(|node| node.mesh.as_ref())
            .flat_map(|mesh| &mesh.primitives)
            .flat_map(|primitive| &primitive.morph_targets)
            .map(|morph_target| morph_target.total_length())
            .sum();

        let mut offset = 0;
        let mut morph_targets = vec![glm::Vec4::identity(); number_of_morph_targets];
        for graph in first_scene.graphs.iter() {
            graph.walk(|node_index| {
                let node_offset = graph[node_index];
                if let Some(mesh) = self.nodes[node_offset].mesh.as_ref() {
                    for primitive in mesh.primitives.iter() {
                        for morph_target in primitive.morph_targets.iter() {
                            for position in morph_target.positions.iter() {
                                morph_targets[offset] = *position;
                                offset += 1;
                            }

                            for normal in morph_target.normals.iter() {
                                morph_targets[offset] = *normal;
                                offset += 1;
                            }

                            for tangent in morph_target.tangents.iter() {
                                morph_targets[offset] = *tangent;
                                offset += 1;
                            }
                        }
                    }
                }
                Ok(())
            })?;
        }
        Ok(morph_targets)
    }

    pub fn morph_target_weights(&self) -> Result<Vec<f32>> {
        let first_scene = self.scenes.first().context("Failed to find a scene")?;
        let number_of_morph_target_weights = self
            .nodes
            .iter()
            .filter_map(|node| node.mesh.as_ref())
            .map(|mesh| mesh.weights.len())
            .sum();

        let mut offset = 0;
        let mut morph_target_weights = vec![0_f32; number_of_morph_target_weights];
        for graph in first_scene.graphs.iter() {
            graph.walk(|node_index| {
                let node_offset = graph[node_index];
                if let Some(mesh) = self.nodes[node_offset].mesh.as_ref() {
                    for weight in mesh.weights.iter() {
                        morph_target_weights[offset] = *weight;
                        offset += 1;
                    }
                }
                Ok(())
            })?;
        }
        Ok(morph_target_weights)
    }

    pub fn merge_with(&mut self, asset: Self) -> Result<()> {
        let Self {
            mut nodes,
            mut scenes,
            mut animations,
            mut materials,
            textures,
            mut geometry,
        } = asset;

        let number_of_textures = self.textures.len();
        self.textures.extend_from_slice(&textures);

        let number_of_materials = self.materials.len();
        materials.iter_mut().for_each(|material| {
            let increment = |value: &mut i32| {
                if *value != -1_i32 {
                    *value += number_of_textures as i32;
                }
            };
            increment(&mut material.color_texture_index);
            increment(&mut material.metallic_roughness_texture_index);
            increment(&mut material.normal_texture_index);
            increment(&mut material.occlusion_texture_index);
            increment(&mut material.emissive_texture_index);
        });
        materials
            .into_iter()
            .for_each(|material| self.materials.push(material));

        let number_of_vertices = self.geometry.vertices.len();
        let number_of_indices = self.geometry.indices.len();
        geometry
            .indices
            .iter_mut()
            .for_each(|index| *index += number_of_vertices as u32);

        let Geometry { vertices, indices } = geometry;
        vertices
            .into_iter()
            .for_each(|vertex| self.geometry.vertices.push(vertex));
        indices
            .into_iter()
            .for_each(|index| self.geometry.indices.push(index));

        let number_of_nodes = self.nodes.len();
        nodes.iter_mut().for_each(|node| {
            if let Some(mesh) = node.mesh.as_mut() {
                for primitive in mesh.primitives.iter_mut() {
                    primitive.first_index += number_of_indices;
                    primitive.first_vertex += number_of_vertices;
                    if let Some(material_index) = primitive.material_index.as_mut() {
                        *material_index += number_of_materials;
                    }
                }
            }

            if let Some(skin) = node.skin.as_mut() {
                for joint in skin.joints.iter_mut() {
                    joint.target_node += number_of_nodes;
                }
            }
        });
        nodes.into_iter().for_each(|node| self.nodes.push(node));

        scenes[0].graphs.iter_mut().for_each(|graph| {
            let mut scenegraph = graph.clone();
            scenegraph.offset_by(number_of_nodes);
            self.scenes[0].graphs.push(scenegraph);
        });

        animations.iter_mut().for_each(|animation| {
            for channel in animation.channels.iter_mut() {
                channel.target_node += number_of_nodes;
            }
        });

        animations
            .into_iter()
            .for_each(|node| self.animations.push(node));

        Ok(())
    }
}
