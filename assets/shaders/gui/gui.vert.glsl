#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec2 inPosition;
layout(location = 1) in vec2 inTexCoord;
layout(location = 2) in vec4 inColor;

layout(push_constant) uniform PushConstants {
  mat4 orthographic;
} pushConstants;

layout(location = 0) out vec4 outColor;
layout(location = 1) out vec2 outTexCoord;

void main() {
  outColor = inColor;
  outTexCoord = inTexCoord;

  gl_Position = pushConstants.orthographic * vec4(inPosition.x, inPosition.y, 0.0, 1.0);
}