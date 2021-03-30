// Adapted from: https://github.com/SaschaWillems/Vulkan-glTF-PBR/blob/master/data/shaders/pbr_khr.frag

#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_ARB_shading_language_420pack : enable

layout (location = 0) in vec3 inWorldPos;
layout (location = 1) in vec3 inNormal;
layout (location = 2) in vec2 inUV0;
layout (location = 3) in vec2 inUV1;
layout (location = 4) in vec3 inColor0;

#define MAX_NUMBER_OF_TEXTURES 200

layout(binding = 2) uniform sampler2D textures[MAX_NUMBER_OF_TEXTURES];
layout(binding = 3) uniform sampler2D brdflut;
layout(binding = 4) uniform samplerCube prefilter;
layout(binding = 5) uniform samplerCube irradiance;

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

const float M_PI = 3.141592653589793;
const float minRoughness = 0.04;
const float EmissiveFactor = 1.0f;
const float Gamma = 2.2f;
const float Exposure = 4.5f;

vec3 Uncharted2Tonemap(vec3 color)
{
	float A = 0.15;
	float B = 0.50;
	float C = 0.10;
	float D = 0.20;
	float E = 0.02;
	float F = 0.30;
	float W = 11.2;
	return ((color*(A*color+C*B)+D*E)/(color*(A*color+B)+D*F))-E/F;
}

vec4 tonemap(vec4 color)
{
	vec3 outcol = Uncharted2Tonemap(color.rgb * Exposure);
	outcol = outcol * (1.0f / Uncharted2Tonemap(vec3(11.2f)));
	return vec4(pow(outcol, vec3(1.0f / Gamma)), color.a);
}

// Find the normal for this fragment, pulling either from a predefined normal map
// or from the interpolated mesh normal and tangent attributes.
vec3 getNormal()
{
    if (material.normalTextureSet <= -1) {
        return normalize(inNormal);
    }

    vec3 normal = inNormal;
    if (material.normalTextureIndex > -1) {
        vec2 tex_coord = inUV0;
        if(material.normalTextureSet == 1) {
            tex_coord = inUV1;
        }
        normal = texture(textures[material.normalTextureSet], tex_coord).xyz;
    } 

	// Perturb normal, see http://www.thetenthplanet.de/archives/1180
	vec3 tangentNormal = normal * 2.0 - 1.0;

	vec3 q1 = dFdx(inWorldPos);
	vec3 q2 = dFdy(inWorldPos);
	vec2 st1 = dFdx(inUV0);
	vec2 st2 = dFdy(inUV0);

	vec3 N = normalize(inNormal);
	vec3 T = normalize(q1 * st2.t - q2 * st1.t);
	vec3 B = -normalize(cross(N, T));
	mat3 TBN = mat3(T, B, N);

	return normalize(TBN * tangentNormal);
}

vec4 srgb_to_linear(vec4 srgbIn)
{
  vec3 linOut = pow(srgbIn.xyz,vec3(2.2));
  return vec4(linOut,srgbIn.w);;
}

const int LightKind_Directional = 0;
const int LightKind_Point = 1;
const int LightKind_Spot = 2;

float getRangeAttenuation(float range, float distance)
{
    if (range <= 0.0)
    {
        // negative range means unlimited
        return 1.0;
    }
    return max(min(1.0 - pow(distance / range, 4.0), 1.0), 0.0) / pow(distance, 2.0);
}

float getSpotAttenuation(vec3 pointToLight, vec3 spotDirection, float outerConeCos, float innerConeCos)
{
    float actualCos = dot(normalize(spotDirection), normalize(-pointToLight));
    if (actualCos > outerConeCos)
    {
        if (actualCos < innerConeCos)
        {
            return smoothstep(outerConeCos, innerConeCos, actualCos);
        }
        return 1.0;
    }
    return 0.0;
}

void main()
{
    float perceptualRoughness;
    float metallic;
    vec3 diffuseColor;

    vec3 f0 = vec3(0.04);

    // Albedo
    vec4 baseColor;
    if (material.colorTextureIndex > -1) {
        vec2 tex_coord = inUV0;
        if(material.colorTextureSet == 1) {
            tex_coord = inUV1;
        }
        vec4 albedoMap = texture(textures[material.colorTextureIndex], tex_coord);
        baseColor = srgb_to_linear(albedoMap) * material.baseColorFactor * vec4(inColor0, 1.0);
    } else {
        baseColor = material.baseColorFactor * vec4(inColor0, 1.0);
    }

    if (material.alphaMode == 2 && baseColor.a < material.alphaCutoff) {
        discard;
    }

    // Unlit models
    if (material.isUnlit == 1) {
        outColor = vec4(pow(baseColor.rgb, vec3(1.0 / 2.2)), baseColor.a);
        return;
    }

    float minRoughness = 1.0;
    perceptualRoughness = material.roughnessFactor;
    metallic = material.metallicFactor;
    if (material.metallicRoughnessTextureSet > -1)
    {
        vec4 physicalDescriptor = texture(textures[material.metallicRoughnessTextureSet], inUV0);
        perceptualRoughness = physicalDescriptor.g * perceptualRoughness;
        metallic = physicalDescriptor.b * metallic;
    } else {
        perceptualRoughness = clamp(perceptualRoughness, minRoughness, 1.0);
        metallic = clamp(metallic, 0.0, 1.0);
    }

    diffuseColor = baseColor.rgb * (vec3(1.0) - f0);
    diffuseColor *= 1.0 - metallic;

    float alphaRoughness = perceptualRoughness * perceptualRoughness;

    vec3 specularColor = mix(f0, baseColor.rgb, metallic);

    float reflectance = max(max(specularColor.r, specularColor.g), specularColor.b);

    float reflectance90 = clamp(reflectance * 25.0, 0.0, 1.0);
    vec3 specularEnvironmentR0 = specularColor.rgb;
    vec3 specularEnvironmentR90 = vec3(1.0, 1.0, 1.0) * reflectance90;

    vec3 n = getNormal();
    vec3 v = normalize(uboView.cameraPosition.xyz - inWorldPos); // Vector from surface point to camera
    float NdotV = clamp(abs(dot(n, v)), 0.001, 1.0);

    vec3 color = vec3(0.0, 0.0, 0.0);

    for (int i = 0; i < uboView.numberOfLights; i++) {
        Light light = uboView.lights[i];

        vec3 pointToLight = -light.direction;
        float rangeAttenuation = 1.0;
        float spotAttenuation = 1.0;

        if(light.kind != LightKind_Directional)
        {
            pointToLight = light.position - inWorldPos;
        }

        // Compute range and spot light attenuation.
        if (light.kind != LightKind_Directional)
        {
            rangeAttenuation = getRangeAttenuation(light.range, length(pointToLight));
        }
        if (light.kind == LightKind_Spot)
        {
            spotAttenuation = getSpotAttenuation(pointToLight, light.direction, light.outerConeCos, light.innerConeCos);
        }

        vec3 intensity = rangeAttenuation * spotAttenuation * light.intensity * light.color;

        vec3 l = normalize(pointToLight); // Vector from surface point to light
        vec3 h = normalize(l+v);          // Half vector between both l and v

        float NdotL = clamp(dot(n, l), 0.001, 1.0);
        float NdotH = clamp(dot(n, h), 0.0, 1.0);
        float LdotH = clamp(dot(l, h), 0.0, 1.0);
        float VdotH = clamp(dot(v, h), 0.0, 1.0);

        // Calculate the shading terms for the microfacet specular shading model

        // The following equation models the Fresnel reflectance term of the spec equation (aka F())
        // Implementation of fresnel from [4], Equation 15
        vec3 F = specularEnvironmentR0 + (specularEnvironmentR90 - specularEnvironmentR0) * pow(clamp(1.0 - VdotH, 0.0, 1.0), 5.0);

        // This calculates the specular geometric attenuation (aka G()),
        // where rougher material will reflect less light back to the viewer.
        // This implementation is based on [1] Equation 4, and we adopt their modifications to
        // alphaRoughness as input as originally proposed in [2].
        float r = alphaRoughness;
        float attenuationL = 2.0 * NdotL / (NdotL + sqrt(r * r + (1.0 - r * r) * (NdotL * NdotL)));
        float attenuationV = 2.0 * NdotV / (NdotV + sqrt(r * r + (1.0 - r * r) * (NdotV * NdotV)));
        float G = attenuationL * attenuationV;

        // The following equation(s) model the distribution of microfacet normals across the area being drawn (aka D())
        // Implementation from "Average Irregularity Representation of a Roughened Surface for Ray Reflection" by T. S. Trowbridge, and K. P. Reitz
        // Follows the distribution function recommended in the SIGGRAPH 2013 course notes from EPIC Games [1], Equation 3.
        float roughnessSq = alphaRoughness * alphaRoughness;
        float f = (NdotH * roughnessSq - NdotH) * NdotH + 1.0;
        float D = roughnessSq / (M_PI * f * f);

        vec3 diffuseContrib = (1.0 - F) * diffuseColor / M_PI;
        vec3 specContrib = F * G * D / (4.0 * NdotL * NdotV);
        color += NdotL * intensity * (diffuseContrib + specContrib);
    }

    // retrieve a scale and bias to F0
    float prefilterMipLevels = 10; // mip_levels for a 512x512px cubemap face
    float lod = (perceptualRoughness * prefilterMipLevels);
    vec3 brdf = (texture(brdflut, vec2(NdotV, 1.0 - perceptualRoughness))).rgb;

    vec3 diffuseLight = srgb_to_linear(tonemap(texture(irradiance, n))).rgb;
    vec3 diffuse = diffuseLight * diffuseColor;

    vec3 reflection = -normalize(reflect(v, n));
    reflection.y *= -1.0f;

    vec3 specularLight = srgb_to_linear(tonemap(textureLod(prefilter, reflection, lod))).rgb;
    vec3 specular = specularLight * (specularColor * brdf.x + brdf.y);

    color += diffuse + specular;

    // Occlusion map
    if (material.occlusionTextureIndex > -1) {
        vec2 tex_coord = inUV0;
        if(material.occlusionTextureSet == 1) {
            tex_coord = inUV1;
        }
        vec4 occlusionMap = texture(textures[material.occlusionTextureIndex], tex_coord);
        color = mix(color, color * occlusionMap.r, material.occlusionStrength);
    }

    // Emissive map
    if (material.emissiveTextureIndex > -1) {
        vec2 tex_coord = inUV0;
        if(material.emissiveTextureSet == 1) {
            tex_coord = inUV1;
        }
        vec4 emissiveMap = texture(textures[material.emissiveTextureIndex], tex_coord);
        color += srgb_to_linear(emissiveMap).rgb * material.emissiveFactor;
    }

    outColor = vec4(color, baseColor.a);
}