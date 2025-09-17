#version 410 core
precision mediump float;

in vec2 v_uv;

uniform bool u_useTexture;
uniform vec4 u_color;
uniform sampler2D u_texture;

out vec4 fragColor;

void main() {
    // Use the provided color if it's set; otherwise, sample from the texture
    if (u_useTexture) {
        fragColor = texture(u_texture, v_uv);
    } else {
        fragColor = u_color;
    }
}