// The cell grid shared with the CPU side: how world metres map onto world
// cells, and world cells onto the texels of the window textures. The world is
// centred on the origin; the window textures hold WINDOW_CHUNKS x
// WINDOW_CHUNKS chunks, each at its world cells modulo WINDOW_DIM. The coarse
// window textures hold the same chunks on a grid COARSE_FACTOR times coarser.

const CELLS_PER_METER: f32 = f32(constants::CELLS_PER_METER);

const CHUNK_DIM: u32 = constants::CHUNK_DIM;

const WORLD_DIM: u32 = constants::WORLD_CHUNKS * constants::CHUNK_DIM;

const WINDOW_DIM: u32 = constants::WINDOW_CHUNKS * constants::CHUNK_DIM;

const COARSE_FACTOR: u32 = constants::COARSE_FACTOR;

const COARSE_CHUNK_DIM: u32 = CHUNK_DIM / COARSE_FACTOR;

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
    return texel_on(cell, window_origin, CHUNK_DIM);
}

// Texel of the coarse window textures holding coarse world cell `cell`, which
// spans world cells `cell * COARSE_FACTOR` onwards, after moving it onto the
// nearest coarse cell of the window, so reads past the window repeat its edge.
fn coarse_window_texel(cell: vec2i, window_origin: vec2u) -> vec2u {
    return texel_on(cell, window_origin, COARSE_CHUNK_DIM);
}

// Texel holding `cell` of a grid with `chunk_dim` cells along each chunk side,
// clamped onto the window whose lower chunk is `window_origin`.
fn texel_on(cell: vec2i, window_origin: vec2u, chunk_dim: u32) -> vec2u {
    let first = vec2i(window_origin * chunk_dim);
    let inside = vec2u(clamp(cell, first, first + i32(chunk_dim * constants::WINDOW_CHUNKS) - 1));
    return ((inside / chunk_dim) % constants::WINDOW_CHUNKS) * chunk_dim + inside % chunk_dim;
}
