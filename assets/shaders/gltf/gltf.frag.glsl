#version 450

#define MAX_NUMBER_OF_TEXTURES 100

layout(location=0)in vec2 inUV0;

layout(binding=2)uniform sampler2D textures[MAX_NUMBER_OF_TEXTURES];

layout(push_constant)uniform Material{
    vec4 baseColorFactor;
    vec3 emissiveFactor;
    int colorTextureSet;
    int metallicRoughnessTextureSet;
    int normalTextureSet;
    int occlusionTextureSet;
    int emissiveTextureSet;
    float metallicFactor;
    float roughnessFactor;
    int alphaMode;
    float alphaCutoff;
}material;

layout(location=0)out vec4 outColor;

layout(binding=0)uniform UboView{
    mat4 view;
    mat4 projection;
}uboView;

vec4 srgb_to_linear(vec4 srgbIn)
{
    vec3 linOut=pow(srgbIn.xyz,vec3(2.2));
    return vec4(linOut,srgbIn.w);;
}

void main()
{
    vec4 baseColor;
    
    if(material.colorTextureSet>-1){
        vec4 albedoMap=texture(textures[material.colorTextureSet],inUV0);
        baseColor=srgb_to_linear(albedoMap)*material.baseColorFactor;
    }else{
        baseColor=material.baseColorFactor;
    }
    
    if(material.alphaMode==2 && baseColor.a<material.alphaCutoff){
        discard;
    }
  
    outColor=baseColor;
}