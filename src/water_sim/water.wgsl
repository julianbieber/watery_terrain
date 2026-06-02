@group(0) @binding(0) var input:  texture_storage_2d<r32float, read>;
@group(0) @binding(1) var output: texture_storage_2d<r32float, write>;

@group(0) @binding(2) var flow_x: texture_storage_2d<r32float, read_write>;
@group(0) @binding(3) var flow_y: texture_storage_2d<r32float, read_write>;

@group(0) @binding(4) var<storage, read> displacements: array<vec4f>;

@group(0) @binding(5) var base_height_texture: texture_storage_2d<r32float, read>;

// Tile size in texture pixels
const TILE_SIZE: i32 = 1024;

struct SimParams {
    id: i32,
    tile_index: i32,
    _pad: vec2f,
};
var<push_constant> sim: SimParams;

// -------- helpers --------

// returns base and water height separately
fn get_height(location: vec2i) -> vec2f {
    return vec2f(
        textureLoad(base_height_texture, location).x,
        textureLoad(input, location).x
    );
}

fn set_height(location: vec2i, v: f32) {
    textureStore(output, location, vec4f(v, 0.0, 0.0, 0.0));
}

fn get_flow_x(edge: vec2i) -> f32 {
    return textureLoad(flow_x, edge).x;
}

fn get_flow_y(edge: vec2i) -> f32 {
    return textureLoad(flow_y, edge).x;
}

fn set_flow_x(edge: vec2i, v: f32) {
    textureStore(flow_x, edge, vec4f(v, 0.0, 0.0, 0.0));
}

fn set_flow_y(edge: vec2i, v: f32) {
    textureStore(flow_y, edge, vec4f(v, 0.0, 0.0, 0.0));
}

struct DisplacementResult {
    distance: f32,
    index: i32,
};

fn distance_from_displacement(p: vec2f) -> DisplacementResult {
    let l = arrayLength(&displacements);
    var m = 10000000000.0;
    var current_min: i32 = -1;

    for (var i: u32 = 0; i < l; i = i + 1) {
        let c = displacements[i];
        let d = length(c.xy - p) - c.z;
        if d < m {
            m = d;
            current_min = i32(i);
        }
    }

    return DisplacementResult(m, current_min);
}

fn distance_from_specific(p: vec2f, i: u32) -> f32 {
    let c = displacements[i];
    let d = length(c.xy - p) - c.z;
    return d;
}

// Edge dampening based on distance from tile center
// Dampens water movement near tile edges to reduce artifacts
fn get_edge_dampening(local_pos: vec2i) -> f32 {
    // local_pos is within [0, TILE_SIZE-1] x [0, TILE_SIZE-1]
    let center = vec2f(f32(TILE_SIZE) / 2.0);
    let dist_from_center = distance(vec2f(local_pos), center);
    let max_dist = f32(TILE_SIZE) / 2.0;
    
    // Dampen more as we get closer to the edge
    // At center: dampening = 1.0 (no dampening)
    // At edge: dampening = 0.0 (full dampening)
    // Use smoothstep for smooth transition starting at 70% from center
    let t = dist_from_center / max_dist;
    return 1.0 - smoothstep(0.7, 1.0, t);
}

// -------- pass 0: update flows on edges --------

fn update_flows(invocation_id: vec3<u32>) {
    let size = vec2i(TILE_SIZE, TILE_SIZE);
    let x = i32(invocation_id.x);
    let y = i32(invocation_id.y);

    let p = vec2f(f32(x), f32(y));
    let local_pos = vec2i(x, y);
    let dampening_factor = get_edge_dampening(local_pos);
    let dampening = 0.118 * dampening_factor;
    let momentum = 0.99;

    // Horizontal edges (flow_x): (ex, y), ex in [0..W]
    if (x <= size.x && y < size.y) {
        let ex = x;
        let ey = y;
        // solid walls at domain borders
        if (ex == 0 || ex == size.x) {
            set_flow_x(vec2i(ex, ey), 0.0);
        } else {
            let left_cell  = vec2i(ex - 1, ey);
            let right_cell = vec2i(ex,     ey);

            let hL = get_height(left_cell);
            var hR = get_height(right_cell);
            var f = get_flow_x(vec2i(ex, ey));
            var dh = (hL.x + hL.y) - (hR.x+hR.y);
            if dh < 0.0 {
                dh = -min(abs(dh), hR.y);
            } else if dh > 0.0 {
                dh = min(dh, hL.y);
            }
            f = f * momentum + dh * dampening;
            
            // Add displacement effects
            let d = distance_from_displacement(p);
            if d.index >= 0 && d.distance < 0.0 {
                let displacement_circle = displacements[d.index];
                let dir = normalize(p - displacement_circle.xy);
                f += dot(dir, vec2f(1.0, 0.0)) * displacement_circle.w;
            }
            
            set_flow_x(vec2i(ex, ey), f);
        }
    }

    // Vertical edges (flow_y): (x, ey), ey in [0..H]
    if (x < size.x && y <= size.y) {
        let ex = x;
        let ey = y;
        if (ey == 0 || ey == size.y) {
            set_flow_y(vec2i(ex, ey), 0.0);
        } else {
            let down_cell = vec2i(ex, ey - 1);
            let up_cell   = vec2i(ex, ey);

            let hD = get_height(down_cell);
            var hU = get_height(up_cell);

            var f = get_flow_y(vec2i(ex, ey));
            var dh = (hD.x + hD.y) - (hU.x+hU.y);
            if dh < 0.0 {
                dh = -min(abs(dh), hU.y);
            } else if dh > 0.0 {
                dh = min(dh, hD.y);
            }
            f = f * momentum + dh * dampening;
            
            let d = distance_from_displacement(p);
            if d.index >= 0 && d.distance < 0.0 {
                let displacement_circle = displacements[d.index];
                let dir = normalize(p - displacement_circle.xy);
                f += dot(dir, vec2f(0.0, 1.0)) * displacement_circle.w;
            }
            
            set_flow_y(vec2i(ex, ey), f);
        }
    }
}

// -------- pass 1: update water height from net flow --------

fn update_water_height(invocation_id: vec3<u32>) {
    let l = vec2i(invocation_id.xy);
    let size = vec2i(TILE_SIZE, TILE_SIZE);

    if (l.x < 0 || l.y < 0 || l.x >= size.x || l.y >= size.y) {
        return;
    }

    let own = get_height(l).y;
    let local_pos = l;
    let dampening_factor = get_edge_dampening(local_pos);

    // edges around cell (x,y):
    // flow_x(ex,y): between (ex-1,y) -> (ex,y), + is left->right
    let fx_left  = get_flow_x(vec2i(l.x,     l.y));
    let fx_right = get_flow_x(vec2i(l.x + 1, l.y));

    // flow_y(x,ey): between (x,ey-1) -> (x,ey), + is down->up
    let fy_down  = get_flow_y(vec2i(l.x, l.y));
    let fy_up    = get_flow_y(vec2i(l.x, l.y + 1));

    // net inflow (positive = gain)
    let net = (fx_left - fx_right) + (fy_down - fy_up);
    
    // Apply edge dampening - reduce water movement near edges
    let net_dampened = net * dampening_factor;

    var new_height = max(0.0, own + net_dampened);

    set_height(l, new_height);
}

// -------- entry --------

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) invocation_id: vec3<u32>) {
    if sim.id == 0 {
        update_flows(invocation_id);
    } else if sim.id == 1 {
        update_water_height(invocation_id);
    }
}
