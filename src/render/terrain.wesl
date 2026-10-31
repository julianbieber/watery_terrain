// Displacement of the terrain clipmap by the height texture, shared by the
// forward and prepass vertex entry points.

import package::grid;
import package::render::heightfield;
import bevy_pbr::render::{
    mesh_functions,
    view_transformations::position_world_to_clip,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var height_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var height_sampler: sampler;

const TERRAIN_UV_REPEAT: f32 = 40.0;

fn get_height(world_xz: vec2f) -> vec4f {
    let cell = grid::cell_position(world_xz, textureDimensions(height_texture));
    let n = heightfield::normal(
        heightfield::sample(height_texture, cell - vec2f(1.0, 0.0)),
        heightfield::sample(height_texture, cell + vec2f(1.0, 0.0)),
        heightfield::sample(height_texture, cell - vec2f(0.0, 1.0)),
        heightfield::sample(height_texture, cell + vec2f(0.0, 1.0)),
    );
    return vec4f(heightfield::sample(height_texture, cell), n);
}

fn wrap(x: f32) -> f32 {
    return fract(x + ceil(abs(x)));
}


fn height_uv_range(h: f32) -> vec2f {
    if h < 0.0 {
        return vec2f(0.0, 0.25);
    }
    if h < 2.25 {
        return vec2f(0.25, 0.5);
    }
    if h < 4.35 {
        return vec2f(0.5, 0.75);
    }
    return vec2f(0.75, 1.0);

}

// A clipmap vertex moved onto the terrain surface. `height` is the sampled
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

    let tiled_uv = ((world_position.xz+1024.0)/2048.0) * TERRAIN_UV_REPEAT;
    let u_range = height_uv_range(height.x);
    let uv = vec2f(mix(u_range.x, u_range.y, wrap(tiled_uv.x)), tiled_uv.y);

    return Displaced(world_position, position_world_to_clip(world_position.xyz), height, uv);
}
