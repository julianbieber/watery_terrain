// Entry points of the SDF surface material in the main pass: the ray-marched
// land or water lit by Bevy's PBR from the material's textures, the water
// fading in with the depth of water the ray crosses before the scene behind.

import watery_terrain::render::heightfield;
import package::sdf;
import bevy_pbr::render::{
    forward_io::VertexOutput,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
};

@if(DEPTH_PREPASS)
import bevy_pbr::prepass::utils::prepass_depth;

const WATER_ABSORPTION_PER_METRE: f32 = 0.35;

const DEPTH_WHERE_NOTHING_IS_BEHIND: f32 = 100.0;

struct Output {
    @location(0) color: vec4f,
    @builtin(frag_depth) depth: f32,
}

fn same_surface(t: f32) -> f32 {
    return 0.02 + t * 1e-3;
}

fn tangent(normal: vec3f, u_direction: f32) -> vec4f {
    if abs(normal.z) > 0.999 {
        return vec4f(heightfield::NORMAL_MAP_STRENGTH * u_direction, 0.0, 0.0, u_direction);
    }
    return heightfield::tangent(normal, u_direction);
}

fn texture_uv(position: vec3f, land: bool) -> vec2f {
    if !land {
        return heightfield::tiled_uv(position.xz);
    }
    return heightfield::atlas_uv(position.xz, position.y);
}

fn texture_u_direction(position: vec3f, land: bool) -> f32 {
    if !land {
        return 1.0;
    }
    return heightfield::atlas_u_direction(position.xz);
}

@vertex
fn vertex(@builtin(instance_index) instance_index: u32, @location(0) position: vec3f) -> sdf::ProxyOutput {
    return sdf::proxy(position, instance_index);
}

@fragment
fn fragment(in: sdf::ProxyOutput) -> Output {
    let land = sdf::params.surface == sdf::SURFACE_LAND;
    var ray = sdf::ray_at(in.position.xy);
    var prepass = 0.0;
    @if(DEPTH_PREPASS)
    prepass = prepass_depth(in.position, 0u);
    let scene = sdf::t_at_depth(ray, in.position.xy, prepass);

    var limit = scene;
    if land {
        limit = sdf::FAR;
        @if(DEPTH_PREPASS) {
            if scene >= sdf::FAR {
                discard;
            }
            ray.t_min = max(ray.t_min, scene - same_surface(scene));
            ray.level = 1u;
        }
    }
    let hit = sdf::march(ray, limit);
    if !hit.found {
        discard;
    }
    var depth = sdf::depth_at(hit.position);
    @if(DEPTH_PREPASS) {
        if land && abs(hit.t - scene) < same_surface(scene) {
            depth = prepass;
        }
    }

    let normal = sdf::hit_normal(hit);
    var vertex: VertexOutput;
    vertex.position = vec4f(in.position.xy, depth, in.position.w);
    vertex.world_position = vec4f(hit.position, 1.0);
    vertex.world_normal = normal;
    @if(VERTEX_UVS_A)
    vertex.uv = texture_uv(hit.position, land);
    @if(VERTEX_TANGENTS)
    vertex.world_tangent = tangent(normal, texture_u_direction(hit.position, land));
    @if(VERTEX_OUTPUT_INSTANCE_INDEX)
    vertex.instance_index = in.instance_index;

    var pbr_input = pbr_input_from_standard_material(vertex, true);
    if !land {
        let crossed = min(scene, hit.t + DEPTH_WHERE_NOTHING_IS_BEHIND) - hit.t;
        pbr_input.material.base_color.a *= 1.0 - exp(-crossed * WATER_ABSORPTION_PER_METRE);
    }
    let lit = apply_pbr_lighting(pbr_input);
    return Output(main_pass_post_lighting_processing(pbr_input, lit), depth);
}
