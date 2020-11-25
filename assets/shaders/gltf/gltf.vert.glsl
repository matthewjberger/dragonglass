#version 450


layout(location=0) in vec3 inPosition;
layout(location=1) in vec3 inNormal;
layout(location=2) in vec2 inUV0;
layout(location=3) in vec2 inUV1;
layout(location=4) in vec4 inJoint0;
layout(location=5) in vec4 inWeight0;
layout(location=6) in vec3 inColor0;

#define MAX_NUMBER_OF_JOINTS 200
#define MAX_NUMBER_OF_MORPH_TARGETS 128
#define MAX_NUMBER_OF_MORPH_WEIGHTS 128

layout(binding=0) uniform UboView{
  mat4 view;
  mat4 projection;
  vec4 cameraPosition;
  mat4 jointMatrices[MAX_NUMBER_OF_JOINTS];
  vec4 morphTargets[MAX_NUMBER_OF_MORPH_TARGETS];
  float morphTargetWeights[MAX_NUMBER_OF_MORPH_WEIGHTS];
} uboView;

layout(binding=1) uniform UboInstance{
  mat4 model;
  vec4 node_info;
} uboInstance;

layout(location=0) out vec3 outPosition;
layout(location=1) out vec3 outNormal;
layout(location=2) out vec2 outUV0;
layout(location=3) out vec2 outUV1;
layout(location=4) out vec3 outColor0;

void main()
{
  float jointCount = uboInstance.node_info.x;
  float jointOffset = uboInstance.node_info.y;
  float morphWeightCount = uboInstance.node_info.z;
  float morphWeightOffset = uboInstance.node_info.w;

  mat4 skinMatrix = mat4(1.0);
  if (jointCount > 0.0) {
    skinMatrix =
      inWeight0.x * uboView.jointMatrices[int(inJoint0.x + jointOffset)] +
      inWeight0.y * uboView.jointMatrices[int(inJoint0.y + jointOffset)] +
      inWeight0.z * uboView.jointMatrices[int(inJoint0.z + jointOffset)] +
      inWeight0.w * uboView.jointMatrices[int(inJoint0.w + jointOffset)];
  }
  mat4 skinnedModel = uboInstance.model * skinMatrix;

  gl_Position = uboView.projection * uboView.view * skinnedModel * vec4(inPosition, 1.0);

  outPosition = inPosition; 
  outNormal = normalize(transpose(inverse(mat3(skinnedModel))) * inNormal);
  outUV0 = inUV0;
  outUV1 = inUV1;
  outColor0 = inColor0;
}