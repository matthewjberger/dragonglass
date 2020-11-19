use crate::asset::{
    AlphaMode, Animation, Asset, Camera, Channel, Format, Geometry, Interpolation, Joint, Light,
    LightKind, Material, Mesh, Node, OrthographicCamera, PerspectiveCamera, Primitive, Projection,
    Scene, SceneGraph, Skin, Texture, Transform, TransformationSet, Vertex,
};
use anyhow::{Context, Result};
use gltf::animation::util::ReadOutputs;
use nalgebra_glm as glm;
use petgraph::prelude::*;
use std::path::Path;

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

fn node_transform(node: &gltf::Node) -> Transform {
    let (translation, rotation, scale) = node.transform().decomposed();

    let translation: glm::Vec3 = translation.into();
    let scale: glm::Vec3 = scale.into();
    let rotation = glm::quat_normalize(&glm::make_quat(&rotation));

    Transform::new(translation, rotation, scale)
}

const DEFAULT_NAME: &str = "<Unnamed>";

pub fn load_gltf_asset(path: impl AsRef<Path>) -> Result<Asset> {
    let (gltf, buffers, textures) = gltf::import(path)?;

    let textures = load_textures(&textures);
    let (nodes, geometry) = load_nodes(&gltf, &buffers)?;
    let scenes = load_scenes(&gltf);
    let animations = load_animations(&gltf, &buffers)?;
    let materials = load_materials(&gltf, &buffers)?;

    Ok(Asset {
        nodes,
        scenes,
        animations,
        materials,
        textures,
        geometry,
    })
}

fn load_textures(textures: &[gltf::image::Data]) -> Vec<Texture> {
    textures
        .iter()
        .map(|texture| Texture {
            pixels: texture.pixels.to_vec(),
            format: map_gltf_format(texture.format),
            width: texture.width,
            height: texture.height,
        })
        .collect()
}

fn map_gltf_format(format: gltf::image::Format) -> Format {
    match format {
        gltf::image::Format::R8 => Format::R8,
        gltf::image::Format::R8G8 => Format::R8G8,
        gltf::image::Format::R8G8B8 => Format::R8G8B8,
        gltf::image::Format::R8G8B8A8 => Format::R8G8B8A8,
        gltf::image::Format::B8G8R8 => Format::B8G8R8,
        gltf::image::Format::B8G8R8A8 => Format::B8G8R8A8,
        gltf::image::Format::R16 => Format::R16,
        gltf::image::Format::R16G16 => Format::R16G16,
        gltf::image::Format::R16G16B16 => Format::R16G16B16,
        gltf::image::Format::R16G16B16A16 => Format::R16G16B16A16,
    }
}

fn load_material(primitive_material: &gltf::Material) -> Result<Material> {
    let mut material = Material::default();
    material.name = primitive_material
        .name()
        .unwrap_or(DEFAULT_NAME)
        .to_string();
    let pbr = primitive_material.pbr_metallic_roughness();
    material.base_color_factor = glm::Vec4::from(pbr.base_color_factor());
    material.metallic_factor = pbr.metallic_factor();
    material.roughness_factor = pbr.roughness_factor();
    material.emissive_factor = glm::Vec3::from(primitive_material.emissive_factor());
    material.alpha_mode = map_gltf_alpha_mode(&primitive_material.alpha_mode());
    material.alpha_cutoff = primitive_material.alpha_cutoff();
    material.is_unlit = if primitive_material.unlit() { 1 } else { 0 };
    if let Some(base_color_texture) = pbr.base_color_texture() {
        material.color_texture_index = base_color_texture.texture().source().index() as i32;
        material.color_texture_set = base_color_texture.tex_coord() as i32;
    }
    if let Some(metallic_roughness_texture) = pbr.metallic_roughness_texture() {
        material.metallic_roughness_texture_index =
            metallic_roughness_texture.texture().source().index() as i32;
        material.metallic_roughness_texture_set = metallic_roughness_texture.tex_coord() as i32;
    }
    if let Some(normal_texture) = primitive_material.normal_texture() {
        material.normal_texture_index = normal_texture.texture().source().index() as i32;
        material.normal_texture_set = normal_texture.tex_coord() as i32;
        material.normal_texture_scale = normal_texture.scale();
    }
    if let Some(occlusion_texture) = primitive_material.occlusion_texture() {
        material.occlusion_texture_index = occlusion_texture.texture().source().index() as i32;
        material.occlusion_texture_set = occlusion_texture.tex_coord() as i32;
        material.occlusion_strength = occlusion_texture.strength();
    }
    if let Some(emissive_texture) = primitive_material.emissive_texture() {
        material.emissive_texture_index = emissive_texture.texture().source().index() as i32;
        material.emissive_texture_set = emissive_texture.tex_coord() as i32;
    }
    Ok(material)
}

fn map_gltf_alpha_mode(alpha_mode: &gltf::material::AlphaMode) -> AlphaMode {
    match alpha_mode {
        gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
        gltf::material::AlphaMode::Mask => AlphaMode::Mask,
        gltf::material::AlphaMode::Blend => AlphaMode::Blend,
    }
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
                camera: load_camera(&node)?,
                mesh: load_mesh(&node, buffers, &mut geometry)?,
                skin: load_skin(&node, buffers),
                light: load_light(&node),
            })
        })
        .collect::<Result<_>>()?;
    Ok((nodes, geometry))
}

fn load_camera(node: &gltf::Node) -> Result<Option<Camera>> {
    match node.camera() {
        Some(camera) => {
            let projection = match camera.projection() {
                gltf::camera::Projection::Perspective(camera) => {
                    Projection::Perspective(PerspectiveCamera {
                        aspect_ratio: camera.aspect_ratio(),
                        y_fov_deg: camera.yfov(),
                        z_far: camera.zfar(),
                        z_near: camera.znear(),
                    })
                }
                gltf::camera::Projection::Orthographic(camera) => {
                    Projection::Orthographic(OrthographicCamera {
                        x_mag: camera.xmag(),
                        y_mag: camera.ymag(),
                        z_far: camera.zfar(),
                        z_near: camera.znear(),
                    })
                }
            };
            log::info!("Loaded camera with projection: {:#?}", projection);
            Ok(Some(Camera {
                name: camera.name().unwrap_or(DEFAULT_NAME).to_string(),
                projection,
            }))
        }
        None => Ok(None),
    }
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
                .map(|primitive| load_primitive(&primitive, buffers, geometry))
                .collect::<Result<Vec<_>>>()?;
            let weights = match mesh.weights() {
                Some(weights) => Some(weights.to_vec()),
                None => None,
            };
            Ok(Some(Mesh {
                name: mesh.name().unwrap_or(DEFAULT_NAME).to_string(),
                primitives,
                weights,
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
    let number_of_indices = load_primitive_indices(primitive, buffers, geometry)?;
    let number_of_vertices = load_primitive_vertices(primitive, buffers, geometry)?;
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

    let mut positions = Vec::new();

    let read_positions = reader.read_positions().context(
        "Failed to read vertex positions from the model. Vertex positions are required.",
    )?;
    for position in read_positions {
        positions.push(glm::Vec3::from(position));
    }
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
            let _interpolation = map_gltf_interpolation(sampler.interpolation());
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
                    transformations = TransformationSet::MorphTargetWeights(morph_target_weights);
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

fn map_gltf_interpolation(interpolation: gltf::animation::Interpolation) -> Interpolation {
    match interpolation {
        gltf::animation::Interpolation::Linear => Interpolation::Linear,
        gltf::animation::Interpolation::Step => Interpolation::Step,
        gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
    }
}

fn load_materials(gltf: &gltf::Document, buffers: &[gltf::buffer::Data]) -> Result<Vec<Material>> {
    let mut materials = Vec::new();
    for material in gltf.materials() {
        materials.push(load_material(&material)?);
    }
    Ok(materials)
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

            let joints = load_joints(&skin, &inverse_bind_matrices);

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
            kind: map_gltf_light_kind(light.kind()),
        }),
        None => None,
    }
}

fn map_gltf_light_kind(light: gltf::khr_lights_punctual::Kind) -> LightKind {
    match light {
        gltf::khr_lights_punctual::Kind::Directional => LightKind::Directional,
        gltf::khr_lights_punctual::Kind::Point => LightKind::Point,
        gltf::khr_lights_punctual::Kind::Spot {
            inner_cone_angle,
            outer_cone_angle,
        } => LightKind::Spot {
            inner_cone_angle,
            outer_cone_angle,
        },
    }
}
