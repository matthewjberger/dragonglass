use crate::{
    AlphaMode, Animation, BoundingBox, Camera, Channel, Ecs, Entity, Filter, Format, Geometry,
    Interpolation, Joint, Light, LightKind, Material, Mesh, MeshRender, MorphTarget, Name,
    OrthographicCamera, PerspectiveCamera, Primitive, Projection, Sampler, Scene, SceneGraph, Skin,
    Texture, Transform, TransformationSet, Vertex, World, WrappingMode,
};
use anyhow::{Context, Result};
use gltf::animation::util::ReadOutputs;
use nalgebra_glm as glm;
use petgraph::prelude::*;
use std::path::Path;

pub fn create_scene_graph(node: &gltf::Node, ecs: &mut Ecs, entities: &[Entity]) -> SceneGraph {
    let mut node_graph = SceneGraph::new();
    graph_node(&mut node_graph, node, NodeIndex::new(0), ecs, entities);
    node_graph
}

pub fn graph_node(
    graph: &mut SceneGraph,
    gltf_node: &gltf::Node,
    parent_index: NodeIndex,
    ecs: &mut Ecs,
    entities: &[Entity],
) {
    let entity = entities[gltf_node.index()];
    let index = graph.add_node(entity);
    if parent_index != index {
        graph.add_edge(parent_index, index);
    }
    for child in gltf_node.children() {
        graph_node(graph, &child, index, ecs, entities);
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

pub fn load_gltf(path: impl AsRef<Path>, world: &mut World, ecs: &mut Ecs) -> Result<()> {
    let (gltf, buffers, images) = gltf::import(path)?;

    let number_of_materials = world.materials.len();

    let number_of_textures = world.textures.len();
    let mut materials = load_materials(&gltf)?;
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
        .for_each(|material| world.materials.push(material));

    load_textures(&gltf, &images)?
        .into_iter()
        .for_each(|texture| world.textures.push(texture));

    let entities = ecs
        .spawn_batch((0..gltf.nodes().len()).map(|_| ()))
        .collect::<Vec<_>>();

    load_animations(&gltf, &buffers, &entities)?
        .into_iter()
        .for_each(|node| world.animations.push(node));

    load_nodes(&gltf, &buffers, ecs, &mut world.geometry, &entities)?;

    entities.iter().for_each(|entity| {
        if let Ok(mut mesh) = ecs.get_mut::<Mesh>(*entity) {
            mesh.primitives.iter_mut().for_each(|primitive| {
                if let Some(material_index) = primitive.material_index.as_mut() {
                    *material_index += number_of_materials
                }
            })
        }
    });

    // Only merge default scene
    let new_scenes = load_scenes(&gltf, ecs, &entities);
    if let Some(new_scene) = new_scenes.into_iter().next() {
        new_scene.graphs.into_iter().for_each(|graph| {
            world.scene.graphs.push(graph);
        });
    }

    Ok(())
}

fn load_samplers(document: &gltf::Document) -> Vec<Sampler> {
    document.samplers().map(map_gltf_sampler).collect()
}

fn map_gltf_sampler(sampler: gltf::texture::Sampler) -> Sampler {
    let mut min_filter = Filter::Linear;
    if let Some(min) = sampler.min_filter() {
        min_filter = match min {
            gltf::texture::MinFilter::Linear
            | gltf::texture::MinFilter::LinearMipmapLinear
            | gltf::texture::MinFilter::LinearMipmapNearest => Filter::Linear,
            gltf::texture::MinFilter::Nearest
            | gltf::texture::MinFilter::NearestMipmapLinear
            | gltf::texture::MinFilter::NearestMipmapNearest => Filter::Nearest,
        };
    }

    let mut mag_filter = Filter::Linear;
    if let Some(mag) = sampler.mag_filter() {
        mag_filter = match mag {
            gltf::texture::MagFilter::Nearest => Filter::Nearest,
            gltf::texture::MagFilter::Linear => Filter::Linear,
        };
    }

    let wrap_s = match sampler.wrap_s() {
        gltf::texture::WrappingMode::ClampToEdge => WrappingMode::ClampToEdge,
        gltf::texture::WrappingMode::MirroredRepeat => WrappingMode::MirroredRepeat,
        gltf::texture::WrappingMode::Repeat => WrappingMode::Repeat,
    };

    let wrap_t = match sampler.wrap_t() {
        gltf::texture::WrappingMode::ClampToEdge => WrappingMode::ClampToEdge,
        gltf::texture::WrappingMode::MirroredRepeat => WrappingMode::MirroredRepeat,
        gltf::texture::WrappingMode::Repeat => WrappingMode::Repeat,
    };

    Sampler {
        name: sampler.name().unwrap_or(DEFAULT_NAME).to_string(),
        min_filter,
        mag_filter,
        wrap_s,
        wrap_t,
    }
}

fn load_textures(gltf: &gltf::Document, images: &[gltf::image::Data]) -> Result<Vec<Texture>> {
    let samplers = load_samplers(gltf);
    let mut textures = Vec::new();
    for texture in gltf.textures() {
        let sampler_error_message = "Failed to lookup sampler specified by texture!";
        let sampler = match texture.sampler().index() {
            Some(sampler_index) => samplers
                .get(sampler_index)
                .context(sampler_error_message)?
                .clone(),
            None => Sampler::default(),
        };

        let image_error_message = "Failed to lookup sampler specified by texture!";
        let image_index = texture.source().index();
        let image = images.get(image_index).context(image_error_message)?;

        let texture = Texture {
            pixels: image.pixels.to_vec(),
            format: map_gltf_format(image.format),
            width: image.width,
            height: image.height,
            sampler,
        };
        textures.push(texture);
    }
    Ok(textures)
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
    material.is_unlit = primitive_material.unlit();
    if let Some(base_color_texture) = pbr.base_color_texture() {
        material.color_texture_index = base_color_texture.texture().index() as i32;
        material.color_texture_set = base_color_texture.tex_coord() as i32;
    }
    if let Some(metallic_roughness_texture) = pbr.metallic_roughness_texture() {
        material.metallic_roughness_texture_index =
            metallic_roughness_texture.texture().index() as i32;
        material.metallic_roughness_texture_set = metallic_roughness_texture.tex_coord() as i32;
    }
    if let Some(normal_texture) = primitive_material.normal_texture() {
        material.normal_texture_index = normal_texture.texture().index() as i32;
        material.normal_texture_set = normal_texture.tex_coord() as i32;
        material.normal_texture_scale = normal_texture.scale();
    }
    if let Some(occlusion_texture) = primitive_material.occlusion_texture() {
        material.occlusion_texture_index = occlusion_texture.texture().index() as i32;
        material.occlusion_texture_set = occlusion_texture.tex_coord() as i32;
        material.occlusion_strength = occlusion_texture.strength();
    }
    if let Some(emissive_texture) = primitive_material.emissive_texture() {
        material.emissive_texture_index = emissive_texture.texture().index() as i32;
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

fn load_scenes(gltf: &gltf::Document, ecs: &mut Ecs, entities: &[Entity]) -> Vec<Scene> {
    gltf.scenes()
        .map(|scene| Scene {
            name: scene.name().unwrap_or(DEFAULT_NAME).to_string(),
            graphs: scene
                .nodes()
                .map(|node| create_scene_graph(&node, ecs, entities))
                .collect(),
        })
        .collect::<Vec<_>>()
}

fn load_nodes(
    gltf: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    ecs: &mut Ecs,
    geometry: &mut Geometry,
    entities: &[Entity],
) -> Result<()> {
    for (index, node) in gltf.nodes().enumerate() {
        let entity = entities[index];

        let name = node.name().unwrap_or(DEFAULT_NAME).to_string();

        ecs.insert(entity, (Name(name),))?;

        ecs.insert(entity, (node_transform(&node),))?;

        if let Some(camera) = node.camera() {
            ecs.insert(entity, (load_camera(&camera)?,))?;
        }

        if let Some(gltf_mesh) = node.mesh() {
            let mesh = load_mesh(&gltf_mesh, buffers, geometry)?;
            let name = if geometry.meshes.contains_key(&mesh.name) {
                // FIXME: increment a name with a number
                //        instead of just adding an underscore
                let name = mesh.name.to_string();
                name + "_"
            } else {
                mesh.name.to_string()
            };
            if geometry.meshes.insert(name.clone(), mesh).is_some() {}
            ecs.insert(entity, (MeshRender { name },))?;
        }

        if let Some(skin) = node.skin() {
            ecs.insert(entity, (load_skin(&skin, buffers, entities),))?;
        }

        if let Some(light) = node.light() {
            ecs.insert(entity, (load_light(&light),))?;
        }
    }

    Ok(())
}

fn load_camera(camera: &gltf::Camera) -> Result<Camera> {
    let projection = match camera.projection() {
        gltf::camera::Projection::Perspective(camera) => {
            Projection::Perspective(PerspectiveCamera {
                aspect_ratio: camera.aspect_ratio(),
                y_fov_rad: camera.yfov(),
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
    Ok(Camera {
        name: camera.name().unwrap_or(DEFAULT_NAME).to_string(),
        projection,
        enabled: false,
    })
}

fn load_mesh(
    mesh: &gltf::Mesh,
    buffers: &[gltf::buffer::Data],
    geometry: &mut Geometry,
) -> Result<Mesh> {
    let primitives = mesh
        .primitives()
        .map(|primitive| load_primitive(&primitive, buffers, geometry))
        .collect::<Result<Vec<_>>>()?;
    let weights = match mesh.weights() {
        Some(weights) => weights.to_vec(),
        None => Vec::new(),
    };
    Ok(Mesh {
        name: mesh.name().unwrap_or(DEFAULT_NAME).to_string(),
        primitives,
        weights,
    })
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
    let bounding_box = primitive.bounding_box();
    let morph_targets = load_morph_targets(primitive, buffers)?;
    let bounding_box = BoundingBox::new(
        glm::Vec3::from(bounding_box.min),
        glm::Vec3::from(bounding_box.max),
    );
    Ok(Primitive {
        first_index,
        first_vertex,
        number_of_indices,
        number_of_vertices,
        morph_targets,
        material_index: primitive.material().index(),
        bounding_box,
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

fn load_morph_targets(
    primitive: &gltf::Primitive,
    buffers: &[gltf::buffer::Data],
) -> Result<Vec<MorphTarget>> {
    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

    let mut morph_targets = Vec::new();
    for (mut position_displacements, mut normal_displacements, mut tangent_displacements) in
        reader.read_morph_targets()
    {
        let positions = match position_displacements.as_mut() {
            Some(position_displacements) => position_displacements
                .map(glm::Vec3::from)
                .map(|v| glm::vec3_to_vec4(&v))
                .collect::<Vec<_>>(),
            None => Vec::new(),
        };

        let normals = match normal_displacements.as_mut() {
            Some(normal_displacements) => normal_displacements
                .map(glm::Vec3::from)
                .map(|v| glm::vec3_to_vec4(&v))
                .collect::<Vec<_>>(),
            None => Vec::new(),
        };

        let tangents = match tangent_displacements.as_mut() {
            Some(tangent_displacements) => tangent_displacements
                .map(glm::Vec3::from)
                .map(|v| glm::vec3_to_vec4(&v))
                .collect::<Vec<_>>(),
            None => Vec::new(),
        };

        let morph_target = MorphTarget {
            positions,
            normals,
            tangents,
        };
        morph_targets.push(morph_target);
    }

    Ok(morph_targets)
}

fn load_animations(
    gltf: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    entities: &[Entity],
) -> Result<Vec<Animation>> {
    let mut animations = Vec::new();
    for animation in gltf.animations() {
        let name = animation.name().unwrap_or(DEFAULT_NAME).to_string();
        let mut channels = Vec::new();
        for channel in animation.channels() {
            let sampler = channel.sampler();
            let _interpolation = map_gltf_interpolation(sampler.interpolation());
            let target_node = channel.target().node().index();
            let target = entities[target_node];
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
                target,
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

fn load_materials(gltf: &gltf::Document) -> Result<Vec<Material>> {
    let mut materials = Vec::new();
    for material in gltf.materials() {
        materials.push(load_material(&material)?);
    }
    Ok(materials)
}

fn load_skin(skin: &gltf::Skin, buffers: &[gltf::buffer::Data], entities: &[Entity]) -> Skin {
    let reader = skin.reader(|buffer| Some(&buffers[buffer.index()]));
    let inverse_bind_matrices = reader
        .read_inverse_bind_matrices()
        .map_or(Vec::new(), |matrices| {
            matrices.map(glm::Mat4::from).collect::<Vec<_>>()
        });
    let joints = load_joints(skin, &inverse_bind_matrices, entities);
    let name = skin.name().unwrap_or(DEFAULT_NAME).to_string();
    Skin { joints, name }
}

fn load_joints(
    skin: &gltf::Skin,
    inverse_bind_matrices: &[glm::Mat4],
    entities: &[Entity],
) -> Vec<Joint> {
    skin.joints()
        .enumerate()
        .map(|(index, joint_node)| {
            let inverse_bind_matrix = *inverse_bind_matrices
                .get(index)
                .unwrap_or(&glm::Mat4::identity());
            Joint {
                inverse_bind_matrix,
                target: entities[joint_node.index()],
            }
        })
        .collect()
}

fn load_light(light: &gltf::khr_lights_punctual::Light) -> Light {
    Light {
        color: glm::make_vec3(&light.color()),
        intensity: light.intensity(),
        range: light.range().unwrap_or(-1.0), // if no range is present, range is assumed to be infinite
        kind: map_gltf_light_kind(light.kind()),
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
