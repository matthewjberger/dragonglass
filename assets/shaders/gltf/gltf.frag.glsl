#version 450

layout(location=0) in vec3 inPosition;
layout(location=1) in vec3 inNormal;
layout(location=2) in vec2 inUV0;
layout(location=3) in vec2 inUV1;
layout(location=4) in vec3 inColor0;

#define MAX_NUMBER_OF_TEXTURES 200

layout(binding=2) uniform sampler2D textures[MAX_NUMBER_OF_TEXTURES];

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
    int kind;
    vec3 position;
    float innerConeCos;
    float outerConeCos;
    vec3 padding;
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
    vec3 linOut = pow(srgbIn.xyz,vec3(2.2));
    return vec4(linOut,srgbIn.w);
}

void main()
{
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

    if (material.isUnlit == 1) {
        outColor = vec4(pow(baseColor.rgb, vec3(1.0 / 2.2)), baseColor.a);
        return;
    }

    vec3 normal = normalize(inNormal);
    vec3 view = normalize(uboView.cameraPosition - inPosition);

    vec3 color = vec3(0.0);
    /* Blinn-Phong Shading */
    for (int i = 0; i < uboView.numberOfLights; i++) {
        // Treat all lights as directional lights for now

        Light light = uboView.lights[i];
        vec3 lightDir = normalize(-light.direction);

        vec3 ambient = 0.05 * light.color * baseColor.rgb;

        float diff = max(dot(normal, lightDir), 0.0);
        vec3 diffuse = diff * light.color * baseColor.rgb;

        vec3 halfway = normalize(view + lightDir);
        vec3 specular = vec3(0.0);
        if (diff > 0.0) {
            float shine = 32.0;
            float spec = pow(max(dot(halfway, normal), 0.0), shine);
            specular = light.color * spec;
        }

        color += ambient + diffuse + specular;
    }

    if (material.occlusionTextureIndex > -1) {
        vec2 tex_coord = inUV0;
        if(material.occlusionTextureSet == 1) {
            tex_coord = inUV1;
        }
        vec4 occlusionMap = texture(textures[material.occlusionTextureIndex], tex_coord);
        color = mix(color, color * occlusionMap.r, material.occlusionStrength);
    }

    if (material.emissiveTextureIndex > -1) {
        vec2 tex_coord = inUV0;
        if(material.emissiveTextureSet == 1) {
            tex_coord = inUV1;
        }
        vec4 emissiveMap = texture(textures[material.emissiveTextureIndex], tex_coord);
        color += srgb_to_linear(emissiveMap).rgb * material.emissiveFactor;
    }
  
    color = pow(color, vec3(1.0 / 2.2));
    outColor = vec4(color, baseColor.a);
}