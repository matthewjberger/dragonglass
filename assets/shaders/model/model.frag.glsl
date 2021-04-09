    #version 450

layout(location=0) in vec3 inPosition;
layout(location=1) in vec3 inNormal;
layout(location=2) in vec2 inUV0;
layout(location=3) in vec2 inUV1;
layout(location=4) in vec3 inColor0;

#define MAX_NUMBER_OF_TEXTURES 200

layout(binding=2) uniform sampler2D textures[MAX_NUMBER_OF_TEXTURES];
layout(binding=3) uniform sampler2D brdflut;
layout(binding=4) uniform samplerCube prefilterMap;
layout(binding=5) uniform samplerCube irradianceMap;

layout(push_constant) uniform Material{
    vec4 baseColorFactor;
    vec3 emissiveFactor;
    int colorTextureIndex;
    int colorTextureSet;
    int metallicRoughnessTextureIndex;
    int metallicRoughnessTextureSet;
    int normalTextureIndex;
    int normalTextureSet;
    float normalTextureScale;
    int occlusionTextureIndex;
    int occlusionTextureSet;
    float occlusionStrength;
    int emissiveTextureIndex;
    int emissiveTextureSet;
    float metallicFactor;
    float roughnessFactor;
    int alphaMode;
    float alphaCutoff;
    int isUnlit;
} material;

layout(location = 0) out vec4 outColor;

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
#define MAX_NUMBER_OF_JOINTS 200

layout(binding=0) uniform UboView{
  mat4 view;
  mat4 projection;
  vec3 cameraPosition;
  int numberOfLights;
  mat4 jointMatrices[MAX_NUMBER_OF_JOINTS];
  Light lights[MAX_NUMBER_OF_LIGHTS];
} uboView;

vec4 srgb_to_linear(vec4 srgbIn)
{
    return vec4(pow(srgbIn.xyz,vec3(2.2)),srgbIn.w);
}

const int LightType_Directional = 0;
const int LightType_Point = 1;
const int LightType_Spot = 2;

const float PI = 3.14159265359;

vec3 getNormal()
{
    if (material.normalTextureIndex <= -1) {
        return normalize(inNormal);
    }

    vec2 tex_coord = inUV0;
    if(material.normalTextureSet == 1) {
        tex_coord = inUV1;
    }

    // Perturb normal, see http://www.thetenthplanet.de/archives/1180
	vec3 tangentNormal = texture(textures[material.normalTextureIndex], tex_coord).xyz * 2.0 - 1.0;

	vec3 q1 = dFdx(inPosition);
	vec3 q2 = dFdy(inPosition);
	vec2 st1 = dFdx(tex_coord);
	vec2 st2 = dFdy(tex_coord);

	vec3 N = normalize(inNormal);
	vec3 T = normalize(q1 * st2.t - q2 * st1.t);
	vec3 B = -normalize(cross(N, T));
	mat3 TBN = mat3(T, B, N);

	return normalize(TBN * tangentNormal) * vec3(vec2(material.normalTextureScale), 1.0);
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

vec3 fresnelSchlickRoughness(float cosTheta, vec3 F0, float roughness)
{
    return F0 + (max(vec3(1.0 - roughness), F0) - F0) * pow(max(1.0 - cosTheta, 0.0), 5.0);
}   

void main()
{
    // base color
    vec4 baseColor = material.baseColorFactor;
    if (material.colorTextureIndex > -1) {
        vec2 tex_coord = inUV0;
        if(material.colorTextureSet == 1) {
            tex_coord = inUV1;
        }
        vec4 albedoMap = texture(textures[material.colorTextureIndex], tex_coord);
        baseColor *= srgb_to_linear(albedoMap);
    }

    vec3 albedo = baseColor.rgb * inColor0;

    // alpha discard
    if (material.alphaMode == 2 && baseColor.a < material.alphaCutoff) {
        discard;
    }

    // unlit
    if (material.isUnlit == 1) {
        outColor = vec4(pow(albedo, vec3(1.0 / 2.2)), material.baseColorFactor.a);
        return;
    }

    // metallic
    float metallic = material.metallicFactor;
    float roughness = material.roughnessFactor;
    if (material.metallicRoughnessTextureIndex > -1)
    {
        vec2 tex_coord = inUV0;
        if(material.metallicRoughnessTextureSet == 1) {
            tex_coord = inUV1;
        }
        vec4 physicalDescriptor = texture(textures[material.metallicRoughnessTextureIndex], tex_coord);
        roughness *= physicalDescriptor.g;
        metallic *= physicalDescriptor.b;
    }

    // Occlusion
    float occlusion = 1.0;
    if (material.occlusionTextureIndex > -1) {
        vec2 tex_coord = inUV0;
        if(material.occlusionTextureSet == 1) {
            tex_coord = inUV1;
        }
        occlusion = texture(textures[material.occlusionTextureIndex], tex_coord).r;
    }

    // Emissive texture
    vec3 emission = vec3(0.0);
    if (material.emissiveTextureIndex > -1) {
        vec2 tex_coord = inUV0;
        if(material.emissiveTextureSet == 1) {
            tex_coord = inUV1;
        }
        emission = srgb_to_linear(texture(textures[material.emissiveTextureIndex], tex_coord)).rgb * material.emissiveFactor;
    }

    vec3 N = getNormal();
    vec3 V = normalize(uboView.cameraPosition - inPosition);
    vec3 R = reflect(-V, N); 

    // calculate reflectance at normal incidence; if dia-electric (like plastic) use F0
    // of 0.04 and if it's a metal, use the albedo color as F0 (metallic workflow)
    vec3 F0 = vec3(0.04);
    F0 = mix(F0, albedo, metallic);

    // reflectance equation
    vec3 Lo = vec3(0.0);
    for(int i = 0; i < uboView.numberOfLights; ++i)
    {
        Light light = uboView.lights[i];

        // calculate per-light radiance
        vec3 L = normalize(light.position - inPosition);
        vec3 H = normalize(V + L);
        float distance = length(light.position - inPosition);
        float attenuation = 1.0 / (distance * distance);
        vec3 radiance = light.color * attenuation;

        // Cook-Torrance BRDF
        float NDF = DistributionGGX(N, H, roughness);
        float G   = GeometrySmith(N, V, L, roughness);
        vec3 F    = fresnelSchlick(max(dot(H, V), 0.0), F0);

        vec3 numerator    = NDF * G * F;
        float denominator = 4 * max(dot(N, V), 0.0) * max(dot(N, L), 0.0) + 0.001; // 0.001 to prevent divide by zero.
        vec3 specular = numerator / denominator;

        // kS is equal to Fresnel
        vec3 kS = F;
        // for energy conservation, the diffuse and specular light can't
        // be above 1.0 (unless the surface emits light); to preserve this
        // relationship the diffuse component (kD) should equal 1.0 - kS.
        vec3 kD = vec3(1.0) - kS;
        // multiply kD by the inverse metalness such that only non-metals
        // have diffuse lighting, or a linear blend if partly metal (pure metals
        // have no diffuse light).
        kD *= 1.0 - metallic;

        // scale light by NdotL
        float NdotL = max(dot(N, L), 0.0);

        // add to outgoing radiance Lo
        Lo += (kD * albedo / PI + specular) * radiance * NdotL;  // note that we already multiplied the BRDF by the Fresnel (kS) so we won't multiply by kS again
    }

    // IBL
    vec3 F = fresnelSchlickRoughness(max(dot(N, V), 0.0), F0, roughness);
    vec3 kS = F;
    vec3 kD = 1.0 - kS;
    kD *= 1.0 - metallic;	  
    
    vec3 irradiance = srgb_to_linear(texture(irradianceMap, N)).rgb;
    vec3 diffuse      = irradiance * albedo;
    
    // sample both the pre-filter map and the BRDF lut and combine them together as per the Split-Sum approximation to get the IBL specular part.
    const float MAX_REFLECTION_LOD = 4.0;
    vec3 prefilteredColor = srgb_to_linear(textureLod(prefilterMap, R,  roughness * MAX_REFLECTION_LOD)).rgb;    
    vec2 brdf  = texture(brdflut, vec2(max(dot(N, V), 0.0), roughness)).rg;
    vec3 specular = prefilteredColor * (F * brdf.x + brdf.y);

    vec3 ambient = kD * diffuse + specular;

    // occlusion
    ambient = mix(ambient, ambient * occlusion, material.occlusionStrength);

    vec3 color = ambient + Lo;

    // emission
    color += emission;

    // HDR tonemapping
    color = color / (color + vec3(1.0));

    // gamma correct
    color = pow(color, vec3(1.0/2.2)); 

    outColor = vec4(color, baseColor.a);
}