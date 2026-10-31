import package::grid;

@group(0) @binding(0) var water: texture_storage_2d<r32float, read_write>;

@group(0) @binding(1) var flow_x: texture_storage_2d<r32float, read_write>;
@group(0) @binding(2) var flow_y: texture_storage_2d<r32float, read_write>;

struct Displacement {
    center: vec2f,
    radius: f32,
    push: f32,
}

struct Displacements {
    count: u32,
    items: array<Displacement, constants::MAX_DISPLACEMENTS>,
}

@group(0) @binding(3) var<storage, read> displacements: Displacements;

@group(0) @binding(4) var base_height_texture: texture_storage_2d<r32float, read>;

@group(0) @binding(5) var outflow_scale: texture_storage_2d<r32float, read_write>;


struct SimParams {
    id: i32,
    _pad: vec3f,
};
var<immediate> sim: SimParams;

// -------- helpers --------

// returns base and water height separately
fn get_height(location: vec2i) -> vec2f {
    return vec2f(textureLoad(base_height_texture, location).x,textureLoad(water, location).x);
}

fn set_height(location: vec2i, v: f32) {
    textureStore(water, location, vec4f(v, 0.0, 0.0, 0.0));
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
}

fn distance_from_displacement(p: vec2f) -> DisplacementResult {
    var m = 10000000000.0;
    var current_min: i32 = -1;

    for (var i: u32 = 0; i < displacements.count; i = i + 1) {
        let c = displacements.items[i];
        let d = length(c.center - p) - c.radius;
        if d < m {
            m = d;
            current_min = i32(i);
        }
    }

    return DisplacementResult(m, current_min);
}

// Flow that the nearest displacement circle `d` pushes through point `p`
// along `axis`: radially outward inside the circle, nothing outside it or at
// its exact centre, where the outward direction is undefined.
fn displacement_push(d: DisplacementResult, p: vec2f, axis: vec2f) -> f32 {
    if d.index < 0 || d.distance >= 0.0 {
        return 0.0;
    }
    let circle = displacements.items[d.index];
    let offset = p - circle.center;
    let offset_length = length(offset);
    if offset_length == 0.0 {
        return 0.0;
    }
    return dot(offset / offset_length, axis) * circle.push;
}

// -------- pass 0: update flows on edges --------

fn update_flows(invocation_id: vec3<u32>) {
    let size = vec2i(textureDimensions(water)); // (W,H)
    let x = i32(invocation_id.x);
    let y = i32(invocation_id.y);

    let acceleration = 0.118;
    let damping = 0.99;

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
            f = f * damping + dh * acceleration;
            let edge_center = grid::world_position(vec2f(f32(ex), f32(ey) + 0.5), vec2u(size));
            f += displacement_push(distance_from_displacement(edge_center), edge_center, vec2f(1.0, 0.0));

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
            f = f * damping + dh * acceleration;
            let edge_center = grid::world_position(vec2f(f32(ex) + 0.5, f32(ey)), vec2u(size));
            f += displacement_push(distance_from_displacement(edge_center), edge_center, vec2f(0.0, 1.0));
            set_flow_y(vec2i(ex, ey), f);
        }
    }
}

// Fraction of its outflows a cell can afford: all of them while it holds more
// water than they carry away, proportionally less otherwise.
fn update_outflow_scale(invocation_id: vec3<u32>) {
    let l = vec2i(invocation_id.xy);
    let size = vec2i(textureDimensions(water));
    if (l.x >= size.x || l.y >= size.y) {
        return;
    }
    let outflow = max(0.0, get_flow_x(l + vec2i(1, 0))) + max(0.0, -get_flow_x(l))
        + max(0.0, get_flow_y(l + vec2i(0, 1))) + max(0.0, -get_flow_y(l));
    let available = get_height(l).y;
    var scale = 1.0;
    if outflow > available {
        scale = available / outflow;
    }
    textureStore(outflow_scale, l, vec4f(scale, 0.0, 0.0, 0.0));
}

fn source_scale(f: f32, positive_source: vec2i, negative_source: vec2i) -> f32 {
    return textureLoad(outflow_scale, select(negative_source, positive_source, f > 0.0)).x;
}

// Scales every interior edge's flow by the outflow scale of the cell it
// drains, so no cell gives away more water than it holds.
fn limit_flows(invocation_id: vec3<u32>) {
    let size = vec2i(textureDimensions(water));
    let e = vec2i(invocation_id.xy);
    if (e.x > 0 && e.x < size.x && e.y < size.y) {
        let f = get_flow_x(e);
        set_flow_x(e, f * source_scale(f, e - vec2i(1, 0), e));
    }
    if (e.x < size.x && e.y > 0 && e.y < size.y) {
        let f = get_flow_y(e);
        set_flow_y(e, f * source_scale(f, e - vec2i(0, 1), e));
    }
}

// -------- pass 3: update water height from net flow --------

fn update_water_height(invocation_id: vec3<u32>) {
    let l = vec2i(invocation_id.xy);
    let size = vec2i(textureDimensions(water));

    if (l.x < 0 || l.y < 0 || l.x >= size.x || l.y >= size.y) {
        return;
    }

    let own = get_height(l).y;

    // edges around cell (x,y):
    // flow_x(ex,y): between (ex-1,y) -> (ex,y), + is left->right
    let fx_left  = get_flow_x(vec2i(l.x,     l.y));
    let fx_right = get_flow_x(vec2i(l.x + 1, l.y));

    // flow_y(x,ey): between (x,ey-1) -> (x,ey), + is down->up
    let fy_down  = get_flow_y(vec2i(l.x, l.y));
    let fy_up    = get_flow_y(vec2i(l.x, l.y + 1));

    // net inflow (positive = gain)
    let net = (fx_left - fx_right) + (fy_down - fy_up);

    var new_height = max(0.0, own + net);

    set_height(l, new_height);
}

// -------- entry --------

@compute @workgroup_size(constants::WORKGROUP_SIZE, constants::WORKGROUP_SIZE, 1)
fn main(@builtin(global_invocation_id) invocation_id: vec3<u32>) {
    if sim.id == 0 {
        update_flows(invocation_id);
    } else if sim.id == 1 {
        update_outflow_scale(invocation_id);
    } else if sim.id == 2 {
        limit_flows(invocation_id);
    } else if sim.id == 3 {
        update_water_height(invocation_id);
    }
}
