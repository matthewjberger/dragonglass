use anyhow::{Context, Result};
use nalgebra_glm as glm;
use petgraph::prelude::*;

pub struct Scene {
    pub name: String,
    pub graphs: Vec<SceneGraph>,
}

pub struct Node {
    pub name: String,
    pub transform: Transform,
    pub camera: Option<Camera>,
    pub mesh: Option<Mesh>,
    pub skin: Option<Skin>,
    pub light: Option<Light>,
}

#[derive(Debug)]
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

pub struct Light {
    pub name: String,
    pub color: glm::Vec3,
    pub intensity: f32,
    pub range: f32,
    pub kind: LightKind,
}

pub enum LightKind {
    Directional,
    Point,
    Spot {
        inner_cone_angle: f32,
        outer_cone_angle: f32,
    },
}

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

#[derive(Debug)]
pub enum Projection {
    Perspective(PerspectiveCamera),
    Orthographic(OrthographicCamera),
}

#[derive(Default, Debug)]
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

#[derive(Default, Debug)]
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
    pub weights: Option<Vec<f32>>,
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
    pub channels: Vec<Channel>,
    pub max_animation_time: f32,
}

#[derive(Debug)]
pub struct Channel {
    pub target_node: usize,
    pub inputs: Vec<f32>,
    pub transformations: TransformationSet,
    pub _interpolation: Interpolation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Interpolation {
    Linear,
    Step,
    CubicSpline,
}

#[derive(Debug)]
pub enum TransformationSet {
    Translations(Vec<glm::Vec3>),
    Rotations(Vec<glm::Vec4>),
    Scales(Vec<glm::Vec3>),
    MorphTargetWeights(Vec<f32>),
}

#[derive(Debug)]
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
    pub is_unlit: i32,
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
            is_unlit: 0,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
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

#[derive(Clone, Debug)]
pub struct Texture {
    pub pixels: Vec<u8>,
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub sampler: Sampler,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

#[derive(Default, Clone, Debug)]
pub struct Sampler {
    pub name: String,
    pub min_filter: Filter,
    pub mag_filter: Filter,
    pub wrap_s: WrappingMode,
    pub wrap_t: WrappingMode,
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub enum Filter {
    Nearest,
    Linear,
}

impl Default for Filter {
    fn default() -> Self {
        Self::Nearest
    }
}

pub type SceneGraph = Graph<usize, ()>;

pub fn walk_scenegraph(
    graph: &SceneGraph,
    mut action: impl FnMut(NodeIndex) -> Result<()>,
) -> Result<()> {
    let mut dfs = Dfs::new(graph, NodeIndex::new(0));
    while let Some(node_index) = dfs.next(&graph) {
        action(node_index)?;
    }
    Ok(())
}

pub fn global_transform(graph: &SceneGraph, index: NodeIndex, nodes: &[Node]) -> glm::Mat4 {
    let transform = nodes[graph[index]].transform.matrix();
    let mut incoming_walker = graph.neighbors_directed(index, Incoming).detach();
    match incoming_walker.next_node(graph) {
        Some(parent_index) => global_transform(graph, parent_index, nodes) * transform,
        None => transform,
    }
}

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
                        TransformationSet::MorphTargetWeights(weights) => {
                            let start = weights[previous_key];
                            let end = weights[next_key];
                            let _weight = glm::lerp_scalar(start, end, interpolation);
                            // TODO: Assign this weight somewhere...
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
            walk_scenegraph(graph, |node_index| {
                let node_offset = graph[node_index];
                if let Some(skin) = self.nodes[node_offset].skin.as_ref() {
                    number_of_joints += skin.joints.len();
                }
                Ok(())
            })?;
        }

        let mut joint_matrices = vec![glm::Mat4::identity(); number_of_joints];
        for graph in first_scene.graphs.iter() {
            let mut dfs = Dfs::new(graph, NodeIndex::new(0));
            while let Some(node_index) = dfs.next(&graph) {
                let node_offset = graph[node_index];
                let node_transform = global_transform(graph, node_index, &self.nodes);
                if let Some(skin) = self.nodes[node_offset].skin.as_ref() {
                    for joint in skin.joints.iter() {
                        let joint_transform = {
                            let mut transform = glm::Mat4::identity();
                            for graph in first_scene.graphs.iter() {
                                if let Some(index) = graph
                                    .node_indices()
                                    .find(|i| graph[*i] == joint.target_node)
                                {
                                    transform = global_transform(graph, index, &self.nodes);
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
            }
        }
        Ok(joint_matrices)
    }
}