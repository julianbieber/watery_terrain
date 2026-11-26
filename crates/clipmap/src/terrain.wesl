// Displacement of the terrain clipmap by the height texture, shared by the
// forward and prepass vertex entry points.

import watery_terrain::grid;
import watery_terrain::render::heightfield;
import bevy_pbr::render::{
    mesh_functions,
    view_transformations::position_world_to_clip,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var height_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var height_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var<uniform> window_origin: vec2u;
@group(constants::MATERIAL_BIND_GROUP) @binding(105) var<uniform> shape: heightfield::Shape;

fn height_at(cell: vec2f) -> f32 {
    return heightfield::sample(height_texture, cell, window_origin);
}

fn morphed_height(cell: vec2f, placement: heightfield::Placement) -> f32 {
    let height = height_at(cell);
    let o = heightfield::coarse_offset(placement);
    if o.x == 0.0 && o.y == 0.0 {
        return height;
    }
    var coarse = height_at(cell + o) + height_at(cell - o);
    if o.x != 0.0 && o.y != 0.0 {
        coarse = (coarse + height_at(cell + vec2f(o.x, -o.y)) + height_at(cell + vec2f(-o.x, o.y))) * 0.5;
    }
    return mix(height, coarse * 0.5, placement.blend);
}

fn get_height(placement: heightfield::Placement) -> vec4f {
    let cell = grid::world_cell(placement.world_xz);
    let step = heightfield::normal_step(placement);
    let n = heightfield::normal(
        height_at(cell - vec2f(step, 0.0)),
        height_at(cell + vec2f(step, 0.0)),
        height_at(cell - vec2f(0.0, step)),
        height_at(cell + vec2f(0.0, step)),
        step,
    );
    return vec4f(morphed_height(cell, placement), n);
}

// A clipmap vertex moved onto the terrain surface. `height` is the sampled
// height followed by the surface normal.
struct Displaced {
    world_position: vec4f,
    clip_position: vec4f,
    height: vec4f,
    uv: vec2f,
}

fn displace(instance_index: u32, local_position: vec3f, quad: f32) -> Displaced {
    let model = mesh_functions::get_world_from_local(instance_index);
    let placement = heightfield::place(model, local_position.xz, quad, shape);
    let height = get_height(placement);
    let world_position = vec4f(placement.world_xz.x, height.x, placement.world_xz.y, 1.0);

    let uv = heightfield::atlas_uv(world_position.xz, height.x);

    return Displaced(world_position, position_world_to_clip(world_position.xyz), height, uv);
}
