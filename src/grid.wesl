// The cell grid shared with the CPU side: how world metres map onto world
// cells, and world cells onto the texels of the window textures. The world is
// centred on the origin; the window textures hold WINDOW_CHUNKS x
// WINDOW_CHUNKS chunks, each at its world cells modulo WINDOW_DIM.

const CELLS_PER_METER: f32 = f32(constants::CELLS_PER_METER);

const CHUNK_DIM: u32 = constants::CHUNK_DIM;

const WORLD_DIM: u32 = constants::WORLD_CHUNKS * constants::CHUNK_DIM;

const WINDOW_DIM: u32 = constants::WINDOW_CHUNKS * constants::CHUNK_DIM;

// Continuous world cell coordinates of `world_xz`; cell `i` spans [i, i + 1).
fn world_cell(world_xz: vec2f) -> vec2f {
    return world_xz * CELLS_PER_METER + f32(WORLD_DIM) * 0.5;
}

// World position (xz) of the continuous world cell coordinates `cell`; the
// inverse of `world_cell`.
fn world_of_cell(cell: vec2f) -> vec2f {
    return (cell - f32(WORLD_DIM) * 0.5) / CELLS_PER_METER;
}

// First world cell of the window whose lower chunk is `window_origin`.
fn window_first_cell(window_origin: vec2u) -> vec2i {
    return vec2i(window_origin * CHUNK_DIM);
}

// Texel holding world cell `cell`, after moving it onto the nearest cell of
// the window whose lower chunk is `window_origin`, so reads past the window
// repeat its edge.
fn window_texel(cell: vec2i, window_origin: vec2u) -> vec2u {
    let first = window_first_cell(window_origin);
    let inside = vec2u(clamp(cell, first, first + i32(WINDOW_DIM) - 1));
    return ((inside / CHUNK_DIM) % constants::WINDOW_CHUNKS) * CHUNK_DIM + inside % CHUNK_DIM;
}
