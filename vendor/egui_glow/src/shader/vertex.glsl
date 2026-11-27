#if NEW_SHADER_INTERFACE
    #define I in
    #define O out
    #define V(x) x
#else
    #define I attribute
    #define O varying
    #define V(x) vec3(x)
#endif

#ifdef GL_ES
    // RustDo: highp (was mediump). Mobile GPUs honour mediump as 16-bit floats, too
    // coarse for texture coordinates in a large font atlas: letters came out
    // clipped and shifted in the web app on Android tablets.
    precision highp float;
#endif

uniform vec2 u_screen_size;
I vec2 a_pos;
I vec4 a_srgba; // 0-255 sRGB
I vec2 a_tc;
O vec4 v_rgba_in_gamma;
O vec2 v_tc;

void main() {
    gl_Position = vec4(
                      2.0 * a_pos.x / u_screen_size.x - 1.0,
                      1.0 - 2.0 * a_pos.y / u_screen_size.y,
                      0.0,
                      1.0);
    v_rgba_in_gamma = a_srgba / 255.0;
    v_tc = a_tc;
}
