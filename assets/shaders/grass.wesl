// @material grass

import bevy_pbr_generator::pbr_maps::{Surface, default_surface};
import package::tiling;

struct Params {
    // @group Colour
    dark_color: vec3<f32>,     // @ui "Base" color srgb(0.08, 0.20, 0.04)
    light_color: vec3<f32>,    // @ui "Tips" color srgb(0.32, 0.52, 0.12)
    dry_color: vec3<f32>,      // @ui "Dry" color srgb(0.58, 0.52, 0.24)
    soil_color: vec3<f32>,     // @ui "Soil" color srgb(0.22, 0.16, 0.10)
    // @group Blades
    blade_width: f32,          // @ui "Across" 160.0 [8.0, 512.0] step 1.0
    blade_length: f32,         // @ui "Along" 32.0 [2.0, 256.0] step 1.0
    blade_warp: f32,           // @ui "Bend" 0.015 [0.0, 0.1]
    clump_scale: f32,          // @ui "Clumps" 10.0 [1.0, 64.0] step 1.0
    // @group Patches
    dry_scale: f32,            // @ui "Dry scale" 3.0 [1.0, 16.0] step 1.0
    dry_amount: f32,           // @ui "Dryness" 0.30 [0.0, 1.0]
    soil_amount: f32,          // @ui "Bare soil" 0.15 [0.0, 1.0]
    // @group Surface
    roughness: f32,            // @ui 0.75 [0.0, 1.0]
    specular: f32,             // @ui 0.5 [0.0, 1.0]
    translucency: f32,         // @ui 0.25 [0.0, 1.0]
    sheen: f32,                // @ui "Anisotropy" 0.30 [0.0, 1.0]
    // @group Relief
    relief: f32,               // @ui 0.003 [0.0, 0.02]
    occlusion_strength: f32,   // @ui "Occlusion" 0.60 [0.0, 1.0]
    depth_scale: f32,          // @ui "Parallax depth" 0.06 [0.0, 0.5]
}
@group(1) @binding(0) var<uniform> params: Params;

const NORMAL_EPSILON: f32 = 1.0 / 1024.0;

fn bent(uv: vec2<f32>) -> vec2<f32> {
    let warp = vec2<f32>(
        tiling::fbm(uv, vec2<f32>(4.0), 3u, 0.5),
        tiling::fbm(uv + vec2<f32>(0.37, 0.61), vec2<f32>(4.0), 3u, 0.5),
    );
    return uv + warp * params.blade_warp;
}

fn clumps(uv: vec2<f32>) -> f32 {
    return tiling::unit(tiling::fbm(uv, vec2<f32>(params.clump_scale), 3u, 0.5));
}

fn blades(uv: vec2<f32>) -> f32 {
    let w = bent(uv);
    let cells = vec2<f32>(params.blade_width, params.blade_length);
    let a = tiling::unit(tiling::perlin(w, cells));
    let b = tiling::unit(tiling::perlin(w + vec2<f32>(0.5 / cells.x, 0.29), cells * vec2<f32>(1.3, 1.1)));
    return max(a, b);
}

fn soil(uv: vec2<f32>) -> f32 {
    let bare = 1.0 - clumps(uv);
    return smoothstep(1.0 - params.soil_amount, 1.0, bare + 0.15 * (1.0 - blades(uv)));
}

fn shape(uv: vec2<f32>) -> f32 {
    return blades(uv) * (0.45 + 0.55 * clumps(uv)) * (1.0 - soil(uv));
}

fn height(uv: vec2<f32>) -> f32 {
    return shape(uv) * params.relief;
}

fn surface(uv: vec2<f32>) -> Surface {
    var s = default_surface();

    let h = shape(uv);
    let bare = soil(uv);
    let jitter = tiling::speckle(bent(uv), vec2<f32>(params.blade_width, params.blade_length));

    var color = mix(params.dark_color, params.light_color, smoothstep(0.1, 0.9, h));
    color *= 0.85 + 0.3 * jitter.x;
    let dry = tiling::unit(tiling::fbm(uv, vec2<f32>(params.dry_scale), 4u, 0.5));
    let dry_mask = smoothstep(1.0 - params.dry_amount, 1.0, dry + 0.2 * (jitter.y - 0.5));
    color = mix(color, params.dry_color * (0.8 + 0.4 * h), dry_mask);
    color = mix(color, params.soil_color * (0.85 + 0.3 * jitter.z), bare);
    s.base_color = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0));

    s.roughness = clamp(mix(params.roughness, 0.95, max(bare, dry_mask * 0.5)), 0.0, 1.0);
    s.specular = params.specular;
    s.occlusion = clamp(1.0 - params.occlusion_strength * (1.0 - h), 0.0, 1.0);

    s.diffuse_transmission = params.translucency * h * (1.0 - bare);
    s.thickness = 0.1;

    let du = vec2<f32>(NORMAL_EPSILON, 0.0);
    let dv = vec2<f32>(0.0, NORMAL_EPSILON);
    let slope = vec2<f32>(height(uv + du) - height(uv - du), height(uv + dv) - height(uv - dv))
        / (2.0 * NORMAL_EPSILON);
    s.normal = normalize(vec3<f32>(-slope, 1.0));

    s.anisotropy_direction = vec2<f32>(0.0, 1.0);
    s.anisotropy_strength = params.sheen * h * (1.0 - bare);

    s.depth = clamp((1.0 - h) * params.depth_scale, 0.0, 1.0);

    return s;
}
