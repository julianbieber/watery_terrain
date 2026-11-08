// @role height
// @range 0 45

// A fjord: narrow arms of a glacial trough winding in from open sea on the +x edge,
// under steep walls and a highland that rises inland. Heights are metres. Water stands
// where the ground is below `sea_level`, because every border cell is held at or above
// that level and the +x edge sits exactly on it.

import package::lib::{fbm, ridged_fbm, seed_offset, globals, uv, document_extent};

struct Params {
    // @group Water
    sea_level: f32,     // @ui "Sea level (m)" 10.0 [2.0, 25.0]
    floor_depth: f32,   // @ui "Depth below sea (m)" 6.0 [1.0, 10.0]
    // @group Trough
    mouth_width: f32,   // @ui "Half width at mouth (m)" 20.0 [5.0, 60.0]
    head_width: f32,    // @ui "Half width at head (m)" 7.0 [2.0, 40.0]
    wall: f32,          // @ui "Wall run (m)" 24.0 [5.0, 80.0]
    wander: f32,        // @ui "Wander (m)" 75.0 [0.0, 160.0]
    // @group Highland
    shoulder: f32,      // @ui "Shoulder above sea (m)" 6.0 [1.0, 25.0]
    rise: f32,          // @ui "Rise per metre inland" 0.09 [0.0, 0.4]
    relief: f32,        // @ui "Ridge relief (m)" 12.0 [0.0, 30.0]
    // @group Shaping
    feature: f32,       // @ui "Feature size (m)" 130.0 [30.0, 400.0]
    octaves: u32,       // @ui "Octaves" 5 [1, 8] step 1
}
@group(0) @binding(2) var<uniform> params: Params;

const CELLS_PER_METRE: f32 = 10.0;
const ARMS: u32 = 6u;
const JOIN_METRES: f32 = 14.0;
const OPEN_SEA_START: f32 = 0.90;
const OPEN_SEA_END: f32 = 0.97;
const BORDER_CELLS: f32 = 8.0;

fn arm_seaward_uv(i: u32) -> vec2<f32> {
    switch i {
        case 0u: { return vec2<f32>(1.05, 0.56); }
        case 1u: { return vec2<f32>(0.70, 0.50); }
        case 2u: { return vec2<f32>(0.70, 0.50); }
        case 3u: { return vec2<f32>(0.46, 0.49); }
        case 4u: { return vec2<f32>(0.46, 0.49); }
        default: { return vec2<f32>(0.84, 0.54); }
    }
}

fn arm_landward_uv(i: u32) -> vec2<f32> {
    switch i {
        case 0u: { return vec2<f32>(0.70, 0.50); }
        case 1u: { return vec2<f32>(0.46, 0.49); }
        case 2u: { return vec2<f32>(0.60, 0.88); }
        case 3u: { return vec2<f32>(0.10, 0.28); }
        case 4u: { return vec2<f32>(0.20, 0.80); }
        default: { return vec2<f32>(0.72, 0.10); }
    }
}

fn arm_share_of_mouth_width(i: u32, along: f32) -> f32 {
    switch i {
        case 0u: { return mix(1.0, 0.9, along); }
        case 1u: { return mix(0.9, 0.75, along); }
        case 2u: { return mix(0.6, 0.0, along); }
        case 3u: { return mix(0.75, 0.0, along); }
        case 4u: { return mix(0.55, 0.0, along); }
        default: { return mix(0.6, 0.0, along); }
    }
}

fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

fn signed_metres_to_water_edge(m: vec2<f32>, extent: vec2<f32>) -> f32 {
    var nearest = 1e9;
    for (var i = 0u; i < ARMS; i = i + 1u) {
        let a = arm_seaward_uv(i) * extent;
        let b = arm_landward_uv(i) * extent;
        let along = clamp(dot(m - a, b - a) / dot(b - a, b - a), 0.0, 1.0);
        let away = length(m - (a + (b - a) * along));
        let half_width = mix(params.head_width, params.mouth_width, arm_share_of_mouth_width(i, along));
        nearest = smooth_min(nearest, away - half_width, JOIN_METRES);
    }
    return nearest;
}

fn value(p: vec2<f32>) -> f32 {
    let extent = document_extent() / CELLS_PER_METRE;
    let at = uv(p) * extent;
    let base = at / params.feature + seed_offset(globals.seed, 7u);

    let broad = vec2<f32>(
        fbm(base * 0.6, 2u, 0.5, 2.0),
        fbm(base * 0.6 + vec2<f32>(31.7, 11.3), 2u, 0.5, 2.0),
    );
    let fine = vec2<f32>(
        fbm(base * 2.3 + vec2<f32>(5.1, 47.9), 3u, 0.5, 2.0),
        fbm(base * 2.3 + vec2<f32>(83.2, 2.6), 3u, 0.5, 2.0),
    );
    let edge = signed_metres_to_water_edge(at + broad * params.wander + fine * params.wander * 0.22, extent);

    let up_wall = smoothstep(0.0, params.wall, edge);
    let floor_height = params.sea_level - params.floor_depth;
    let inland = max(edge - params.wall, 0.0);
    let ridges = ridged_fbm(base, params.octaves, 0.45, 2.1) * smoothstep(0.0, 150.0, inland);
    let land = params.sea_level + params.shoulder + inland * params.rise + ridges * params.relief;
    var height = mix(floor_height, land, up_wall);

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
