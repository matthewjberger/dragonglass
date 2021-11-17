// Vertex shader

[[block]]
struct Uniform {
    view: mat4x4<f32>;
    projection: mat4x4<f32>;
};
[[group(0), binding(0)]]
var<uniform> ubo: Uniform;

[[block]]
struct DynamicUniform {
    model: mat4x4<f32>;
};
[[group(1), binding(0)]]
var<uniform> mesh_ubo: DynamicUniform;

struct VertexInput {
    [[location(0)]] position: vec3<f32>;
    [[location(1)]] normal: vec3<f32>;
    [[location(2)]] uv_0: vec2<f32>;
    [[location(3)]] uv_1: vec2<f32>;
    [[location(4)]] joint_0: vec4<f32>;
    [[location(5)]] weight_0: vec4<f32>;
    [[location(6)]] color_0: vec3<f32>;
};

struct VertexOutput {
    [[builtin(position)]] clip_position: vec4<f32>;
    [[location(0)]] color: vec3<f32>;
    [[location(1)]] uv: vec2<f32>;
};

[[stage(vertex)]]
fn vs_main(
    vertex: VertexInput,
) -> VertexOutput {
    var output: VertexOutput;
    output.color = vertex.color_0;
    output.uv = vertex.uv_0;
    output.clip_position = ubo.projection * ubo.view * vec4<f32>(vertex.position, 1.0);
    return output;
}

[[stage(fragment)]]
fn fs_main(vertex: VertexOutput) -> [[location(0)]] vec4<f32> {
    // return vec4<f32>(vertex.color, 1.0);
    return vec4<f32>(vec3<f32>(vertex.uv, 1.0), 1.0);
}