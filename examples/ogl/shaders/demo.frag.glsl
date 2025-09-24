#version 460 core

in VS_OUT {
    vec2 uv;
} fs_in;

layout (location = 4) uniform sampler2D u_texture;

out vec4 frag_color;

void main() {
    vec4 tex_color = texture(u_texture, fs_in.uv);

    if (tex_color.a <= 0.01) {
        // Totally discards a fragment/pixel if the alpha value falls below the threshold (0.01)
        discard;
    }

    frag_color = tex_color;
}