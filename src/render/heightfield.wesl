// Surface geometry of a heightfield drawn from a grid texture, shared by the
// terrain and the water.

import package::grid;

// Unit normal of the surface from its heights one cell to either side of a
// point along world x (`x_minus`, `x_plus`) and world z (`z_minus`, `z_plus`).
fn normal(x_minus: f32, x_plus: f32, z_minus: f32, z_plus: f32) -> vec3f {
    let per_metre = grid::CELLS_PER_METER * 0.5;
    return normalize(vec3f((x_minus - x_plus) * per_metre, 1.0, (z_minus - z_plus) * per_metre));
}
