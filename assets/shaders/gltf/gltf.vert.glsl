#version 450

layout(location=0)in vec3 inPosition;
layout(location=1)in vec3 inNormal;
layout(location=2)in vec2 inUV0;

layout(binding=0)uniform UboView{
  mat4 view;
  mat4 projection;
}assetUbo;

layout(binding=1)uniform UboInstance{
  mat4 model;
}modelUbo;

layout(location=0)out vec2 outTexCoord;

void main(){
  mat4 mvp=assetUbo.projection*assetUbo.view*modelUbo.model;
  gl_Position=mvp*vec4(inPosition,0.,1.);
  outTexCoord=inTexCoord;
}
