// The shallow-water simulation over the active window and its exchange with
// displacing objects, one pass per dispatch selected by `SimParams.id`. Every
// texture holds the window's chunks at their world cells modulo WINDOW_DIM; a
// cell's flow texels hold the flow across its own -x and -z edges.

import package::grid;

@group(0) @binding(0) var water: texture_storage_2d<r32float, read_write>;

@group(0) @binding(1) var flow_x: texture_storage_2d<r32float, read_write>;
@group(0) @binding(2) var flow_y: texture_storage_2d<r32float, read_write>;

struct Displacement {
    center: vec2f,
    velocity: vec2f,
    radius: f32,
    height: f32,
    drag: f32,
    entity: vec2u,
}

struct Displacements {
    count: u32,
    items: array<Displacement, constants::MAX_DISPLACEMENTS>,
}

@group(0) @binding(3) var<storage, read> displacements: Displacements;

@group(0) @binding(4) var base_height_texture: texture_storage_2d<r32float, read>;

@group(0) @binding(5) var cell_scratch: texture_storage_2d<r32float, read_write>;

struct WaterSample {
    entity: vec2u,
    velocity: vec2f,
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
    object: u32,
    window_origin: vec2u,
};
var<immediate> sim: SimParams;

const WINDOW: i32 = i32(grid::WINDOW_DIM);

const GRAVITY: f32 = 9.81;

const STEP_SECONDS: f32 = 1.0 / f32(constants::STEPS_PER_SECOND);

const CELL_SPEED: f32 = f32(constants::STEPS_PER_SECOND) / grid::CELLS_PER_METER;

const FLOW_GAIN: f32 = GRAVITY / (CELL_SPEED * CELL_SPEED);

const MAX_WAVE_SPEED: f32 = 0.5 * CELL_SPEED;

const MAX_WAVE_DEPTH: f32 = MAX_WAVE_SPEED * MAX_WAVE_SPEED / GRAVITY;

const MANNING_ROUGHNESS: f32 = 0.03;

const MIN_FRICTION_DEPTH: f32 = 0.001;

const BULK_VISCOSITY: f32 = 0.1;

const OUTFLOW_MARGIN: f32 = 0.9999;

const DRY_DEPTH: f32 = f32(constants::DRY_DEPTH_MM) / 1000.0;

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

fn face_depth(a: vec2f, b: vec2f) -> f32 {
    return max(max(a.x + a.y, b.x + b.y) - max(a.x, b.x), 0.0);
}

fn critical_flow(flow: f32, depth: f32) -> f32 {
    let limit = depth * sqrt(GRAVITY * min(depth, MAX_WAVE_DEPTH)) / CELL_SPEED;
    return clamp(flow, -limit, limit);
}

fn depth_change(cell: vec2i) -> f32 {
    return textureLoad(cell_scratch, texel(cell)).x;
}

fn next_flow(flow: f32, from_cell: vec2f, to_cell: vec2f, rate_difference: f32) -> f32 {
    let depth = face_depth(from_cell, to_cell);
    if depth <= 0.0 {
        return 0.0;
    }
    let drop = (from_cell.x + from_cell.y) - (to_cell.x + to_cell.y);
    let driven = flow + FLOW_GAIN * min(depth, MAX_WAVE_DEPTH) * drop + BULK_VISCOSITY * rate_difference;
    let friction = GRAVITY * STEP_SECONDS * MANNING_ROUGHNESS * MANNING_ROUGHNESS
        * abs(flow) * CELL_SPEED / pow(max(depth, MIN_FRICTION_DEPTH), 7.0 / 3.0);
    return critical_flow(driven / (1.0 + friction), depth);
}

fn update_flows(invocation_id: vec3<u32>) {
    let e = vec2i(invocation_id.xy);
    if !in_window(e) {
        return;
    }
    let here = get_height(e);
    let here_rate = depth_change(e);

    if e.x > 0 {
        let midpoint = vec2f(f32(e.x), f32(e.y) + 0.5);
        let f = next_flow(get_flow_x(e), get_height(e - vec2i(1, 0)), here, depth_change(e - vec2i(1, 0)) - here_rate);
        set_flow_x(e, f * edge_fade(midpoint));
    }

    if e.y > 0 {
        let midpoint = vec2f(f32(e.x) + 0.5, f32(e.y));
        let f = next_flow(get_flow_y(e), get_height(e - vec2i(0, 1)), here, depth_change(e - vec2i(0, 1)) - here_rate);
        set_flow_y(e, f * edge_fade(midpoint));
    }
}

fn immersed_thickness(cell: vec2i, column: vec2f, object: Displacement) -> f32 {
    let offset = world_of_window(vec2f(cell)) - object.center;
    let chord_squared = object.radius * object.radius - dot(offset, offset);
    if chord_squared <= 0.0 {
        return 0.0;
    }
    let half_chord = sqrt(chord_squared);
    let top = min(column.x + column.y, object.height + half_chord);
    let bottom = max(column.x, object.height - half_chord);
    return max(top - bottom, 0.0);
}

fn flow_past_object(flow: f32, upwind: vec2i, downwind: vec2i, axis: vec2f, object: Displacement) -> f32 {
    let from_column = get_height(upwind);
    let to_column = get_height(downwind);
    let depth = face_depth(from_column, to_column);
    if depth <= 0.0 {
        return flow;
    }
    let from_thickness = immersed_thickness(upwind, from_column, object);
    let to_thickness = immersed_thickness(downwind, to_column, object);
    if from_thickness == 0.0 && to_thickness == 0.0 {
        return flow;
    }
    let pressure = FLOW_GAIN * min(depth, MAX_WAVE_DEPTH) * (from_thickness - to_thickness);
    let slip = dot(object.velocity, axis) - flow * CELL_SPEED / depth;
    let rate = object.drag * abs(slip) * STEP_SECONDS / depth;
    let dragged = slip * depth / CELL_SPEED * rate / (1.0 + rate);
    return critical_flow(flow + pressure + dragged, depth);
}

fn apply_object(invocation_id: vec3<u32>) {
    let object = displacements.items[sim.object];
    let first = vec2i(floor(grid::world_cell(object.center - vec2f(object.radius))))
        - grid::window_first_cell(sim.window_origin) - vec2i(1);
    let e = first + vec2i(invocation_id.xy);
    if !in_window(e) {
        return;
    }
    if e.x > 0 {
        let f = get_flow_x(e);
        set_flow_x(e, flow_past_object(f, e - vec2i(1, 0), e, vec2f(1.0, 0.0), object));
    }
    if e.y > 0 {
        let f = get_flow_y(e);
        set_flow_y(e, flow_past_object(f, e - vec2i(0, 1), e, vec2f(0.0, 1.0), object));
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
        scale = available / outflow * OUTFLOW_MARGIN;
    }
    textureStore(cell_scratch, texel(l), vec4f(scale, 0.0, 0.0, 0.0));
}

fn source_scale(f: f32, positive_source: vec2i, negative_source: vec2i) -> f32 {
    return textureLoad(cell_scratch, texel(select(negative_source, positive_source, f > 0.0))).x;
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
    textureStore(cell_scratch, texel(l), vec4f(net, 0.0, 0.0, 0.0));
}

fn water_velocity(cell: vec2i, depth: f32) -> vec2f {
    let flow = vec2f(
        get_flow_x(cell) + get_flow_x(cell + vec2i(1, 0)),
        get_flow_y(cell) + get_flow_y(cell + vec2i(0, 1)),
    ) * 0.5;
    return flow * CELL_SPEED / max(depth, DRY_DEPTH);
}

fn sample_water(invocation_id: vec3<u32>) {
    let i = invocation_id.x + invocation_id.y * constants::WORKGROUP_SIZE;
    if i == 0 {
        samples.count = displacements.count;
    }
    if i >= displacements.count {
        return;
    }
    let displacement = displacements.items[i];
    let first = grid::window_first_cell(sim.window_origin);
    let center = vec2i(floor(grid::world_cell(displacement.center))) - first;
    var sample = WaterSample(displacement.entity, vec2f(0.0), 0.0, 0.0);
    if in_window(center) {
        let column = get_height(center);
        sample.depth = column.y;
        sample.ground = column.x;
        sample.velocity = water_velocity(center, column.y);

        let reach = displacement.radius + 2.0 / grid::CELLS_PER_METER;
        var directions = array(vec2f(1.0, 0.0), vec2f(-1.0, 0.0), vec2f(0.0, 1.0), vec2f(0.0, -1.0));
        var wet = 0.0;
        var surface = 0.0;
        var velocity = vec2f(0.0);
        for (var d = 0; d < 4; d += 1) {
            let rim_point = displacement.center + directions[d] * reach;
            let rim = vec2i(floor(grid::world_cell(rim_point))) - first;
            if in_window(rim) {
                let rim_column = get_height(rim);
                if rim_column.y > DRY_DEPTH {
                    wet += 1.0;
                    surface += rim_column.x + rim_column.y;
                    velocity += water_velocity(rim, rim_column.y);
                }
            }
        }
        if wet > 0.0 {
            sample.depth = max(surface / wet - column.x, 0.0);
            sample.velocity = velocity / wet;
        }
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
    } else if sim.id == 5 {
        apply_object(invocation_id);
    }
}
