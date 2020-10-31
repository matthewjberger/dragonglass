#version 450


layout(location=0) in vec3 inPos;
layout(location=1) in vec3 inNormal;
layout(location=2) in vec2 inUV0;
layout(location=3) in vec2 inUV1;
layout(location=4) in vec2 inJoints0;
layout(location=5) in vec2 inWeights0;

layout(binding=0) uniform UboView{
  mat4 view;
  mat4 projection;
} uboView;

layout(binding=1) uniform UboInstance{
  mat4 model;
} uboInstance;

layout(location=0) out vec2 outUV0;

void main()
{
  vec4 locPos = uboInstance.model * vec4(inPos, 1.0);
  locPos.y = -locPos.y;
  gl_Position = uboView.projection * uboView.view * locPos;
  outUV0 = inUV0;
}