@group(0) @binding(0) var input:  texture_storage_2d<r32float, read>;
@group(0) @binding(1) var output: texture_storage_2d<r32float, write>;

@group(0) @binding(2) var flow_x: texture_storage_2d<r32float, read_write>;
@group(0) @binding(3) var flow_y: texture_storage_2d<r32float, read_write>;

@group(0) @binding(4) var<storage, read> displacements: array<vec4f>;

struct SimParams {
    id: i32,
    _pad: vec3f,
};
var<push_constant> sim: SimParams;

// -------- helpers --------

fn get_height(location: vec2i) -> f32 {
    return textureLoad(input, location).x;
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

fn distance_from_displacement(p: vec2f) -> f32{
    let l = arrayLength(&displacements);
    var m = 10000000000.0;

    for (var i: u32 = 0; i < l; i = i + 1) {
        let c = displacements[i];
        m = min(m, length(c.xy - p)-c.z);
    }

    return m;
}

// -------- pass 0: update flows on edges --------

fn update_flows(invocation_id: vec3<u32>) {
    let size = vec2i(textureDimensions(input)); // (W,H)
    let x = i32(invocation_id.x);
    let y = i32(invocation_id.y);

    let p = vec2f(f32(x) - 1024.0, f32(y) - 1024.0);
    let d = distance_from_displacement(p);

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
            if d < 0.0 {
                hR += -d;
            }

            var f = get_flow_x(vec2i(ex, ey));
            let dh = hL - hR;
            // simple acceleration + damping
            f = f *1. + dh * 0.0015;
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
            if d < 0.0 {
                hU += -d;
            }

            var f = get_flow_y(vec2i(ex, ey));
            let dh = hD - hU;
            f = f *1. + dh * 0.0015;
            set_flow_y(vec2i(ex, ey), f);
        }
    }
}

// -------- pass 1: update water height from net flow --------

fn update_water_height(invocation_id: vec3<u32>) {
    let l = vec2i(invocation_id.xy);
    let size = vec2i(textureDimensions(input));

    if (l.x < 0 || l.y < 0 || l.x >= size.x || l.y >= size.y) {
        return;
    }

    let own = get_height(l);

    // edges around cell (x,y):
    // flow_x(ex,y): between (ex-1,y) -> (ex,y), + is left->right
    let fx_left  = get_flow_x(vec2i(l.x,     l.y));
    let fx_right = get_flow_x(vec2i(l.x + 1, l.y));

    // flow_y(x,ey): between (x,ey-1) -> (x,ey), + is down->up
    let fy_down  = get_flow_y(vec2i(l.x, l.y));
    let fy_up    = get_flow_y(vec2i(l.x, l.y + 1));

    // net inflow (positive = gain)
    let net = (fx_left - fx_right) + (fy_down - fy_up);

    let new_height = own + net;
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
