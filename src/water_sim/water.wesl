// The shallow-water simulation over the active window, one pass per dispatch
// selected by `SimParams.id`, plus the pass that samples the water under each
// displacement for the CPU. Every texture holds the window's chunks at their
// world cells modulo WINDOW_DIM; a cell's flow texels hold the flow across its
// own -x and -z edges.

import package::grid;

@group(0) @binding(0) var water: texture_storage_2d<r32float, read_write>;

@group(0) @binding(1) var flow_x: texture_storage_2d<r32float, read_write>;
@group(0) @binding(2) var flow_y: texture_storage_2d<r32float, read_write>;

struct Displacement {
    center: vec2f,
    radius: f32,
    push: f32,
    entity: vec2u,
}

struct Displacements {
    count: u32,
    items: array<Displacement, constants::MAX_DISPLACEMENTS>,
}

@group(0) @binding(3) var<storage, read> displacements: Displacements;

@group(0) @binding(4) var base_height_texture: texture_storage_2d<r32float, read>;

@group(0) @binding(5) var outflow_scale: texture_storage_2d<r32float, read_write>;

struct WaterSample {
    entity: vec2u,
    depth: f32,
    ground: f32,
}

struct WaterSamples {
    count: u32,
    items: array<WaterSample, constants::MAX_DISPLACEMENTS>,
}

@group(0) @binding(6) var<storage, read_write> samples: WaterSamples;

struct SimParams {
    id: i32,
    window_origin: vec2u,
};
var<immediate> sim: SimParams;

const WINDOW: i32 = i32(grid::WINDOW_DIM);

// Texel of the window cell `cell`, counted from the window's first cell.
fn texel(cell: vec2i) -> vec2u {
    return grid::window_texel(grid::window_first_cell(sim.window_origin) + cell, sim.window_origin);
}

// World position of continuous window cell coordinates `cell`.
fn world_of_window(cell: vec2f) -> vec2f {
    return grid::world_of_cell(vec2f(grid::window_first_cell(sim.window_origin)) + cell);
}

fn in_window(cell: vec2i) -> bool {
    return all(cell >= vec2i(0)) && all(cell < vec2i(WINDOW));
}

// Ground and water height of a window cell.
fn get_height(cell: vec2i) -> vec2f {
    let t = texel(cell);
    return vec2f(textureLoad(base_height_texture, t).x, textureLoad(water, t).x);
}

fn set_height(cell: vec2i, v: f32) {
    textureStore(water, texel(cell), vec4f(v, 0.0, 0.0, 0.0));
}

// Flow across the -x edge of window cell `edge`, positive towards +x; zero on
// the window border and beyond, whose cells are not simulated.
fn get_flow_x(edge: vec2i) -> f32 {
    if edge.x <= 0 || edge.x >= WINDOW || edge.y < 0 || edge.y >= WINDOW {
        return 0.0;
    }
    return textureLoad(flow_x, texel(edge)).x;
}

// Flow across the -z edge of window cell `edge`, positive towards +z; zero on
// the window border and beyond.
fn get_flow_y(edge: vec2i) -> f32 {
    if edge.y <= 0 || edge.y >= WINDOW || edge.x < 0 || edge.x >= WINDOW {
        return 0.0;
    }
    return textureLoad(flow_y, texel(edge)).x;
}

fn set_flow_x(edge: vec2i, v: f32) {
    textureStore(flow_x, texel(edge), vec4f(v, 0.0, 0.0, 0.0));
}

fn set_flow_y(edge: vec2i, v: f32) {
    textureStore(flow_y, texel(edge), vec4f(v, 0.0, 0.0, 0.0));
}

// Factor a flow through window point `p` is scaled by: one further than
// EDGE_FADE_CELLS from the window border, falling linearly to zero on it.
fn edge_fade(p: vec2f) -> f32 {
    let to_border = min(p, vec2f(f32(WINDOW)) - p);
    return clamp(min(to_border.x, to_border.y) / f32(constants::EDGE_FADE_CELLS), 0.0, 1.0);
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
        if c.push == 0.0 {
            continue;
        }
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

fn surface_drop(from_cell: vec2f, to_cell: vec2f) -> f32 {
    let dh = (from_cell.x + from_cell.y) - (to_cell.x + to_cell.y);
    if dh < 0.0 {
        return -min(abs(dh), to_cell.y);
    }
    return min(dh, from_cell.y);
}

fn update_flows(invocation_id: vec3<u32>) {
    let e = vec2i(invocation_id.xy);
    if !in_window(e) {
        return;
    }

    let acceleration = 0.118;
    let damping = 0.99;

    if e.x > 0 {
        let midpoint = vec2f(f32(e.x), f32(e.y) + 0.5);
        var f = get_flow_x(e) * damping
            + surface_drop(get_height(e - vec2i(1, 0)), get_height(e)) * acceleration;
        let edge_center = world_of_window(midpoint);
        f += displacement_push(distance_from_displacement(edge_center), edge_center, vec2f(1.0, 0.0));
        set_flow_x(e, f * edge_fade(midpoint));
    }

    if e.y > 0 {
        let midpoint = vec2f(f32(e.x) + 0.5, f32(e.y));
        var f = get_flow_y(e) * damping
            + surface_drop(get_height(e - vec2i(0, 1)), get_height(e)) * acceleration;
        let edge_center = world_of_window(midpoint);
        f += displacement_push(distance_from_displacement(edge_center), edge_center, vec2f(0.0, 1.0));
        set_flow_y(e, f * edge_fade(midpoint));
    }
}

// Fraction of its outflows a cell can afford: all of them while it holds more
// water than they carry away, proportionally less otherwise.
fn update_outflow_scale(invocation_id: vec3<u32>) {
    let l = vec2i(invocation_id.xy);
    if !in_window(l) {
        return;
    }
    let outflow = max(0.0, get_flow_x(l + vec2i(1, 0))) + max(0.0, -get_flow_x(l))
        + max(0.0, get_flow_y(l + vec2i(0, 1))) + max(0.0, -get_flow_y(l));
    let available = get_height(l).y;
    var scale = 1.0;
    if outflow > available {
        scale = available / outflow;
    }
    textureStore(outflow_scale, texel(l), vec4f(scale, 0.0, 0.0, 0.0));
}

fn source_scale(f: f32, positive_source: vec2i, negative_source: vec2i) -> f32 {
    return textureLoad(outflow_scale, texel(select(negative_source, positive_source, f > 0.0))).x;
}

// Scales every interior edge's flow by the outflow scale of the cell it
// drains, so no cell gives away more water than it holds.
fn limit_flows(invocation_id: vec3<u32>) {
    let e = vec2i(invocation_id.xy);
    if !in_window(e) {
        return;
    }
    if e.x > 0 {
        let f = get_flow_x(e);
        set_flow_x(e, f * source_scale(f, e - vec2i(1, 0), e));
    }
    if e.y > 0 {
        let f = get_flow_y(e);
        set_flow_y(e, f * source_scale(f, e - vec2i(0, 1), e));
    }
}

fn update_water_height(invocation_id: vec3<u32>) {
    let l = vec2i(invocation_id.xy);
    if !in_window(l) {
        return;
    }

    let net = (get_flow_x(l) - get_flow_x(l + vec2i(1, 0)))
        + (get_flow_y(l) - get_flow_y(l + vec2i(0, 1)));

    set_height(l, max(0.0, get_height(l).y + net));
}

// Records the water depth and ground height under every displacement, for
// the CPU to read back; one thread per displacement.
fn sample_water(invocation_id: vec3<u32>) {
    let i = invocation_id.x + invocation_id.y * constants::WORKGROUP_SIZE;
    if i == 0 {
        samples.count = displacements.count;
    }
    if i >= displacements.count {
        return;
    }
    let displacement = displacements.items[i];
    let cell = vec2i(floor(grid::world_cell(displacement.center)))
        - grid::window_first_cell(sim.window_origin);
    var sample = WaterSample(displacement.entity, 0.0, 0.0);
    if in_window(cell) {
        let column = get_height(cell);
        sample.depth = column.y;
        sample.ground = column.x;
    }
    samples.items[i] = sample;
}

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
    } else if sim.id == 4 {
        sample_water(invocation_id);
    }
}
