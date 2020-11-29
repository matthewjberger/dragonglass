#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec4 inColor;
layout(location = 1) in vec2 inTexCoord;

layout(binding = 0, set = 0) uniform sampler2D fontsSampler;

layout(location = 0) out vec4 outColor;

void main() {
  outColor = inColor * texture(fontsSampler, inTexCoord);
}