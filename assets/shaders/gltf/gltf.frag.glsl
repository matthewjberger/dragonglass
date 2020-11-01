#version 450

#define MAX_NUMBER_OF_TEXTURES 100

layout(location=0) in vec2 inUV0;
layout(location=1) in vec2 inUV1;

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
    int occlusionTextureIndex;
    int occlusionTextureSet;
    int emissiveTextureIndex;
    int emissiveTextureSet;
    float metallicFactor;
    float roughnessFactor;
    int alphaMode;
    float alphaCutoff;
    bool isUnlit;
} material;

layout(location = 0) out vec4 outColor;

layout(binding = 0) uniform UboView{
    mat4 view;
    mat4 projection;
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
        baseColor = srgb_to_linear(albedoMap) * material.baseColorFactor;
    } else {
        baseColor = material.baseColorFactor;
    }

    if (material.isUnlit) {
        outColor = baseColor;
        return;
    }
    
    if (material.alphaMode == 2 && baseColor.a < material.alphaCutoff) {
        discard;
    }

    vec3 color = baseColor.rgb;
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