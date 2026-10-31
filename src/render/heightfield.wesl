// Surface geometry of a heightfield drawn from a grid texture, shared by the
// terrain and the water.

import package::grid;

// The texel at or below continuous cell coordinates and how far past it they
// lie, for interpolating between that texel and the three after it.
struct Corners {
    origin: vec2i,
    fraction: vec2f,
}

fn corners(cell: vec2f) -> Corners {
    let origin = floor(cell);
    return Corners(vec2i(origin), cell - origin);
}

// `texel` moved onto the nearest texel of a `dims` texture, so loads past the
// edge repeat the edge.
fn clamp_texel(texel: vec2i, dims: vec2u) -> vec2i {
    return clamp(texel, vec2i(0), vec2i(dims) - 1);
}

// Bilinear interpolation of the heights at `origin`, `origin + x`,
// `origin + z` and `origin + x + z`, in that order.
fn mix_corners(heights: vec4f, fraction: vec2f) -> f32 {
    return mix(mix(heights.x, heights.y, fraction.x), mix(heights.z, heights.w, fraction.x), fraction.y);
}

// Height of `heights` at continuous cell coordinates `cell`, interpolated
// between the four surrounding texels.
fn sample(heights: texture_2d<f32>, cell: vec2f) -> f32 {
    let c = corners(cell);
    let dims = textureDimensions(heights);
    return mix_corners(vec4f(
        textureLoad(heights, clamp_texel(c.origin, dims), 0).r,
        textureLoad(heights, clamp_texel(c.origin + vec2i(1, 0), dims), 0).r,
        textureLoad(heights, clamp_texel(c.origin + vec2i(0, 1), dims), 0).r,
        textureLoad(heights, clamp_texel(c.origin + vec2i(1, 1), dims), 0).r,
    ), c.fraction);
}

const NORMAL_MAP_STRENGTH: f32 = 0.11;

const TEXTURE_REPEAT_METRES: f32 = 51.2;

// Texture coordinates at `world_xz`: one repeat of the surface textures every
// TEXTURE_REPEAT_METRES along each axis.
fn tiled_uv(world_xz: vec2f) -> vec2f {
    return world_xz / TEXTURE_REPEAT_METRES;
}

// Vertex tangent for a surface with unit `normal`: along world +x, the
// direction the texture's u grows, with the bitangent towards -z as Bevy's
// normal mapping expects for v growing along +z. Its length scales the normal
// map's bumps, equally along both axes.
fn tangent(normal: vec3f) -> vec4f {
    return vec4f(normalize(vec3f(normal.y, -normal.x, 0.0)) * NORMAL_MAP_STRENGTH, 1.0);
}

// Unit normal of the surface from its heights one cell to either side of a
// point along world x (`x_minus`, `x_plus`) and world z (`z_minus`, `z_plus`).
fn normal(x_minus: f32, x_plus: f32, z_minus: f32, z_plus: f32) -> vec3f {
    let per_metre = grid::CELLS_PER_METER * 0.5;
    return normalize(vec3f((x_minus - x_plus) * per_metre, 1.0, (z_minus - z_plus) * per_metre));
}
