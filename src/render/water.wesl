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
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var<uniform> window_origin: vec2u;
@group(constants::MATERIAL_BIND_GROUP) @binding(105) var<uniform> shape: heightfield::Shape;

const DRY_DEPTH: f32 = f32(constants::DRY_DEPTH_MM) / 1000.0;

struct Column {
    water: f32,
    ground: f32,
}

fn column(cell: vec2i) -> Column {
    let texel = grid::window_texel(cell, window_origin);
    return Column(
        textureLoad(water_height_texture, texel, 0).r,
        textureLoad(base_height_texture, texel, 0).r,
    );
}

fn surface(cell: vec2i) -> f32 {
    let c = column(cell);
    return c.water + c.ground;
}

// Height the water is drawn at: its surface where there is water, just
// below the terrain where there is none, so dry ground hides it.
fn drawn(cell: vec2i) -> f32 {
    let c = column(cell);
    if c.water <= DRY_DEPTH {
        return c.ground - 0.1;
    }
    return c.water + c.ground;
}

fn sample_surface(cell: vec2f) -> f32 {
    let c = heightfield::corners(cell);
    return heightfield::mix_corners(vec4f(
        surface(c.origin),
        surface(c.origin + vec2i(1, 0)),
        surface(c.origin + vec2i(0, 1)),
        surface(c.origin + vec2i(1, 1)),
    ), c.fraction);
}

fn sample_drawn(cell: vec2f) -> f32 {
    let c = heightfield::corners(cell);
    return heightfield::mix_corners(vec4f(
        drawn(c.origin),
        drawn(c.origin + vec2i(1, 0)),
        drawn(c.origin + vec2i(0, 1)),
        drawn(c.origin + vec2i(1, 1)),
    ), c.fraction);
}

fn morphed_drawn(cell: vec2f, placement: heightfield::Placement) -> f32 {
    let height = sample_drawn(cell);
    let o = heightfield::coarse_offset(placement);
    if o.x == 0.0 && o.y == 0.0 {
        return height;
    }
    var coarse = sample_drawn(cell + o) + sample_drawn(cell - o);
    if o.x != 0.0 && o.y != 0.0 {
        coarse = (coarse + sample_drawn(cell + vec2f(o.x, -o.y)) + sample_drawn(cell + vec2f(-o.x, o.y))) * 0.5;
    }
    return mix(height, coarse * 0.5, placement.blend);
}

fn get_height(placement: heightfield::Placement) -> vec4f {
    let cell = grid::world_cell(placement.world_xz);
    let step = heightfield::normal_step(placement);
    let n = heightfield::normal(
        sample_surface(cell - vec2f(step, 0.0)),
        sample_surface(cell + vec2f(step, 0.0)),
        sample_surface(cell - vec2f(0.0, step)),
        sample_surface(cell + vec2f(0.0, step)),
        step,
    );
    return vec4f(morphed_drawn(cell, placement), n);
}

// A clipmap vertex moved onto the water surface. `height` is the surface
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

    let uv = heightfield::tiled_uv(world_position.xz);

    return Displaced(world_position, position_world_to_clip(world_position.xyz), height, uv);
}
