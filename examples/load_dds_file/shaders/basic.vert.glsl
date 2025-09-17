#version 410 core
precision mediump float;

layout (location = 0) in vec3 aPos;
layout (location = 1) in vec2 aTexCoords;

uniform mat4 u_transformMatrix;

out vec2 v_uv;

void main() {
    gl_Position = u_transformMatrix * vec4(aPos, 1.0);
    v_uv = aTexCoords;
}