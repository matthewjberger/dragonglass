use anyhow::{bail, Context, Result};
use dragonglass_opengl::{gl, GeometryBuffer, ShaderProgram, Texture};
use dragonglass_world::{
    AlphaMode, EntityStore, Format, LightKind, Material, MeshRender, RigidBody, Transform, World,
};
use nalgebra_glm as glm;
use std::{ptr, str};

// TODO: This is duplicated in the vulkan backend and should be moved
#[derive(Default, Debug, Copy, Clone)]
pub struct Light {
    pub direction: glm::Vec3,
    pub range: f32,

    pub color: glm::Vec3,
    pub intensity: f32,

    pub position: glm::Vec3,
    pub inner_cone_cos: f32,

    pub outer_cone_cos: f32,
    pub kind: i32,

    pub padding: glm::Vec2,
}

impl Light {
    pub fn from_node(transform: &Transform, light: &dragonglass_world::Light) -> Self {
        let mut inner_cone_cos: f32 = 0.0;
        let mut outer_cone_cos: f32 = 0.0;
        let kind = match light.kind {
            LightKind::Directional => 0,
            LightKind::Point => 1,
            LightKind::Spot {
                inner_cone_angle,
                outer_cone_angle,
            } => {
                inner_cone_cos = inner_cone_angle;
                outer_cone_cos = outer_cone_angle;
                2
            }
        };
        Self {
            direction: -1.0 * glm::quat_rotate_vec3(&transform.rotation, &glm::Vec3::z()),
            range: light.range,
            color: light.color,
            intensity: light.intensity,
            position: transform.translation,
            inner_cone_cos,
            outer_cone_cos,
            kind,
            padding: glm::vec2(0.0, 0.0),
        }
    }
}

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

out vec3 Position;
out vec2 UV0;
out vec3 Normal;
out vec3 Color0;

void main()
{
   Position = vec3(model * vec4(inPosition, 1.0));
   gl_Position = projection * view * vec4(Position, 1.0);
   UV0 = inUV0;
   Normal = mat3(model) * inNormal;
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

#define MAX_NUMBER_OF_LIGHTS 4
uniform Light lights[MAX_NUMBER_OF_LIGHTS];
uniform int numberOfLights;

struct Material {
    vec4 baseColorFactor;
    vec4 emissiveFactor;
    int alphaMode;
    float alphaCutoff;
    float occlusionStrength;
    float metallicFactor;
    float roughnessFactor;
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

in vec3 Position;
in vec2 UV0;
in vec3 Normal;
in vec3 Color0;

layout (location = 0) out vec4 color;
layout (location = 1) out vec4 brightColor;

vec4 srgb_to_linear(vec4 srgbIn)
{
    return vec4(pow(srgbIn.xyz,vec3(2.2)),srgbIn.w);
}

const float PI = 3.14159265359;

vec3 getNormal();
float DistributionGGX(vec3 N, vec3 H, float roughness);
float GeometrySchlickGGX(float NdotV, float roughness);
float GeometrySmith(vec3 N, vec3 V, vec3 L, float roughness);
vec3 fresnelSchlick(float cosTheta, vec3 F0);

void main(void)
{
    color = material.baseColorFactor;

    if (material.hasDiffuseTexture) {
        vec4 albedoMap = texture(DiffuseTexture, UV0);
        color = srgb_to_linear(albedoMap);
    }

    color *= vec4(Color0, 1.0);

    float baseAlpha = color.a;

    // alpha discard
    if (material.alphaMode == 2 && color.a < material.alphaCutoff) {
        discard;
    }

    if (material.isUnlit) {
        color = vec4(pow(color.rgb, vec3(1.0 / 2.2)), color.a);
        return;
    }

    float metallic = material.metallicFactor;
    float roughness = material.roughnessFactor;
    if (material.hasPhysicalTexture)
    {
        vec4 physicalDescriptor = texture(PhysicalTexture, UV0);
        roughness *= physicalDescriptor.g;
        metallic *= physicalDescriptor.b;
    }

    // calculate reflectance at normal incidence; if dia-electric (like plastic) use F0 
    // of 0.04 and if it's a metal, use the albedo color as F0 (metallic workflow)    
    vec3 F0 = vec3(0.04); 
    F0 = mix(F0, color.rgb, metallic);

    vec3 N = getNormal();
    vec3 V = normalize(cameraPosition - Position);
    vec3 R = reflect(-V, N); 

    // reflectance equation
    vec3 Lo = vec3(0.0);
    for(int i = 0; i < numberOfLights; ++i) 
    {
        Light light = lights[i];

        vec3 L = normalize(light.position - Position);
        vec3 H = normalize(V + L);
        float distance = length(light.position - Position);
        float attenuation = 1.0 / (distance * distance);
        vec3 radiance = light.color * attenuation;

        float NDF = DistributionGGX(N, H, roughness);
        float G = GeometrySmith(N, V, L, roughness);
        vec3 F = fresnelSchlick(max(dot(H, V), 0.0), F0);

        vec3 nominator = NDF * G * F;
        float denominator = 4 * max(dot(N, V), 0.0) * max(dot(N, L), 0.0) + 0.001;
        vec3 specular = nominator / denominator;

        vec3 kS = F;
        vec3 kD = vec3(1.0) - kS;
        kD *= 1.0 - metallic;

        float NdotL = max(dot(N, L), 0.0);

        Lo += (kD * color.rgb / PI + specular) * radiance * NdotL;
    }

    color *= vec4(0.3);
    color += vec4(Lo, 0.0);

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
    color.a = baseAlpha;

    float brightness = dot(color.rgb, vec3(0.2126, 0.7152, 0.0722));
    if (brightness > 1.0)
    {
        brightColor = vec4(color.rgb, 1.0);
    }
    else {
        brightColor = vec4(0.0, 0.0, 0.0, 1.0);
    }
}

vec3 getNormal()
{
    if (!material.hasNormalTexture) {
        return Normal;
    }

    vec3 tangentNormal = texture(NormalTexture, UV0).xyz * 2.0 - 1.0;

    vec3 Q1  = dFdx(Position);
    vec3 Q2  = dFdy(Position);
    vec2 st1 = dFdx(UV0);
    vec2 st2 = dFdy(UV0);

    vec3 N   = normalize(Normal);
    vec3 T  = normalize(Q1*st2.t - Q2*st1.t);
    vec3 B  = -normalize(cross(N, T));
    mat3 TBN = mat3(T, B, N);

    return normalize(TBN * tangentNormal);
}

float DistributionGGX(vec3 N, vec3 H, float roughness)
{
    float a = roughness*roughness;
    float a2 = a*a;
    float NdotH = max(dot(N, H), 0.0);
    float NdotH2 = NdotH*NdotH;

    float nom   = a2;
    float denom = (NdotH2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;

    return nom / denom;
}

float GeometrySchlickGGX(float NdotV, float roughness)
{
    float r = (roughness + 1.0);
    float k = (r*r) / 8.0;

    float nom   = NdotV;
    float denom = NdotV * (1.0 - k) + k;

    return nom / denom;
}

float GeometrySmith(vec3 N, vec3 V, vec3 L, float roughness)
{
    float NdotV = max(dot(N, V), 0.0);
    float NdotL = max(dot(N, L), 0.0);
    float ggx2 = GeometrySchlickGGX(NdotV, roughness);
    float ggx1 = GeometrySchlickGGX(NdotL, roughness);

    return ggx1 * ggx2;
}

vec3 fresnelSchlick(float cosTheta, vec3 F0)
{
    return F0 + (1.0 - F0) * pow(max(1.0 - cosTheta, 0.0), 5.0);
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
            Format::R16F => gl::R16F,
            Format::R16G16F => gl::RG16F,
            Format::R16G16B16F => gl::RGB16F,
            Format::R16G16B16A16F => gl::RGBA16F,
            Format::R32 => gl::R32UI,
            Format::R32G32 => gl::RG32UI,
            Format::R32G32B32 => gl::RGB32UI,
            Format::R32G32B32A32 => gl::RGBA32UI,
            Format::R32F => gl::R32F,
            Format::R32G32F => gl::RG32F,
            Format::R32G32B32F => gl::RGB32F,
            Format::R32G32B32A32F => gl::RGBA32F,
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

        let world_lights = world
            .lights()?
            .iter()
            .map(|(transform, light)| Light::from_node(transform, light))
            .collect::<Vec<_>>();
        for (index, light) in world_lights.iter().enumerate() {
            let name = |key: &str| format!("lights[{}].{}", index, key);

            self.shader_program
                .set_uniform_vec3(&name("direction"), light.direction.as_slice());
            self.shader_program
                .set_uniform_float(&name("range"), light.range);
            self.shader_program
                .set_uniform_vec3(&name("color"), light.color.as_slice());
            self.shader_program
                .set_uniform_float(&name("intensity"), light.intensity);
            self.shader_program
                .set_uniform_vec3(&name("position"), light.position.as_slice());
            self.shader_program
                .set_uniform_float(&name("innerConeCos"), light.inner_cone_cos);
            self.shader_program
                .set_uniform_float(&name("outerConeCos"), light.outer_cone_cos);
            self.shader_program
                .set_uniform_int(&name("kind"), light.kind);
        }

        self.shader_program
            .set_uniform_int("numberOfLights", world_lights.len() as _);

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

                                    self.shader_program.set_uniform_float(
                                        "material.metallicFactor",
                                        material.metallic_factor,
                                    );

                                    self.shader_program.set_uniform_float(
                                        "material.roughnessFactor",
                                        material.roughness_factor,
                                    );

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
