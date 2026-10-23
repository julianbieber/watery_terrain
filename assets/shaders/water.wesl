// @material water

import bevy_pbr_generator::pbr_maps::{Surface, default_surface};
import package::tiling;

struct Params {
    // @group Colour
    deep_color: vec3<f32>,        // @ui "Deep" color srgb(0.02, 0.10, 0.16)
    shallow_color: vec3<f32>,     // @ui "Shallow" color srgb(0.06, 0.30, 0.34)
    foam_color: vec3<f32>,        // @ui "Foam" color srgb(0.90, 0.95, 0.97)
    opacity: f32,                 // @ui 1.0 [0.0, 1.0]
    // @group Waves
    wave_scale: f32,              // @ui "Waves" 4.0 [1.0, 32.0] step 1.0
    wave_octaves: i32,            // @ui "Octaves" 4 [1, 8]
    ripple_scale: f32,            // @ui "Ripples" 24.0 [1.0, 128.0] step 1.0
    wave_height: f32,             // @ui "Wave height" 0.004 [0.0, 0.02]
    ripple_height: f32,           // @ui "Ripple height" 0.0008 [0.0, 0.005]
    // @group Foam
    foam_amount: f32,             // @ui "Amount" 0.10 [0.0, 1.0]
    foam_roughness: f32,          // @ui "Roughness" 0.6 [0.0, 1.0]
    // @group Surface
    roughness: f32,               // @ui 0.04 [0.0, 1.0]
    specular: f32,                // @ui 0.35 [0.0, 1.0]
    specular_transmission: f32,   // @ui "Transmission" 0.7 [0.0, 1.0]
    thickness: f32,               // @ui 0.3 [0.0, 1.0]
    depth_scale: f32,             // @ui "Parallax depth" 0.02 [0.0, 0.5]
}
@group(1) @binding(0) var<uniform> params: Params;

const NORMAL_EPSILON: f32 = 1.0 / 1024.0;

fn flowed(uv: vec2<f32>) -> vec2<f32> {
    let warp = vec2<f32>(
        tiling::fbm(uv, vec2<f32>(2.0), 3u, 0.5),
        tiling::fbm(uv + vec2<f32>(0.53, 0.29), vec2<f32>(2.0), 3u, 0.5),
    );
    return uv + warp * (0.25 / max(round(params.wave_scale), 1.0));
}

fn crest(uv: vec2<f32>) -> f32 {
    return pow(tiling::ridged(flowed(uv), vec2<f32>(params.wave_scale), 2u, 0.5), 4.0);
}

fn swell(uv: vec2<f32>) -> f32 {
    return tiling::unit(tiling::fbm(flowed(uv), vec2<f32>(params.wave_scale), u32(max(params.wave_octaves, 1)), 0.5));
}

fn height(uv: vec2<f32>) -> f32 {
    let waves = 0.6 * swell(uv) + 0.4 * crest(uv);
    let ripples = tiling::fbm(uv, vec2<f32>(params.ripple_scale), 3u, 0.5);
    return waves * params.wave_height + ripples * params.ripple_height;
}

fn surface(uv: vec2<f32>) -> Surface {
    var s = default_surface();

    let depth = swell(uv);
    let breakup = tiling::unit(tiling::fbm(uv, vec2<f32>(params.ripple_scale * 2.0), 3u, 0.5));
    let foam = smoothstep(1.0 - params.foam_amount, 1.0, crest(uv) * (0.6 + 0.6 * breakup));

    let water = mix(params.deep_color, params.shallow_color, smoothstep(0.2, 0.8, depth));
    s.base_color = mix(water, params.foam_color, foam);
    s.opacity = mix(params.opacity, 1.0, foam);

    s.roughness = mix(params.roughness, params.foam_roughness, foam);
    s.specular = params.specular;
    s.occlusion = 1.0;

    s.specular_transmission = params.specular_transmission * (1.0 - foam);
    s.thickness = params.thickness;

    let du = vec2<f32>(NORMAL_EPSILON, 0.0);
    let dv = vec2<f32>(0.0, NORMAL_EPSILON);
    let slope = vec2<f32>(height(uv + du) - height(uv - du), height(uv + dv) - height(uv - dv))
        / (2.0 * NORMAL_EPSILON);
    s.normal = normalize(vec3<f32>(-slope, 1.0));

    s.depth = clamp((1.0 - depth) * params.depth_scale, 0.0, 1.0);

    return s;
}
