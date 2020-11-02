#version 450


layout(location=0) in vec3 inPosition;
layout(location=1) in vec3 inNormal;
layout(location=2) in vec2 inUV0;
layout(location=3) in vec2 inUV1;
layout(location=4) in vec2 inJoint0;
layout(location=5) in vec2 inWeight0;
layout(location=6) in vec3 inColor0;

layout(binding=0) uniform UboView{
  mat4 view;
  mat4 projection;
  vec3 cameraPosition;
} uboView;

layout(binding=1) uniform UboInstance{
  mat4 model;
} uboInstance;

layout(location=0) out vec3 outPosition;
layout(location=1) out vec3 outNormal;
layout(location=2) out vec2 outUV0;
layout(location=3) out vec2 outUV1;
layout(location=4) out vec3 outColor0;

void main()
{
  mat4 mvp = uboView.projection * uboView.view * uboInstance.model;
  vec4 position = mvp * vec4(inPosition, 1.0);
  gl_Position = position;

  outPosition = position.xyz; 
  outNormal = mat3(mvp) * inNormal;
  outUV0 = inUV0;
  outUV1 = inUV1;
  outColor0 = inColor0;
}