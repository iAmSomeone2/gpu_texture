#version 450 core
precision highp float;

in VS_OUT {
    vec2 uv;
} fs_in;

uniform sampler2D u_texture;

out vec4 frag_color;

void main() {
    vec4 tex_color = texture(u_texture, fs_in.uv);
    if (tex_color.a <= 0.01) {
        discard;
    }
    frag_color = tex_color;
}