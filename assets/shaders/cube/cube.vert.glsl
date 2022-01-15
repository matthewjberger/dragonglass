#version 450

layout (location = 0) in vec3 inPosition;

layout(push_constant) uniform PushConstants{
  mat4 mvp;
  vec4 color;
} pushConstants;

layout (location = 0) out vec4 outColor;

void main()
{
  gl_Position = pushConstants.mvp * vec4(inPosition, 1.0);
  outColor = pushConstants.color;
}