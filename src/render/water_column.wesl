// The water surface every renderer draws over the window: where the water and
// the ground of a world cell lie, from the fine window textures in the chunk
// the simulation follows the camera with and from the coarse ones elsewhere.

import package::grid;

const DRY_DEPTH: f32 = f32(constants::DRY_DEPTH_MM) / 1000.0;

// How far below the ground dry cells are drawn, so the ground hides them.
const DRY_DROP: f32 = 0.1;

struct Column {
    water: f32,
    ground: f32,
}

// Water and ground of world cell `cell` of the window whose lower chunk is
// `window_origin`, from its fine (`water`, `ground`) and coarse
// (`coarse_water`, `coarse_ground`) textures. Outside the chunk `centre` the
// water reaches up to its coarse cell's surface, and is dry while that is.
fn column(
    water: texture_2d<f32>,
    ground: texture_2d<f32>,
    coarse_water: texture_2d<f32>,
    coarse_ground: texture_2d<f32>,
    cell: vec2i,
    window_origin: vec2u,
    centre: vec2u,
) -> Column {
    let texel = grid::window_texel(cell, window_origin);
    let ground_height = textureLoad(ground, texel, 0).r;
    let chunk = vec2u(clamp(cell, vec2i(0), vec2i(i32(grid::WORLD_DIM) - 1))) / grid::CHUNK_DIM;
    if all(chunk == centre) {
        return Column(textureLoad(water, texel, 0).r, ground_height);
    }
    let coarse_cell = vec2i(floor(vec2f(cell) / f32(grid::COARSE_FACTOR)));
    let coarse_texel = grid::coarse_window_texel(coarse_cell, window_origin);
    let coarse_depth = textureLoad(coarse_water, coarse_texel, 0).r;
    if coarse_depth <= DRY_DEPTH {
        return Column(0.0, ground_height);
    }
    let surface = textureLoad(coarse_ground, coarse_texel, 0).r + coarse_depth;
    return Column(max(surface - ground_height, 0.0), ground_height);
}

fn surface(c: Column) -> f32 {
    return c.water + c.ground;
}

// Height the water of `c` is drawn at: its surface where there is water,
// DRY_DROP below the ground where there is none.
fn drawn(c: Column) -> f32 {
    if c.water <= DRY_DEPTH {
        return c.ground - DRY_DROP;
    }
    return c.water + c.ground;
}
