#version 450 core
precision mediump float;

layout (location = 0) uniform uint count;

layout (std430, binding = 0) buffer InstanceData {
    mat4 transform[];
};

out VS_OUT {
    vec2 uv;
    vec3 color;
} vs_out;

const vec2 pos[4] = vec2[](
vec2(-1, -1),
vec2(1, -1),
vec2(-1, 1),
vec2(1, 1)
);

const vec2 uv[4] = vec2[](
vec2(0, 0),
vec2(1, 0),
vec2(0, 1),
vec2(1, 1)
);

void main() {
    vs_out.uv = uv[gl_VertexID];
    float idx_pct = gl_InstanceID / count;
    vs_out.color = vec3(idx_pct, 0.0, 1.0);
    gl_Position = transform[gl_InstanceID] * vec4(pos[gl_VertexID], 1.0, 1.0);
}