#version 450 core
precision highp float;

layout (location = 0) in vec3 a_pos;
layout (location = 1) in vec2 a_uv;

layout (location = 2) uniform mat4 u_projection;
layout (location = 3) uniform mat4 u_transform;

out VS_OUT {
    vec2 uv;
} vs_out;

void main() {
    vs_out.uv = a_uv;
    gl_Position = u_projection * u_transform * vec4(a_pos, 1.0);
}