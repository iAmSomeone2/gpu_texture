#version 450 core
precision mediump float;

in VS_OUT {
    vec2 uv;
    vec3 color;
} fs_in;

uniform sampler2D u_texture;

out vec4 fragColor;

void main() {
    fragColor = vec4(fs_in.uv, 0.0, 1.0);
}