#version 450 core
precision highp float;

layout (location = 0) in vec3 a_pos;
layout (location = 1) in vec2 a_uv;

layout (location = 2) uniform mat4 u_projection;
layout (location = 3) uniform float lut_point;

layout (std430, binding = 0) buffer AnimLut {
    float z_pos[100];
};

out VS_OUT {
    vec2 uv;
} vs_out;

vec4 transform_mesh() {
    uint z_idx = uint(round(99.0 * lut_point));

    mat4 transform = mat4(
    vec4(1, 0, 0, 0),
    vec4(0, 1, 0, 0),
    vec4(0, 0, 1, 0),
    vec4(0, 0, z_pos[z_idx], 0)
    );

    return transform * vec4(a_pos, 1.0);
}

void main() {
    vs_out.uv = a_uv;
    gl_Position = u_projection * transform_mesh();
}