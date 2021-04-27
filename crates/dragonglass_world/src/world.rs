use crate::{Name, WorldPhysics};
use anyhow::{bail, Context, Result};
use lazy_static::lazy_static;
use legion::{
    serialize::set_entity_serializer, serialize::Canon, world::Entry, EntityStore, IntoQuery,
    Registry,
};
use na::{linalg::QR, Isometry3, Translation3, UnitQuaternion};
use nalgebra as na;
use nalgebra_glm as glm;
use petgraph::{graph::WalkNeighbors, prelude::*};
use serde::{de::DeserializeSeed, Deserialize, Deserializer, Serialize, Serializer};
use std::{
    collections::HashMap,
    ops::{Index, IndexMut},
};
use std::{
    path::Path,
    sync::{Arc, RwLock},
};

lazy_static! {
    pub static ref COMPONENT_REGISTRY: Arc<RwLock<Registry<String>>> = {
        let mut registry = Registry::default();
        registry.register::<Name>("name".to_string());
        registry.register::<Transform>("transform".to_string());
        registry.register::<Camera>("camera".to_string());
        registry.register::<MeshRender>("mesh".to_string());
        registry.register::<Skin>("skin".to_string());
        registry.register::<Light>("light".to_string());
        Arc::new(RwLock::new(registry))
    };
    pub static ref ENTITY_SERIALIZER: Canon = Canon::default();
}

pub type Ecs = legion::World;
pub type Entity = legion::Entity;

#[derive(Default, Serialize, Deserialize)]
pub struct World {
    #[serde(serialize_with = "serialize_ecs", deserialize_with = "deserialize_ecs")]
    pub ecs: Ecs,
    pub physics: WorldPhysics,
    pub scene: Scene,
    pub animations: Vec<Animation>,
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
    pub geometry: Geometry,
}

impl World {
    pub const MAIN_CAMERA_NAME: &'static str = &"Main Camera";

    pub fn new() -> Result<World> {
        let mut world = World::default();
        world.initialize()?;
        Ok(world)
    }

    fn initialize(&mut self) -> Result<()> {
        self.scene = Scene::default();
        self.scene.name = "Main Scene".to_string();
        self.add_default_camera()?;
        Ok(())
    }

    pub fn entry_mut(&mut self, entity: Entity) -> Result<Entry> {
        self.ecs
            .entry(entity)
            .context("Failed to find a requested camera entity!")
    }

    fn add_default_camera(&mut self) -> Result<()> {
        let position = glm::vec3(0.0, 0.0, 10.0);
        let mut transform = Transform {
            translation: position,
            ..Default::default()
        };
        transform.look_at(&(-position), &glm::Vec3::y());

        let camera_entity = self.ecs.push((
            transform,
            Camera {
                name: Self::MAIN_CAMERA_NAME.to_string(),
                projection: Projection::Perspective(PerspectiveCamera {
                    aspect_ratio: None,
                    y_fov_rad: 70_f32.to_radians(),
                    z_far: Some(1000.0),
                    z_near: 0.1,
                }),
                enabled: true,
            },
        ));

        self.scene.default_scenegraph_mut()?.add_node(camera_entity);

        Ok(())
    }

    pub fn add_default_light(&mut self) -> Result<()> {
        let position = glm::vec3(-4.0, 10.0, 0.0);
        let mut transform = Transform {
            translation: position,
            ..Default::default()
        };
        transform.look_at(&(-position), &glm::Vec3::y());
        let light_entity = self.ecs.push((
            transform,
            Light {
                color: glm::vec3(200.0, 200.0, 200.0),
                kind: LightKind::Point,
                ..Default::default()
            },
        ));
        self.scene.default_scenegraph_mut()?.add_node(light_entity);
        Ok(())
    }

    pub fn active_camera(&self) -> Result<Entity> {
        let mut query = <(Entity, &Camera)>::query();
        for (entity, camera) in query.iter(&self.ecs) {
            if camera.enabled {
                return Ok(*entity);
            }
        }
        bail!("The world must have at least one entity with an enabled camera component to render with!")
    }

    pub fn global_transform(&self, graph: &SceneGraph, index: NodeIndex) -> Result<glm::Mat4> {
        let entity = graph[index];
        let transform = match self.ecs.entry_ref(entity)?.get_component::<Transform>() {
            Ok(transform) => transform.matrix(),
            Err(_) => bail!(
                "A transform component was requested from a component that does not have one!"
            ),
        };
        let mut incoming_walker = graph.0.neighbors_directed(index, Incoming).detach();
        match incoming_walker.next_node(&graph.0) {
            Some(parent_index) => Ok(self.global_transform(graph, parent_index)? * transform),
            None => Ok(transform),
        }
    }

    pub fn entity_global_transform_matrix(&self, entity: Entity) -> Result<glm::Mat4> {
        let mut transform = glm::Mat4::identity();
        let mut found = false;
        for graph in self.scene.graphs.iter() {
            graph.walk(|node_index| {
                if entity != graph[node_index] {
                    return Ok(());
                }
                transform = self.global_transform(graph, node_index)?;
                found = true;
                Ok(())
            })?;
            if found {
                break;
            }
        }
        if !found {
            // TODO: Maybe returning an error if the global transform of an entity that isn't in the scenegraph is better...
            // Not found in the scenegraph, so the entity just have a local transform
            transform = self
                .ecs
                .entry_ref(entity)?
                .get_component::<Transform>()?
                .matrix();
        }
        Ok(transform)
    }

    pub fn entity_global_transform(&self, entity: Entity) -> Result<Transform> {
        let transform_matrix = self.entity_global_transform_matrix(entity)?;
        Ok(Transform::from(transform_matrix))
    }

    pub fn active_camera_matrices(&self, aspect_ratio: f32) -> Result<(glm::Mat4, glm::Mat4)> {
        let camera_entity = self.active_camera()?;
        let transform = self.entity_global_transform(camera_entity)?;
        let view = transform.as_view_matrix();
        let projection = {
            let entry = self.ecs.entry_ref(camera_entity)?;
            let camera = entry.get_component::<Camera>()?;
            camera.projection_matrix(aspect_ratio)
        };
        Ok((projection, view))
    }

    pub fn active_camera_is_main(&self) -> Result<bool> {
        let entity = self.active_camera()?;
        let entry = self.ecs.entry_ref(entity)?;
        let camera = entry.get_component::<Camera>()?;
        Ok(camera.name == Self::MAIN_CAMERA_NAME)
    }

    pub fn clear(&mut self) -> Result<()> {
        self.ecs.clear();
        self.scene.graphs.clear();
        self.textures.clear();
        self.animations.clear();
        self.materials.clear();
        self.geometry.clear();
        self.initialize()?;
        Ok(())
    }

    pub fn material_at_index(&self, index: usize) -> Result<&Material> {
        let error_message = format!("Failed to lookup material at index: {}", index);
        self.materials.get(index).context(error_message)
    }

    pub fn animate(&mut self, index: usize, step: f32) -> Result<()> {
        let Self {
            animations, ecs, ..
        } = self;

        if animations.get(index).is_none() {
            // TODO: Make this an error and handle it at a higher layer
            log::warn!("No animation at index: {}. Skipping...", index);
            return Ok(());
        }

        let mut animation = &mut animations[index];
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
                            ecs.entry_mut(channel.target)?
                                .get_component_mut::<Transform>()?
                                .translation = translation_vec;
                        }
                        TransformationSet::Rotations(rotations) => {
                            let start = rotations[previous_key];
                            let end = rotations[next_key];
                            let start_quat = glm::make_quat(start.as_slice());
                            let end_quat = glm::make_quat(end.as_slice());
                            let rotation_quat =
                                glm::quat_slerp(&start_quat, &end_quat, interpolation);

                            ecs.entry_mut(channel.target)?
                                .get_component_mut::<Transform>()?
                                .rotation = rotation_quat;
                        }
                        TransformationSet::Scales(scales) => {
                            let start = scales[previous_key];
                            let end = scales[next_key];
                            let scale_vec = glm::mix(&start, &end, interpolation);

                            ecs.entry_mut(channel.target)?
                                .get_component_mut::<Transform>()?
                                .scale = scale_vec;
                        }
                        TransformationSet::MorphTargetWeights(animation_weights) => {
                            match ecs.entry_mut(channel.target)?.get_component_mut::<Mesh>() {
                                Ok(mesh) => {
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
                                        (*mesh).weights[index] = glm::lerp_scalar(
                                            start[index],
                                            end[index],
                                            interpolation,
                                        );
                                    }
                                }
                                Err(_) => {
                                    log::warn!("Animation channel's target node animates morph target weights, but node has no mesh!");
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn lights(&self) -> Result<Vec<(Transform, Light)>> {
        let mut lights = Vec::new();
        for graph in self.scene.graphs.iter() {
            graph.walk(|node_index| {
                let entity = graph[node_index];
                let node_transform = self.global_transform(graph, node_index)?;
                if let Ok(light) = self.ecs.entry_ref(entity)?.get_component::<Light>() {
                    lights.push((Transform::from(node_transform), *light));
                }
                Ok(())
            })?;
        }
        Ok(lights)
    }

    pub fn joint_matrices(&self) -> Result<Vec<glm::Mat4>> {
        let mut offset = 0;
        let mut number_of_joints = 0;
        for graph in self.scene.graphs.iter() {
            graph.walk(|node_index| {
                let entity = graph[node_index];
                if let Ok(skin) = self.ecs.entry_ref(entity)?.get_component::<Skin>() {
                    number_of_joints += skin.joints.len();
                }
                Ok(())
            })?;
        }
        let mut joint_matrices = vec![glm::Mat4::identity(); number_of_joints];
        for graph in self.scene.graphs.iter() {
            graph.walk(|node_index| {
                let entity = graph[node_index];
                let node_transform = self.global_transform(graph, node_index)?;
                if let Ok(skin) = self.ecs.entry_ref(entity)?.get_component::<Skin>() {
                    for joint in skin.joints.iter() {
                        let joint_transform = {
                            let mut transform = glm::Mat4::identity();
                            for graph in self.scene.graphs.iter() {
                                if let Some(index) = graph.find_node(joint.target) {
                                    transform = self.global_transform(graph, index)?;
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

    pub fn as_bytes(&self) -> Result<Vec<u8>> {
        Ok(set_entity_serializer(&*ENTITY_SERIALIZER, || {
            bincode::serialize(&self)
        })?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<World> {
        Ok(set_entity_serializer(&*ENTITY_SERIALIZER, || {
            bincode::deserialize(&bytes[..])
        })?)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        Ok(std::fs::write(path, &self.as_bytes()?)?)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::from_bytes(&std::fs::read(path)?)?)
    }

    pub fn roundtrip(&self) -> Result<World> {
        Self::from_bytes(&self.as_bytes()?)
    }
}

fn serialize_ecs<S>(ecs: &Ecs, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let registry = (&*COMPONENT_REGISTRY)
        .read()
        .expect("Failed to get the component registry lock!");
    ecs.as_serializable(legion::any(), &*registry, &*ENTITY_SERIALIZER)
        .serialize(serializer)
}

fn deserialize_ecs<'de, D>(deserializer: D) -> Result<Ecs, D::Error>
where
    D: Deserializer<'de>,
{
    (&*COMPONENT_REGISTRY)
        .read()
        .expect("Failed to get the component registry lock!")
        .as_deserialize(&*ENTITY_SERIALIZER)
        .deserialize(deserializer)
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Scene {
    pub name: String,
    pub graphs: Vec<SceneGraph>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            name: "Unnamed Scene".to_string(),
            graphs: vec![SceneGraph::default()],
        }
    }
}

impl Scene {
    pub fn default_scenegraph_mut(&mut self) -> Result<&mut SceneGraph> {
        match self.graphs.iter_mut().next() {
            Some(graph) => Ok(graph),
            None => bail!("Failed to find default scenegraph in scene: {}!", self.name),
        }
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Transform {
    pub translation: glm::Vec3,
    pub rotation: glm::Quat,
    pub scale: glm::Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: glm::vec3(0.0, 0.0, 0.0),
            rotation: glm::quat_conjugate(&glm::quat_look_at(&glm::Vec3::z(), &glm::Vec3::y())),
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

    pub fn as_isometry(&self) -> Isometry3<f32> {
        Isometry3::from_parts(
            Translation3::from(self.translation),
            UnitQuaternion::from_quaternion(self.rotation),
        )
    }

    /// Decomposes a 4x4 augmented rotation matrix without shear into translation, rotation, and scaling components
    fn decompose_matrix(transform: glm::Mat4) -> (glm::Vec3, glm::Quat, glm::Vec3) {
        let translation = glm::vec3(transform.m14, transform.m24, transform.m34);

        let qr_decomposition = QR::new(transform);
        let rotation = glm::to_quat(&qr_decomposition.q());

        let scale = transform.m44
            * glm::vec3(
                (transform.m11.powi(2) + transform.m21.powi(2) + transform.m31.powi(2)).sqrt(),
                (transform.m12.powi(2) + transform.m22.powi(2) + transform.m32.powi(2)).sqrt(),
                (transform.m13.powi(2) + transform.m23.powi(2) + transform.m33.powi(2)).sqrt(),
            );

        (translation, rotation, scale)
    }

    pub fn as_view_matrix(&self) -> glm::Mat4 {
        let eye = self.translation;
        let target = self.translation + self.forward();
        let up = self.up();
        glm::look_at(&eye, &target, &up)
    }

    pub fn right(&self) -> glm::Vec3 {
        glm::quat_rotate_vec3(&self.rotation.normalize(), &glm::Vec3::x())
    }

    pub fn up(&self) -> glm::Vec3 {
        glm::quat_rotate_vec3(&self.rotation.normalize(), &glm::Vec3::y())
    }

    pub fn forward(&self) -> glm::Vec3 {
        glm::quat_rotate_vec3(&self.rotation.normalize(), &(-glm::Vec3::z()))
    }

    pub fn rotate(&mut self, increment: &glm::Vec3) {
        self.translation = glm::rotate_x_vec3(&self.translation, increment.x);
        self.translation = glm::rotate_y_vec3(&self.translation, increment.y);
        self.translation = glm::rotate_z_vec3(&self.translation, increment.z);
    }

    pub fn look_at(&mut self, target: &glm::Vec3, up: &glm::Vec3) {
        self.rotation = glm::quat_conjugate(&glm::quat_look_at(target, up));
    }
}

impl From<glm::Mat4> for Transform {
    fn from(matrix: glm::Mat4) -> Self {
        let (translation, rotation, scale) = Self::decompose_matrix(matrix);
        Self {
            translation,
            rotation,
            scale,
        }
    }
}

// The 'name' field is purposefully omitted to keep the struct 'Copy'able
#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
pub struct Light {
    pub color: glm::Vec3,
    pub intensity: f32,
    pub range: f32,
    pub kind: LightKind,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum LightKind {
    Directional,
    Point,
    Spot {
        inner_cone_angle: f32,
        outer_cone_angle: f32,
    },
}

impl Default for LightKind {
    fn default() -> Self {
        Self::Directional
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Camera {
    pub name: String,
    pub projection: Projection,
    pub enabled: bool,
}

impl Camera {
    pub fn projection_matrix(&self, viewport_aspect_ratio: f32) -> glm::Mat4 {
        match &self.projection {
            Projection::Perspective(camera) => camera.matrix(viewport_aspect_ratio),
            Projection::Orthographic(camera) => camera.matrix(),
        }
    }

    pub fn is_orthographic(&self) -> bool {
        match self.projection {
            Projection::Perspective(_) => false,
            Projection::Orthographic(_) => true,
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
    pub y_fov_rad: f32,
    pub z_far: Option<f32>,
    pub z_near: f32,
}

impl PerspectiveCamera {
    pub fn matrix(&self, viewport_aspect_ratio: f32) -> glm::Mat4 {
        let aspect_ratio = if let Some(aspect_ratio) = self.aspect_ratio {
            aspect_ratio
        } else {
            viewport_aspect_ratio
        };

        if let Some(z_far) = self.z_far {
            glm::perspective_zo(aspect_ratio, self.y_fov_rad, self.z_near, z_far)
        } else {
            glm::infinite_perspective_rh_zo(aspect_ratio, self.y_fov_rad, self.z_near)
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
    pub target: Entity,
    pub inverse_bind_matrix: glm::Mat4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshRender {
    pub name: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Mesh {
    pub name: String,
    pub primitives: Vec<Primitive>,
    pub weights: Vec<f32>,
}

impl Mesh {
    pub fn bounding_box(&self) -> BoundingBox {
        let mut bounding_box = BoundingBox::new_invalid();
        self.primitives
            .iter()
            .map(|primitive| &primitive.bounding_box)
            .for_each(|primitive_bounding_box| bounding_box.fit_box(primitive_bounding_box));
        bounding_box
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub min: glm::Vec3,
    pub max: glm::Vec3,
}

impl BoundingBox {
    pub fn new_invalid() -> Self {
        Self {
            min: glm::vec3(f32::MAX, f32::MAX, f32::MAX),
            max: glm::vec3(f32::MIN, f32::MIN, f32::MIN),
        }
    }

    pub fn new(min: glm::Vec3, max: glm::Vec3) -> Self {
        Self { min, max }
    }

    pub fn extents(&self) -> glm::Vec3 {
        glm::abs(&(self.max - self.min))
    }

    pub fn half_extents(&self) -> glm::Vec3 {
        self.extents() / 2.0
    }

    pub fn center(&self) -> glm::Vec3 {
        self.min + self.half_extents()
    }

    pub fn fit_box(&mut self, bounding_box: &Self) {
        self.fit_point(bounding_box.min);
        self.fit_point(bounding_box.max);
    }

    pub fn fit_point(&mut self, point: glm::Vec3) {
        self.min.x = f32::min(self.min.x, point.x);
        self.min.y = f32::min(self.min.y, point.y);
        self.min.z = f32::min(self.min.z, point.z);

        self.max.x = f32::max(self.max.x, point.x);
        self.max.y = f32::max(self.max.y, point.y);
        self.max.z = f32::max(self.max.z, point.z);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Primitive {
    pub first_vertex: usize,
    pub first_index: usize,
    pub number_of_vertices: usize,
    pub number_of_indices: usize,
    pub material_index: Option<usize>,
    pub morph_targets: Vec<MorphTarget>,
    pub bounding_box: BoundingBox,
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
    pub meshes: HashMap<String, Mesh>,
}

impl Geometry {
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
    }
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
    pub target: Entity,
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
            roughness_factor: 1.0,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
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
        Self::Repeat
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
pub struct SceneGraph(pub Graph<Entity, ()>);

impl Default for SceneGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl SceneGraph {
    pub fn new() -> Self {
        Self(Graph::<Entity, ()>::new())
    }

    pub fn number_of_nodes(&self) -> usize {
        self.0.raw_nodes().len()
    }

    pub fn add_node(&mut self, node: Entity) -> NodeIndex {
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
        for node_index in self.0.node_indices() {
            if self.has_parents(node_index) {
                continue;
            }
            let mut dfs = Dfs::new(&self.0, node_index);
            while let Some(node_index) = dfs.next(&self.0) {
                action(node_index)?;
            }
        }
        Ok(())
    }

    pub fn has_neighbors(&self, index: NodeIndex) -> bool {
        self.has_parents(index) || self.has_children(index)
    }

    pub fn has_parents(&self, index: NodeIndex) -> bool {
        self.neighbors(index, Incoming).next_node(&self.0).is_some()
    }

    pub fn has_children(&self, index: NodeIndex) -> bool {
        self.neighbors(index, Outgoing).next_node(&self.0).is_some()
    }

    pub fn neighbors(&self, index: NodeIndex, direction: Direction) -> WalkNeighbors<u32> {
        self.0.neighbors_directed(index, direction).detach()
    }

    pub fn find_node(&self, entity: Entity) -> Option<NodeIndex> {
        match self.0.node_indices().find(|i| self[*i] == entity) {
            Some(index) => Some(index),
            None => None,
        }
    }
}

impl Index<NodeIndex> for SceneGraph {
    type Output = Entity;

    fn index(&self, index: NodeIndex) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<NodeIndex> for SceneGraph {
    fn index_mut(&mut self, index: NodeIndex) -> &mut Self::Output {
        &mut self.0[index]
    }
}
