#version 450

layout(location = 0) in vec2 inUV;

layout(binding = 0) uniform sampler2D color;
layout(binding = 1) uniform Ubo{
  int time;
} ubo;


layout(location = 0) out vec4 outColor;

void main() {
    vec2 uv = inUV;
    vec4 newColor = texture(color, inUV);

    // Chromatic Aberration
    // float strength = 10.0;
    // vec2 texel = 1.0 / vec2(800.0, 600.0);
    // vec2 coords = (uv - 0.5) * 2.0;
    // float coordDot = dot(coords, coords);
    // vec2 precompute = strength * coordDot * coords;
    // vec2 uvR = uv - texel.xy * precompute;
    // vec2 uvB = uv + texel.xy * precompute;
    // newColor.r = texture(color, uvR).r;
    // newColor.g = texture(color, uv).g;
    // newColor.b = texture(color, uvB).b;

    // Film grain
    float filmGrainStrength = 10.0;
    float x = (uv.x + 4.0 ) * (uv.y + 4.0 ) * (ubo.time);
	  vec4 grain = vec4(mod((mod(x, 13.0) + 1.0) * (mod(x, 123.0) + 1.0), 0.01)-0.005) * filmGrainStrength;
    newColor += grain;
    
    outColor = newColor;

    // outColor = texture(color, inUV);
}
