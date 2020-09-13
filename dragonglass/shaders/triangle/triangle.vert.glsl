#version 450

layout(location = 0) in vec2 inPosition;
layout(location = 1) in vec3 inColor;

layout(binding = 0) uniform UniformBuffer {
  mat4 view;
  mat4 projection;
  mat4 model;
} uniformBuffer;

layout(location = 0) out vec3 fragColor;

void main() {
    mat4 mvp = uniformBuffer.projection * uniformBuffer.view * uniformBuffer.model;
    gl_Position = mvp * vec4(inPosition, 0.0, 1.0);
    fragColor = inColor;
}
