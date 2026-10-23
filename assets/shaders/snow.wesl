// @material snow

import bevy_pbr_generator::pbr_maps::{Surface, default_surface};
import package::tiling;

struct Params {
    // @group Colour
    snow_color: vec3<f32>,     // @ui "Snow" color srgb(0.95, 0.97, 1.00)
    shadow_color: vec3<f32>,   // @ui "Shadow" color srgb(0.72, 0.80, 0.92)
    // @group Drifts
    drift_scale: f32,          // @ui "Drifts" 3.0 [1.0, 16.0] step 1.0
    wind_across: f32,          // @ui "Ridges across" 4.0 [1.0, 32.0] step 1.0
    wind_along: f32,           // @ui "Ridges along" 16.0 [1.0, 64.0] step 1.0
    ridge_amount: f32,         // @ui "Ridges" 0.35 [0.0, 1.0]
    // @group Sparkle
    sparkle_scale: f32,        // @ui "Crystal size" 384.0 [32.0, 1024.0] step 1.0
    sparkle_density: f32,      // @ui "Density" 0.03 [0.0, 0.2]
    sparkle_roughness: f32,    // @ui "Roughness" 0.08 [0.0, 1.0]
    sparkle_tilt: f32,         // @ui "Tilt" 0.6 [0.0, 2.0]
    sparkle_glow: f32,         // @ui "Glow" 0.0 [0.0, 2.0]
    // @group Surface
    roughness: f32,            // @ui 0.70 [0.0, 1.0]
    specular: f32,             // @ui 0.35 [0.0, 1.0]
    translucency: f32,         // @ui 0.35 [0.0, 1.0]
    thickness: f32,            // @ui 0.6 [0.0, 1.0]
    // @group Relief
    relief: f32,               // @ui 0.006 [0.0, 0.03]
    occlusion_strength: f32,   // @ui "Occlusion" 0.20 [0.0, 1.0]
    depth_scale: f32,          // @ui "Parallax depth" 0.04 [0.0, 0.5]
}
@group(1) @binding(0) var<uniform> params: Params;

const NORMAL_EPSILON: f32 = 1.0 / 1024.0;

fn shape(uv: vec2<f32>) -> f32 {
    let drifts = tiling::unit(tiling::fbm(uv, vec2<f32>(params.drift_scale), 4u, 0.5));
    let warp = vec2<f32>(tiling::fbm(uv, vec2<f32>(2.0), 2u, 0.5), 0.0) * 0.08;
    let ridges = pow(tiling::ridged(uv + warp, vec2<f32>(params.wind_across, params.wind_along), 3u, 0.45), 3.0);
    let fine = tiling::unit(tiling::perlin(uv, vec2<f32>(96.0)));
    return mix(drifts, ridges, params.ridge_amount) * 0.95 + fine * 0.05;
}

fn height(uv: vec2<f32>) -> f32 {
    return shape(uv) * params.relief;
}

fn surface(uv: vec2<f32>) -> Surface {
    var s = default_surface();

    let h = shape(uv);
    let crystal = tiling::speckle(uv, vec2<f32>(params.sparkle_scale));
    let glint = step(1.0 - params.sparkle_density, crystal.x);

    s.base_color = mix(params.shadow_color, params.snow_color, smoothstep(0.0, 1.0, 0.3 + 0.7 * h));

    s.roughness = mix(params.roughness, params.sparkle_roughness, glint);
    s.specular = mix(params.specular, 1.0, glint);
    s.emissive = vec3<f32>(params.sparkle_glow * glint);
    s.occlusion = clamp(1.0 - params.occlusion_strength * (1.0 - h), 0.0, 1.0);

    s.diffuse_transmission = params.translucency;
    s.thickness = params.thickness;

    let du = vec2<f32>(NORMAL_EPSILON, 0.0);
    let dv = vec2<f32>(0.0, NORMAL_EPSILON);
    let slope = vec2<f32>(height(uv + du) - height(uv - du), height(uv + dv) - height(uv - dv))
        / (2.0 * NORMAL_EPSILON);
    let facet = (crystal.yz - 0.5) * params.sparkle_tilt * glint;
    s.normal = normalize(vec3<f32>(-slope + facet, 1.0));

    s.depth = clamp((1.0 - h) * params.depth_scale, 0.0, 1.0);

    return s;
}
