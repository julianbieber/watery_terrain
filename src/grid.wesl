// The cell grid shared with the CPU side: how world metres map onto the cells
// of a heightmap texture. The grid is centred on the world origin.

const CELLS_PER_METER: f32 = f32(constants::CELLS_PER_METER);

// Cell coordinates of `world_xz` in a grid of `dims` cells; cell `i` spans
// [i, i + 1).
fn cell_position(world_xz: vec2f, dims: vec2u) -> vec2f {
    return world_xz * CELLS_PER_METER + vec2f(dims) * 0.5;
}

// World position (xz) of the cell coordinates `cell` in a grid of `dims`
// cells; the inverse of `cell_position`.
fn world_position(cell: vec2f, dims: vec2u) -> vec2f {
    return (cell - vec2f(dims) * 0.5) / CELLS_PER_METER;
}
