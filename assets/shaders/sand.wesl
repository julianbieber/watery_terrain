// @material sand

import bevy_pbr_generator::pbr_maps::{Surface, default_surface};
import package::tiling;

struct Params {
    // @group Colour
    light_color: vec3<f32>,   // @ui "Crest" color srgb(0.86, 0.76, 0.58)
    dark_color: vec3<f32>,    // @ui "Trough" color srgb(0.64, 0.52, 0.36)
    grain_color: vec3<f32>,   // @ui "Coarse grain" color srgb(0.42, 0.36, 0.30)
    // @group Ripples
    ripple_count: f32,        // @ui "Ripples" 14.0 [1.0, 48.0] step 1.0
    ripple_skew: f32,         // @ui "Skew" 3.0 [0.0, 16.0] step 1.0
    ripple_warp: f32,         // @ui "Warp" 0.6 [0.0, 2.0]
    ripple_sharpness: f32,    // @ui "Sharpness" 0.6 [0.0, 1.0]
    // @group Grains
    patch_scale: f32,         // @ui "Patches" 3.0 [1.0, 16.0] step 1.0
    grain_scale: f32,         // @ui "Grain" 384.0 [16.0, 1024.0] step 1.0
    grain_amount: f32,        // @ui "Coarse amount" 0.25 [0.0, 1.0]
    // @group Surface
    roughness: f32,           // @ui 0.92 [0.0, 1.0]
    specular: f32,            // @ui 0.5 [0.0, 1.0]
    // @group Relief
    relief: f32,              // @ui 0.004 [0.0, 0.02]
    occlusion_strength: f32,  // @ui "Occlusion" 0.35 [0.0, 1.0]
    depth_scale: f32,         // @ui "Parallax depth" 0.04 [0.0, 0.5]
}
@group(1) @binding(0) var<uniform> params: Params;

const TAU: f32 = 6.28318530718;
const NORMAL_EPSILON: f32 = 1.0 / 1024.0;

fn ripple(uv: vec2<f32>) -> f32 {
    let warp = tiling::fbm(uv, vec2<f32>(2.0), 3u, 0.5) * params.ripple_warp;
    let x = round(params.ripple_count) * uv.y + round(params.ripple_skew) * uv.x + warp;
    let t = fract(x);
    let rounded = 0.5 + 0.5 * sin(x * TAU);
    let windswept = select((1.0 - t) / 0.25, t / 0.75, t < 0.75);
    return mix(rounded, windswept, params.ripple_sharpness);
}

fn patches(uv: vec2<f32>) -> f32 {
    return tiling::unit(tiling::fbm(uv, vec2<f32>(params.patch_scale), 4u, 0.5));
}

fn shape(uv: vec2<f32>) -> f32 {
    let fine = tiling::unit(tiling::perlin(uv, vec2<f32>(params.grain_scale * 0.25)));
    return ripple(uv) * (0.6 + 0.4 * patches(uv)) * 0.92 + fine * 0.08;
}

fn height(uv: vec2<f32>) -> f32 {
    return shape(uv) * params.relief;
}

fn surface(uv: vec2<f32>) -> Surface {
    var s = default_surface();

    let h = shape(uv);
    let grain = tiling::speckle(uv, vec2<f32>(params.grain_scale));

    let tone = smoothstep(0.0, 1.0, 0.55 * h + 0.45 * patches(uv));
    var color = mix(params.dark_color, params.light_color, tone);
    color *= 0.92 + 0.16 * grain.x;
    let coarse = step(1.0 - params.grain_amount * 0.3 * (1.2 - h), grain.y);
    color = mix(color, params.grain_color * (0.8 + 0.4 * grain.z), coarse);
    s.base_color = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0));

    s.roughness = clamp(params.roughness - 0.06 * grain.z, 0.0, 1.0);
    s.specular = params.specular;
    s.occlusion = clamp(1.0 - params.occlusion_strength * (1.0 - h), 0.0, 1.0);

    let du = vec2<f32>(NORMAL_EPSILON, 0.0);
    let dv = vec2<f32>(0.0, NORMAL_EPSILON);
    let slope = vec2<f32>(height(uv + du) - height(uv - du), height(uv + dv) - height(uv - dv))
        / (2.0 * NORMAL_EPSILON);
    s.normal = normalize(vec3<f32>(-slope, 1.0));

    s.depth = clamp((1.0 - h) * params.depth_scale, 0.0, 1.0);

    return s;
}
