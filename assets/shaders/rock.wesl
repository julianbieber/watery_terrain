// @material rock

import bevy_pbr_generator::pbr_maps::{Surface, default_surface};
import package::tiling;

struct Params {
    // @group Colour
    dark_color: vec3<f32>,      // @ui "Dark" color srgb(0.20, 0.19, 0.18)
    light_color: vec3<f32>,     // @ui "Light" color srgb(0.50, 0.48, 0.45)
    strata_color: vec3<f32>,    // @ui "Strata" color srgb(0.46, 0.38, 0.30)
    crack_color: vec3<f32>,     // @ui "Crack" color srgb(0.07, 0.065, 0.06)
    mineral_color: vec3<f32>,   // @ui "Mineral" color srgb(0.70, 0.67, 0.62)
    // @group Formation
    block_scale: f32,           // @ui "Blocks" 4.0 [1.0, 32.0] step 1.0
    crack_width: f32,           // @ui "Crack width" 0.06 [0.0, 0.3]
    crack_depth: f32,           // @ui "Crack depth" 0.6 [0.0, 1.0]
    detail_scale: f32,          // @ui "Detail" 16.0 [1.0, 128.0] step 1.0
    octaves: i32,               // @ui 5 [1, 8]
    // @group Strata
    strata_count: f32,          // @ui "Layers" 9.0 [1.0, 48.0] step 1.0
    strata_warp: f32,           // @ui "Warp" 0.5 [0.0, 2.0]
    strata_amount: f32,         // @ui "Amount" 0.35 [0.0, 1.0]
    // @group Minerals
    mineral_scale: f32,         // @ui "Grain" 512.0 [32.0, 1024.0] step 1.0
    mineral_amount: f32,        // @ui "Amount" 0.08 [0.0, 0.5]
    // @group Surface
    roughness: f32,             // @ui 0.85 [0.0, 1.0]
    roughness_variation: f32,   // @ui "Variation" 0.10 [0.0, 0.5]
    specular: f32,              // @ui 0.5 [0.0, 1.0]
    // @group Relief
    relief: f32,                // @ui 0.012 [0.0, 0.05]
    occlusion_strength: f32,    // @ui "Occlusion" 0.6 [0.0, 1.0]
    depth_scale: f32,           // @ui "Parallax depth" 0.10 [0.0, 0.5]
}
@group(1) @binding(0) var<uniform> params: Params;

const TAU: f32 = 6.28318530718;
const NORMAL_EPSILON: f32 = 1.0 / 1024.0;

fn crack(uv: vec2<f32>) -> f32 {
    let warp = vec2<f32>(
        tiling::fbm(uv, vec2<f32>(8.0), 3u, 0.5),
        tiling::fbm(uv + vec2<f32>(0.43, 0.17), vec2<f32>(8.0), 3u, 0.5),
    ) * (0.15 / max(round(params.block_scale), 1.0));
    let d = tiling::worley(uv + warp, vec2<f32>(params.block_scale));
    return 1.0 - smoothstep(0.0, params.crack_width, d.y - d.x);
}

fn detail(uv: vec2<f32>) -> f32 {
    return tiling::unit(tiling::fbm(uv, vec2<f32>(params.detail_scale), u32(max(params.octaves, 1)), 0.5));
}

fn shape(uv: vec2<f32>) -> f32 {
    let mass = tiling::unit(tiling::fbm(uv, vec2<f32>(params.block_scale * 2.0), 3u, 0.5));
    return clamp(0.35 * mass + 0.65 * detail(uv) - crack(uv) * params.crack_depth, 0.0, 1.0);
}

fn height(uv: vec2<f32>) -> f32 {
    return shape(uv) * params.relief;
}

fn surface(uv: vec2<f32>) -> Surface {
    var s = default_surface();

    let h = shape(uv);
    let c = crack(uv);
    let mineral = tiling::speckle(uv, vec2<f32>(params.mineral_scale));

    let tone = tiling::unit(tiling::fbm(uv + vec2<f32>(0.21, 0.73), vec2<f32>(params.block_scale * 1.5), 4u, 0.55));
    var color = mix(params.dark_color, params.light_color, smoothstep(0.15, 0.85, 0.6 * tone + 0.4 * h));

    let strata_warp = tiling::fbm(uv, vec2<f32>(2.0), 3u, 0.5) * params.strata_warp;
    let strata = 0.5 + 0.5 * sin((round(params.strata_count) * uv.y + strata_warp) * TAU);
    color = mix(color, params.strata_color * (0.7 + 0.6 * tone), params.strata_amount * strata * strata);

    color *= 0.9 + 0.2 * mineral.y;
    color = mix(color, params.mineral_color, step(1.0 - params.mineral_amount, mineral.x));
    color = mix(color, params.crack_color, c);
    s.base_color = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0));

    let variation = (detail(uv) - 0.5) * 2.0 * params.roughness_variation;
    s.roughness = clamp(params.roughness + variation + 0.1 * c, 0.0, 1.0);
    s.specular = params.specular;
    s.occlusion = clamp(1.0 - params.occlusion_strength * (0.8 * c + 0.3 * (1.0 - h)), 0.0, 1.0);

    let du = vec2<f32>(NORMAL_EPSILON, 0.0);
    let dv = vec2<f32>(0.0, NORMAL_EPSILON);
    let slope = vec2<f32>(height(uv + du) - height(uv - du), height(uv + dv) - height(uv - dv))
        / (2.0 * NORMAL_EPSILON);
    s.normal = normalize(vec3<f32>(-slope, 1.0));

    s.depth = clamp((1.0 - h) * params.depth_scale, 0.0, 1.0);

    return s;
}
