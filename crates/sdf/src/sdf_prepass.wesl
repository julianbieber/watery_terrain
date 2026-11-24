// Entry points of the SDF surface material in the depth and normal prepasses
// and the shadow passes: the ray-marched surface's depth, so meshes sort
// against it and it casts shadows.

import package::sdf;

@if(MOTION_VECTOR_PREPASS)
import bevy_pbr::render::pbr_prepass_functions::calculate_motion_vector;

struct Output {
    @if(NORMAL_PREPASS)
    @location(0) normal: vec4f,
    @if(MOTION_VECTOR_PREPASS)
    @location(1) motion_vector: vec2f,
    @builtin(frag_depth) depth: f32,
}

@vertex
fn vertex(@builtin(instance_index) instance_index: u32, @location(0) position: vec3f) -> sdf::ProxyOutput {
    return sdf::proxy(position, instance_index);
}

@fragment
fn fragment(in: sdf::ProxyOutput) -> Output {
    let hit = sdf::march(sdf::ray_at(in.position.xy), sdf::FAR);
    if !hit.found {
        discard;
    }
    var out: Output;
    out.depth = sdf::depth_at(hit.position);
    @if(NORMAL_PREPASS)
    out.normal = vec4f(sdf::hit_normal(hit) * 0.5 + vec3f(0.5), 1.0);
    @if(MOTION_VECTOR_PREPASS)
    out.motion_vector = calculate_motion_vector(vec4f(hit.position, 1.0), vec4f(hit.position, 1.0));
    return out;
}
