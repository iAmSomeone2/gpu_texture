#version 410 core
precision mediump float;

/** Per-instance transformation */
layout (location = 0) in mat4 transform;

//layout (std140) uniform TransformBlock {
//    uint count;
//    mat4 matrix[];
//} transforms;

layout (location = 0) out vec2 v_uv;

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
    v_uv = uv[gl_VertexID];
    gl_Position = transform * vec4(pos[gl_VertexID], 1.0, 1.0);
}