use anyhow::{bail, Context, Result};
use dragonglass_opengl::{gl, GeometryBuffer, ShaderProgram, Texture};
use dragonglass_world::{
    AlphaMode, EntityStore, Format, Material, MeshRender, RigidBody, Transform, World,
};
use nalgebra_glm as glm;
use std::{ptr, str};

pub struct WorldRender {
    pub geometry: GeometryBuffer,
    pub shader_program: ShaderProgram,
    pub textures: Vec<Texture>,
}

impl WorldRender {
    const VERTEX_SHADER_SOURCE: &'static str = &r#"
#version 450 core
layout (location = 0) in vec3 inPosition;
layout (location = 1) in vec3 inNormal;
layout (location = 2) in vec2 inUV0;
layout (location = 3) in vec2 inUV1;
layout (location = 4) in vec4 inJoint0;
layout (location = 5) in vec4 inWeight0;
layout (location = 6) in vec3 inColor0;

uniform mat4 view;
uniform mat4 projection;
uniform mat4 model;

out vec2 UV0;
out vec3 Color0;

void main()
{
   gl_Position = projection * view * model * vec4(inPosition, 1.0f);
   UV0 = inUV0;
   Color0 = inColor0;
}
"#;

    const FRAGMENT_SHADER_SOURCE: &'static str = &r#"
#version 450 core

struct Light
{
    vec3 direction;
    float range;

    vec3 color;
    float intensity;

    vec3 position;
    float innerConeCos;

    float outerConeCos;
    int kind;

    vec2 padding;
};

struct Material {
    vec4 baseColorFactor;
    vec4 emissiveFactor;
    int alphaMode;
    float alphaCutoff;
    float occlusionStrength;
    bool isUnlit;
    bool hasDiffuseTexture;
    bool hasPhysicalTexture;
    bool hasNormalTexture;
    bool hasOcclusionTexture;
    bool hasEmissiveTexture;
}; 

uniform Material material;

uniform sampler2D DiffuseTexture;
uniform sampler2D PhysicalTexture;
uniform sampler2D NormalTexture;
uniform sampler2D OcclusionTexture;
uniform sampler2D EmissiveTexture;

uniform vec3 cameraPosition;

in vec2 UV0;
in vec3 Color0;

out vec4 color;

vec4 srgb_to_linear(vec4 srgbIn)
{
    return vec4(pow(srgbIn.xyz,vec3(2.2)),srgbIn.w);
}

void main(void)
{
    color = material.baseColorFactor;
    if (material.hasDiffuseTexture) {
        vec4 albedoMap = texture(DiffuseTexture, UV0);
        color = srgb_to_linear(albedoMap);
    }

    color *= vec4(Color0, 1.0);

    // alpha discard
    if (material.alphaMode == 2 && color.a < material.alphaCutoff) {
        discard;
    }

    if (material.isUnlit) {
        color = vec4(pow(color.rgb, vec3(1.0 / 2.2)), color.a);
        return;
    }

    float occlusion = 1.0;
    if (material.hasOcclusionTexture) {
         occlusion = texture(OcclusionTexture, UV0).r;
    }
    color = mix(color, color * occlusion, material.occlusionStrength);

    vec4 emission = vec4(0.0);
    if (material.hasEmissiveTexture) {
        emission = srgb_to_linear(texture(EmissiveTexture, UV0)) * vec4(material.emissiveFactor.rgb, 1.0);
    }
    color += vec4(emission.rgb, 0.0);

    // HDR tonemapping
    color = color / (color + vec4(1.0));

    // gamma correct
    color = pow(color, vec4(1.0/2.2));
}
"#;

    pub fn new(world: &World) -> Result<Self> {
        let geometry = GeometryBuffer::new(
            &world.geometry.vertices,
            Some(&world.geometry.indices),
            &[3, 3, 2, 2, 4, 4, 3],
        );

        let mut shader_program = ShaderProgram::new();
        shader_program
            .vertex_shader_source(Self::VERTEX_SHADER_SOURCE)?
            .fragment_shader_source(Self::FRAGMENT_SHADER_SOURCE)?
            .link();

        let textures = world
            .textures
            .iter()
            .map(Self::map_world_texture)
            .collect::<Vec<_>>();

        Ok(Self {
            geometry,
            shader_program,
            textures,
        })
    }

    fn map_world_texture(
        world_texture: &dragonglass_world::Texture,
    ) -> dragonglass_opengl::Texture {
        let pixel_format = match world_texture.format {
            Format::R8 => gl::R8,
            Format::R8G8 => gl::RG,
            Format::R8G8B8 => gl::RGB,
            Format::R8G8B8A8 => gl::RGBA,
            Format::B8G8R8 => gl::BGR,
            Format::B8G8R8A8 => gl::BGRA,
            Format::R16 => gl::R16,
            Format::R16G16 => gl::RG16,
            Format::R16G16B16 => gl::RGB16,
            Format::R16G16B16A16 => gl::RGBA16,
        };

        let mut texture = Texture::new();
        texture.load_data(
            world_texture.width,
            world_texture.height,
            &world_texture.pixels,
            pixel_format,
        );
        texture
    }

    pub fn render(&self, world: &World, aspect_ratio: f32) -> Result<()> {
        unsafe {
            gl::Enable(gl::CULL_FACE);
            gl::CullFace(gl::BACK);
            gl::FrontFace(gl::CCW);

            gl::Enable(gl::DEPTH_TEST);
            gl::DepthFunc(gl::LEQUAL);
        }

        self.geometry.bind();
        self.shader_program.use_program();

        let (projection, view) = world.active_camera_matrices(aspect_ratio)?;
        let camera_entity = world.active_camera()?;
        let camera_transform = world.entity_global_transform(camera_entity)?;
        self.shader_program
            .set_uniform_vec3("cameraPosition", camera_transform.translation.as_slice());

        self.shader_program
            .set_uniform_matrix4x4("projection", projection.as_slice());
        self.shader_program
            .set_uniform_matrix4x4("view", view.as_slice());

        for alpha_mode in [AlphaMode::Opaque, AlphaMode::Mask, AlphaMode::Blend].iter() {
            for graph in world.scene.graphs.iter() {
                graph.walk(|node_index| {
                    let entity = graph[node_index];

                    let entry = world.ecs.entry_ref(entity)?;

                    // Render rigid bodies at the transform specified by the physics world instead of the scenegraph
                    // NOTE: The rigid body collider scaling should be the same as the scale of the entity transform
                    //       otherwise this won't look right. It's probably best to just not scale entities that have rigid bodies
                    //       with colliders on them.
                    let model = match entry.get_component::<RigidBody>() {
                        Ok(rigid_body) => {
                            let body = world
                                .physics
                                .bodies
                                .get(rigid_body.handle)
                                .context("Failed to acquire physics body to render!")?;
                            let position = body.position();
                            let translation = position.translation.vector;
                            let rotation = *position.rotation.quaternion();
                            let scale =
                                Transform::from(world.global_transform(graph, node_index)?).scale;
                            Transform::new(translation, rotation, scale).matrix()
                        }
                        Err(_) => world.global_transform(graph, node_index)?,
                    };

                    self.shader_program
                        .set_uniform_matrix4x4("model", model.as_slice());

                    match world.ecs.entry_ref(entity)?.get_component::<MeshRender>() {
                        Ok(mesh_render) => {
                            if let Some(mesh) = world.geometry.meshes.get(&mesh_render.name) {
                                match alpha_mode {
                                    AlphaMode::Opaque | AlphaMode::Mask => unsafe {
                                        gl::Disable(gl::BLEND);
                                    },
                                    AlphaMode::Blend => unsafe {
                                        gl::Enable(gl::BLEND);
                                        gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
                                    },
                                }

                                for primitive in mesh.primitives.iter() {
                                    let material = match primitive.material_index {
                                        Some(material_index) => {
                                            let primitive_material =
                                                world.material_at_index(material_index)?;
                                            if primitive_material.alpha_mode != *alpha_mode {
                                                continue;
                                            }
                                            primitive_material.clone()
                                        }
                                        None => Material::default(),
                                    };

                                    self.shader_program.set_uniform_vec4(
                                        "material.baseColorFactor",
                                        material.base_color_factor.as_slice(),
                                    );

                                    self.shader_program.set_uniform_vec4(
                                        "material.emissiveFactor",
                                        glm::vec3_to_vec4(&material.emissive_factor).as_slice(),
                                    );

                                    self.shader_program.set_uniform_int(
                                        "material.alphaMode",
                                        material.alpha_mode as _,
                                    );

                                    self.shader_program.set_uniform_float(
                                        "material.alphaCutoff",
                                        material.alpha_cutoff,
                                    );

                                    self.shader_program.set_uniform_float(
                                        "material.occlusionStrength",
                                        material.occlusion_strength,
                                    );

                                    self.shader_program
                                        .set_uniform_bool("material.isUnlit", material.is_unlit);

                                    for (index, descriptor) in
                                        ["Diffuse", "Physical", "Normal", "Occlusion", "Emissive"]
                                            .iter()
                                            .enumerate()
                                    {
                                        let texture_index = match *descriptor {
                                            "Diffuse" => material.color_texture_index,
                                            "Physical" => material.metallic_roughness_texture_index,
                                            "Normal" => material.normal_texture_index,
                                            "Occlusion" => material.occlusion_texture_index,
                                            "Emissive" => material.emissive_texture_index,
                                            _ => bail!("Failed to find index for texture type!"),
                                        };
                                        let has_texture = texture_index > -1;

                                        self.shader_program.set_uniform_bool(
                                            &format!("material.has{}Texture", *descriptor),
                                            has_texture,
                                        );

                                        self.shader_program.set_uniform_int(
                                            &format!("{}Texture", *descriptor),
                                            index as _,
                                        );

                                        if has_texture {
                                            self.textures[texture_index as usize].bind(index as _);
                                        }
                                    }

                                    let ptr: *const u8 = ptr::null_mut();
                                    let ptr = unsafe {
                                        ptr.add(primitive.first_index * std::mem::size_of::<u32>())
                                    };
                                    unsafe {
                                        gl::DrawElements(
                                            gl::TRIANGLES,
                                            primitive.number_of_indices as _,
                                            gl::UNSIGNED_INT,
                                            ptr as *const _,
                                        );
                                    }
                                }
                            }
                        }
                        Err(_) => return Ok(()),
                    }

                    Ok(())
                })?;
            }
        }

        Ok(())
    }
}
