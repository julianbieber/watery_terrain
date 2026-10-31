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

const DRY_DEPTH: f32 = f32(constants::DRY_DEPTH_MM) / 1000.0;

struct Column {
    water: f32,
    ground: f32,
}

fn column(texel: vec2i) -> Column {
    let clamped = heightfield::clamp_texel(texel, textureDimensions(water_height_texture));
    return Column(
        textureLoad(water_height_texture, clamped, 0).r,
        textureLoad(base_height_texture, clamped, 0).r,
    );
}

fn surface(texel: vec2i) -> f32 {
    let c = column(texel);
    return c.water + c.ground;
}

// Height the water is drawn at: its surface where there is water, just
// below the terrain where there is none, so dry ground hides it.
fn drawn(texel: vec2i) -> f32 {
    let c = column(texel);
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

fn get_height(world_xz: vec2f) -> vec4f {
    let cell = grid::cell_position(world_xz, textureDimensions(water_height_texture));
    let n = heightfield::normal(
        sample_surface(cell - vec2f(1.0, 0.0)),
        sample_surface(cell + vec2f(1.0, 0.0)),
        sample_surface(cell - vec2f(0.0, 1.0)),
        sample_surface(cell + vec2f(0.0, 1.0)),
    );
    return vec4f(sample_drawn(cell), n);
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

    let uv = heightfield::tiled_uv(world_position.xz);

    return Displaced(world_position, position_world_to_clip(world_position.xyz), height, uv);
}
