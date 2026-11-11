// @role height
// @range 0 200

// Mountains and ocean: open sea towards the +x edge behind a wandering coast, a coastal
// lowland, and ridged mountains climbing inland whose valleys hold tarns. Heights are
// metres. Water stands where the ground is below `sea_level`, because every border cell
// is held at or above that level and the +x edge sits exactly on it.

import package::lib::{fbm, ridged_fbm, seed_offset, globals, uv, document_extent};

struct Params {
    // @group Sea
    sea_level: f32,     // @ui "Sea level (m)" 20.0 [5.0, 60.0]
    sea_depth: f32,     // @ui "Depth of the open sea (m)" 14.0 [2.0, 20.0]
    shelf: f32,         // @ui "Shelf run (m)" 90.0 [10.0, 400.0]
    // @group Coast
    coast: f32,         // @ui "Coast across the document" 0.72 [0.3, 0.95]
    wander: f32,        // @ui "Coast wander (m)" 140.0 [0.0, 400.0]
    // @group Mountains
    lowland: f32,       // @ui "Lowland above sea (m)" 4.0 [0.5, 20.0]
    peak: f32,          // @ui "Peak above sea (m)" 170.0 [20.0, 180.0]
    climb: f32,         // @ui "Climb run inland (m)" 450.0 [50.0, 1200.0]
    // @group Shaping
    feature: f32,       // @ui "Feature size (m)" 260.0 [50.0, 800.0]
    octaves: u32,       // @ui "Octaves" 6 [1, 8] step 1
}
@group(0) @binding(2) var<uniform> params: Params;

const CELLS_PER_METRE: f32 = 10.0;
const OPEN_SEA_START: f32 = 0.93;
const OPEN_SEA_END: f32 = 0.98;
const BORDER_CELLS: f32 = 8.0;

fn metres_inland(at: vec2<f32>, base: vec2<f32>, extent: vec2<f32>) -> f32 {
    let bend = fbm(base * 0.4 + vec2<f32>(17.3, 5.9), 3u, 0.5, 2.0)
        + 0.3 * fbm(base * 1.7 + vec2<f32>(61.1, 23.4), 3u, 0.5, 2.0);
    return params.coast * extent.x + bend * params.wander - at.x;
}

fn value(p: vec2<f32>) -> f32 {
    let extent = document_extent() / CELLS_PER_METRE;
    let at = uv(p) * extent;
    let base = at / params.feature + seed_offset(globals.seed, 11u);

    let inland = metres_inland(at, base, extent);
    let floor_height = params.sea_level - params.sea_depth;

    var height: f32;
    if inland < 0.0 {
        height = mix(params.sea_level, floor_height, smoothstep(0.0, params.shelf, -inland));
    } else {
        let rolling = fbm(base * 1.3 + vec2<f32>(3.7, 91.2), 4u, 0.5, 2.0);
        let low = params.sea_level + params.lowland * smoothstep(0.0, 25.0, inland)
            + rolling * params.lowland;
        let peaks = 1.0 - ridged_fbm(base, params.octaves, 0.5, 2.0);
        let mass = 0.55 + 0.45 * fbm(base * 0.35 + vec2<f32>(44.4, 8.8), 3u, 0.5, 2.0);
        let climb = smoothstep(0.0, params.climb, inland);
        let slope = clamp(inland / (params.coast * extent.x), 0.0, 1.0);
        height = low + climb * (params.peak * (0.45 * slope + 0.55 * peaks * mass));
    }

    let open_sea = smoothstep(OPEN_SEA_START, OPEN_SEA_END, uv(p).x);
    height = mix(height, floor_height, open_sea);

    let doc = document_extent();
    if doc.x - p.x < BORDER_CELLS {
        return params.sea_level;
    }
    if min(min(p.x, p.y), doc.y - p.y) < BORDER_CELLS {
        return max(height, params.sea_level + 0.5);
    }
    return height;
}
