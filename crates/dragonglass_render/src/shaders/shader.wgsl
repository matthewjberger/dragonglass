[[block]]
struct WorldUniform {
    view: mat4x4<f32>;
    projection: mat4x4<f32>;
}

[[block]]
struct EntityUniform {
    model: mat4x4<f32>;
}

[[group(0), binding(0)]]
var<uniform> worldUniform: WorldUniform;

[[group(1), binding(0)]]
var<uniform> entityUniform: EntityUniform;

struct VertexInput {
    [[location(0)]] position: vec3<f32>;
    [[location(1)]] normal: vec3<f32>;
    [[location(2)]] uv_0: vec2<f32>;
    [[location(3)]] uv_0: vec2<f32>;
    [[location(4)]] joint_0: vec4<f32>;
    [[location(5)]] weight_0: vec4<f32>;
    [[location(6)]] color_0: vec3<f32>;
};

struct VertexOutput {
    [[builtin(position)]] clip_position: vec4<f32>;
    [[location(0)]] color: vec3<f32>;
};

[[stage(vertex)]]
fn main(
    model: VertexInput
) -> VertexOutput {
    var out: VertexOutput;
    out.color = model.color_0;
    out.clip_position = vec4<f32>(worldUniform.projection * worldUniform.view * entityUniform.model * model.position, 1.0);
    return out;
}
 
[[stage(fragment)]]
fn main(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}