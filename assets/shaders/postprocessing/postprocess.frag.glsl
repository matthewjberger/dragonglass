#version 450

layout(location = 0) in vec2 inUV;

layout(binding = 0) uniform sampler2D color;
layout(binding = 1) uniform Ubo {
    int time;
    float screen_width;
    float screen_height;
    float chromatic_aberration_strength;
    float film_grain_strength;
} settings;

layout(location = 0) out vec4 outColor;

void main() {
    vec2 uv = inUV / vec2(settings.screen_width, settings.screen_height);

    vec3 col;
    col.r = texture(color,vec2(uv.x+0.003,-uv.y)).x;
    // col.g = texture(color,vec2(uv.x+0.000,-uv.y)).y;
    // col.b = texture(color,vec2(uv.x-0.003,-uv.y)).z;
    // col = clamp(col*0.5+0.5*col*col*1.2,0.0,1.0);
    // col *= 0.5 + 0.5*16.0*uv.x*uv.y*(1.0-uv.x)*(1.0-uv.y);
    // col *= vec3(0.95,1.05,0.95);
    // col *= 0.9+0.1*sin(10.0*settings.time+uv.y*1000.0);
    // col *= 0.99+0.01*sin(110.0*settings.time);

    outColor = vec4(col, 1.0);
}
