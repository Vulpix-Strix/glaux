#version 330

// CRT look for Glaux, applied while the logical frame is drawn into the
// intermediate render target. Uses raylib's default vertex shader, so the
// inputs below follow raylib's default fragment interface.

in vec2 fragTexCoord;
in vec4 fragColor;

uniform sampler2D texture0;
uniform vec4 colDiffuse;

// Logical frame size in texels.
uniform vec2 sourceSize;

out vec4 finalColor;

// Barrel distortion. X bends with vertical position and Y with horizontal
// position; kept low so text in the one-cell margin stays inside the tube.
// Pointer hit-testing mirrors `warp` and this value (`CRT_CURVATURE` in
// pointer.rs); change them together.
const vec2 CURVATURE = vec2(0.025, 0.035);
// Brightness lost between logical rows, at full depth for dark pixels.
const float SCANLINE_DEPTH = 0.30;
// Share of each pixel taken from its horizontal neighbours.
const float SOFTNESS = 0.20;
// Attenuation of the two dimmer channels in each RGB mask triad.
const float MASK_STRENGTH = 0.14;
const float VIGNETTE = 0.20;
// Width of the fade at the tube edge, in tube UV units.
const float EDGE_SOFTNESS = 0.006;
// Overall gain that offsets scanline and mask darkening.
const float GAIN = 1.12;

const float PI = 3.14159265;

vec2 warp(vec2 uv) {
    vec2 c = uv * 2.0 - 1.0;
    c *= vec2(1.0 + c.y * c.y * CURVATURE.x, 1.0 + c.x * c.x * CURVATURE.y);
    return c * 0.5 + 0.5;
}

vec3 fetch(vec2 uv) {
    return texture(texture0, clamp(uv, vec2(0.0), vec2(1.0))).rgb;
}

void main() {
    vec2 uv = warp(fragTexCoord);
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        finalColor = vec4(0.0, 0.0, 0.0, 1.0);
        return;
    }

    // Phosphor softness: blend toward the neighbour on the side of the
    // texel this fragment lies on.
    float texel = 1.0 / sourceSize.x;
    vec3 center = fetch(uv);
    vec3 left = fetch(uv - vec2(texel, 0.0));
    vec3 right = fetch(uv + vec2(texel, 0.0));
    float side = fract(uv.x * sourceSize.x);
    vec3 color = mix(center, mix(left, right, side), SOFTNESS);

    // Scanlines keyed to logical rows. Bright pixels bleed over the gap.
    float luma = dot(color, vec3(0.299, 0.587, 0.114));
    float row = fract(uv.y * sourceSize.y);
    float beam = pow(sin(row * PI), 1.5);
    color *= 1.0 - SCANLINE_DEPTH * (1.0 - luma * 0.5) * (1.0 - beam);

    // RGB shadow mask over render target pixels, one triad every three
    // columns.
    int phase = int(mod(gl_FragCoord.x, 3.0));
    vec3 mask = vec3(1.0 - MASK_STRENGTH);
    mask[phase] = 1.0;
    color *= mask;

    // Vignette and a soft tube edge.
    float v = 16.0 * uv.x * uv.y * (1.0 - uv.x) * (1.0 - uv.y);
    color *= mix(1.0 - VIGNETTE, 1.0, pow(v, 0.25));
    vec2 edge = smoothstep(vec2(0.0), vec2(EDGE_SOFTNESS), uv)
        * smoothstep(vec2(0.0), vec2(EDGE_SOFTNESS), 1.0 - uv);
    color *= edge.x * edge.y * GAIN;

    finalColor = vec4(color, 1.0) * colDiffuse * fragColor;
}
