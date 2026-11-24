// Max-mip pyramids of the window's terrain and water surfaces for the SDF
// surface material: every node holds the highest point of the bilinear
// surface over its cells, the far corners included, so a ray passing above a
// node passes above everything under it. Node (0, 0) of every level starts at
// the window's first cell. Water patches whose corners are all dry lie under
// the ground and count as no water at all.

import watery_terrain::grid;
import watery_terrain::render::water_column;

struct Params {
    window_origin: vec2u,
    centre: vec2u,
}

var<immediate> params: Params;

@group(0) @binding(0) var output: texture_storage_2d<r32float, write>;
@group(0) @binding(1) var input_a: texture_2d<f32>;
@group(0) @binding(2) var input_b: texture_2d<f32>;
@group(0) @binding(3) var input_c: texture_2d<f32>;
@group(0) @binding(4) var input_d: texture_2d<f32>;

const NO_WATER: f32 = -1e30;

fn in_output(texel: vec2u) -> bool {
    return all(texel < textureDimensions(output));
}

// Level k + 1 from level k in `input_a`.
@compute @workgroup_size(constants::WORKGROUP_SIZE, constants::WORKGROUP_SIZE, 1)
fn reduce(@builtin(global_invocation_id) id: vec3u) {
    let texel = id.xy;
    if !in_output(texel) {
        return;
    }
    let base = texel * 2u;
    let highest = max(
        max(textureLoad(input_a, base, 0).r, textureLoad(input_a, base + vec2u(1u, 0u), 0).r),
        max(textureLoad(input_a, base + vec2u(0u, 1u), 0).r, textureLoad(input_a, base + vec2u(1u, 1u), 0).r),
    );
    textureStore(output, texel, vec4f(highest, 0.0, 0.0, 0.0));
}

fn window_first_cell() -> vec2i {
    return grid::window_first_cell(params.window_origin);
}

// Terrain level 1 from the ground window texture in `input_a`.
@compute @workgroup_size(constants::WORKGROUP_SIZE, constants::WORKGROUP_SIZE, 1)
fn terrain_base(@builtin(global_invocation_id) id: vec3u) {
    let node = id.xy;
    if !in_output(node) {
        return;
    }
    let first = window_first_cell() + vec2i(node * 2u);
    var highest = -1e30;
    for (var z = 0; z < 3; z++) {
        for (var x = 0; x < 3; x++) {
            let texel = grid::window_texel(first + vec2i(x, z), params.window_origin);
            highest = max(highest, textureLoad(input_a, texel, 0).r);
        }
    }
    textureStore(output, node, vec4f(highest, 0.0, 0.0, 0.0));
}

// Water level 1 over the centre chunk alone, from the fine water (`input_a`)
// and ground (`input_b`) and the coarse water (`input_c`) and ground
// (`input_d`) window textures.
@compute @workgroup_size(constants::WORKGROUP_SIZE, constants::WORKGROUP_SIZE, 1)
fn centre_base(@builtin(global_invocation_id) id: vec3u) {
    let node = id.xy;
    if !in_output(node) {
        return;
    }
    let first = vec2i(params.centre * grid::CHUNK_DIM) + vec2i(node * 2u);
    var highest = NO_WATER;
    var wet = false;
    for (var z = 0; z < 3; z++) {
        for (var x = 0; x < 3; x++) {
            let column = water_column::column(
                input_a,
                input_b,
                input_c,
                input_d,
                first + vec2i(x, z),
                params.window_origin,
                params.centre,
            );
            highest = max(highest, water_column::drawn(column));
            wet = wet || water_column::is_wet(column);
        }
    }
    textureStore(output, node, vec4f(select(NO_WATER, highest, wet), 0.0, 0.0, 0.0));
}

fn coarse_cell_max(cell: vec2i) -> f32 {
    let fine = cell * i32(grid::COARSE_FACTOR);
    let chunk = vec2u(clamp(fine, vec2i(0), vec2i(i32(grid::WORLD_DIM) - 1))) / grid::CHUNK_DIM;
    if all(chunk == params.centre) {
        let node = vec2u(fine - vec2i(params.centre * grid::CHUNK_DIM)) / 2u;
        return max(
            max(textureLoad(input_a, node, 0).r, textureLoad(input_a, node + vec2u(1u, 0u), 0).r),
            max(textureLoad(input_a, node + vec2u(0u, 1u), 0).r, textureLoad(input_a, node + vec2u(1u, 1u), 0).r),
        );
    }
    let texel = grid::coarse_window_texel(cell, params.window_origin);
    let depth = textureLoad(input_c, texel, 0).r;
    if depth <= water_column::DRY_DEPTH {
        return NO_WATER;
    }
    return textureLoad(input_d, texel, 0).r + depth;
}

// Water level 2 over the window, one node per coarse cell, from the centre's
// level 1 (`input_a`), terrain level 2 (`input_b`) and the coarse water
// (`input_c`) and ground (`input_d`). Where any water touches the node, the
// dry cells drawn under the ground are bounded by the highest ground minus
// water_column::DRY_DROP.
@compute @workgroup_size(constants::WORKGROUP_SIZE, constants::WORKGROUP_SIZE, 1)
fn water_base(@builtin(global_invocation_id) id: vec3u) {
    let node = id.xy;
    if !in_output(node) {
        return;
    }
    let cell = window_first_cell() / i32(grid::COARSE_FACTOR) + vec2i(node);
    let water = max(
        max(coarse_cell_max(cell), coarse_cell_max(cell + vec2i(1, 0))),
        max(coarse_cell_max(cell + vec2i(0, 1)), coarse_cell_max(cell + vec2i(1, 1))),
    );
    var highest = NO_WATER;
    if water > NO_WATER {
        highest = max(water, textureLoad(input_b, node, 0).r - water_column::DRY_DROP);
    }
    textureStore(output, node, vec4f(highest, 0.0, 0.0, 0.0));
}
