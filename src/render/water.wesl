// Displacement of the water clipmap onto the simulated water surface, shared
// by the forward and prepass vertex entry points.

import package::grid;
import package::render::heightfield;
import bevy_pbr::render::{
    mesh_functions,
    view_transformations::position_world_to_clip,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var water_height_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var water_height_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var base_height_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var base_height_sampler: sampler;

fn surface(texel: vec2i) -> f32 {
    return textureLoad(water_height_texture, texel, 0).r + textureLoad(base_height_texture, texel, 0).r;
}

fn get_height(world_xz: vec2f) -> vec4f {
    let uv = vec2i(floor(grid::cell_position(world_xz, textureDimensions(water_height_texture))));
    let h: f32 = textureLoad(water_height_texture, uv, 0).r;
    let base_h: f32 = textureLoad(base_height_texture, uv, 0).r;

    let n = heightfield::normal(
        surface(uv - vec2i(1, 0)),
        surface(uv + vec2i(1, 0)),
        surface(uv - vec2i(0, 1)),
        surface(uv + vec2i(0, 1)),
    );
    if h == 0.0{
        return vec4f(base_h - 0.1, n);
    }

    return vec4f(h+base_h, n);
}

// A clipmap vertex moved onto the water surface. `height` is the surface
// height followed by the surface normal.
struct Displaced {
    world_position: vec4f,
    clip_position: vec4f,
    height: vec4f,
    uv: vec2f,
}

fn displace(instance_index: u32, local_position: vec3f) -> Displaced {
    let model = mesh_functions::get_world_from_local(instance_index);
    var world_position = model * vec4<f32>(local_position, 1.0);
    let height = get_height(world_position.xz);
    world_position.y = height.x;

    let uv = ((world_position.xz+1024.0)/2048.0)*40.0;

    return Displaced(world_position, position_world_to_clip(world_position.xyz), height, uv);
}

fn world_tangent(normal: vec3f) -> vec4f {
    if abs(dot(normal, vec3(0,1,0))) < 0.899 {
        return vec4(normalize(cross(vec3(0,1,0), normal)), 1.0)*0.11;
    }
    return vec4(normalize(cross(vec3(1,0,0), normal)), 1.0)*0.11;
}
