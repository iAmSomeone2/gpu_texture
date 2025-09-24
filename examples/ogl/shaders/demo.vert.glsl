#version 460 core
/**
 * 3D position read from the vertex buffer bound to the active Vertex Attribute Object.
 */
layout (location = 0) in vec3 a_pos;
/** 2D texture coordinate read from the vertex buffer bound to the active Vertex Attribute Object. */
layout (location = 1) in vec2 a_uv;

/** Normalized point to sample the LUT from */
layout (location = 2) uniform float lut_point;

/** Uniform buffer object used for storing uniform data that stays static during a frame */
layout (std140, binding = 0) uniform FrameUniforms {
    mat4 u_projection;
};

///**
// * Pre-computed animation lookup table
// */
//layout (std430, binding = 1) readonly buffer AnimationLUT {
//    uint count;
//    float data[];
//} anim_lut;

/**
 * Pre-computed animation lookup table as a 1D texture
 */
layout (location = 3) uniform sampler1D u_animation_lut;

/**
 * Shader Output Block. Used for grouping all shader outputs under a single data structure.
 *
 * Note: A matching input block is required in the next stage (fragment).
 */
out VS_OUT {
    vec2 uv;
} vs_out;

//float get_z_pos() {
//    float exact_idx = float(anim_lut.count - 1u) * lut_point;
//    uint lower_idx = uint(exact_idx);
//    float mix_ratio = exact_idx - float(lower_idx);
//
//    if (lower_idx >= anim_lut.count - 1u) {
//        return anim_lut.data[anim_lut.count - 1];
//    }
//
//    float lower_val = anim_lut.data[lower_idx];
//    float upper_val = anim_lut.data[lower_idx + 1];
//
//    return mix(lower_val, upper_val, mix_ratio);
//}

void main() {
    vs_out.uv = a_uv;
    float z_pos = texture(u_animation_lut, lut_point).r;

    vec3 offset_pos = vec3(a_pos.xy, -z_pos);
    gl_Position = u_projection * vec4(offset_pos, 1.0);
}