#version 450

layout(location = 0) in vec2 inUV;

layout(binding = 0) uniform sampler2D color;

layout(location = 0) out vec4 outColor;

void main() {
    // Chromatic Aberration
    /* float strength = 10.0; */
    /* vec2 uv = inUV; */
    /* vec2 texel = 1.0 / vec2(800.0, 600.0); */
    /* vec2 coords = (uv - 0.5) * 2.0; */
    /* float coordDot = dot(coords, coords); */
    /* vec2 precompute = strength * coordDot * coords; */
    /* vec2 uvR = uv - texel.xy * precompute; */
    /* vec2 uvB = uv + texel.xy * precompute; */
    /* vec4 newColor; */
    /* newColor.r = texture(color, uvR).r; */
    /* newColor.g = texture(color, uv).g; */
    /* newColor.b = texture(color, uvB).b; */
    /* outColor = newColor; */

    outColor = texture(color, inUV);
}
