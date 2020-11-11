#version 450

layout (location = 0) in vec3 inPosition;

layout(push_constant) uniform PushConstants{
  mat4 view;
  mat4 projection;
} pushConstants;

layout(location = 0) out vec3 vert_texcoord;

void main()
{
  gl_Position = pushConstants.projection * mat4(mat3(pushConstants.view)) * vec4(inPosition, 1.0);
  vert_texcoord = inPosition;
}