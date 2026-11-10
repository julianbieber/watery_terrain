// The shallow-water simulation over the window's centre chunk and its
// exchange with displacing objects, one pass per dispatch selected by
// `SimParams.id`. Every texture holds the window's chunks at their world cells
// modulo WINDOW_DIM; a cell's flow texel holds, as two f16, the flow across its
// own -x and -z edges. The other window chunks keep the water they had and
// exchange none with the centre. Within the centre chunk, a workgroup that is
// dry, carries no flow and borders no such cell is skipped for the step.

import package::grid;

@group(0) @binding(0) var water: texture_storage_2d<r32float, read_write>;

@group(0) @binding(1) var flows: texture_storage_2d<r32uint, read_write>;

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

@group(0) @binding(2) var<storage, read> displacements: Displacements;

@group(0) @binding(3) var base_height_texture: texture_storage_2d<r32float, read>;

@group(0) @binding(4) var cell_scratch: texture_storage_2d<r32float, read_write>;

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

@group(0) @binding(5) var<storage, read_write> samples: WaterSamples;

// The step each workgroup of the centre chunk was last marked active in, row
// after row of GROUPS workgroups.
@group(0) @binding(6) var<storage, read_write> active_groups: array<atomic<u32>>;

struct SimParams {
    id: i32,
    object: u32,
    window_origin: vec2u,
    centre: vec2u,
    step: u32,
    padding: u32,
};
var<immediate> sim: SimParams;

const WINDOW: i32 = i32(grid::WINDOW_DIM);

const CHUNK: i32 = i32(grid::CHUNK_DIM);

const GROUPS: i32 = CHUNK / i32(constants::WORKGROUP_SIZE);

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

// First window cell of the centre chunk.
fn active_first() -> vec2i {
    return vec2i(sim.centre - sim.window_origin) * CHUNK;
}

fn in_active(cell: vec2i) -> bool {
    let local = cell - active_first();
    return all(local >= vec2i(0)) && all(local < vec2i(CHUNK));
}

// The window cell a cell pass's invocation works on.
fn active_cell(invocation_id: vec3<u32>) -> vec2i {
    return active_first() + vec2i(invocation_id.xy);
}

fn group_index(group: vec2i) -> u32 {
    return u32(group.y * GROUPS + group.x);
}

// Whether the workgroup of a cell pass's invocation was marked for this step.
fn group_active(invocation_id: vec3<u32>) -> bool {
    let group = vec2i(invocation_id.xy) / i32(constants::WORKGROUP_SIZE);
    return atomicLoad(&active_groups[group_index(group)]) == sim.step;
}

// Ground and water height of a window cell.
fn get_height(cell: vec2i) -> vec2f {
    let t = texel(cell);
    return vec2f(textureLoad(base_height_texture, t).x, textureLoad(water, t).x);
}

fn set_height(cell: vec2i, v: f32) {
    textureStore(water, texel(cell), vec4f(v, 0.0, 0.0, 0.0));
}

fn stored_flows(cell: vec2i) -> vec2f {
    return unpack2x16float(textureLoad(flows, texel(cell)).x);
}

fn set_flows(cell: vec2i, v: vec2f) {
    textureStore(flows, texel(cell), vec4u(pack2x16float(v), 0u, 0u, 0u));
}

// Flow across the -x edge of window cell `edge`, positive towards +x; zero on
// the centre chunk's border and beyond it.
fn get_flow_x(edge: vec2i) -> f32 {
    let local = edge - active_first();
    if local.x <= 0 || local.x >= CHUNK || local.y < 0 || local.y >= CHUNK {
        return 0.0;
    }
    return stored_flows(edge).x;
}

// Flow across the -z edge of window cell `edge`, positive towards +z; zero on
// the centre chunk's border and beyond it.
fn get_flow_y(edge: vec2i) -> f32 {
    let local = edge - active_first();
    if local.y <= 0 || local.y >= CHUNK || local.x < 0 || local.x >= CHUNK {
        return 0.0;
    }
    return stored_flows(edge).y;
}

// Factor a flow through window point `p` is scaled by: one further than
// EDGE_FADE_CELLS from the centre chunk's border, falling linearly to zero on
// it.
fn edge_fade(p: vec2f) -> f32 {
    let local = p - vec2f(active_first());
    let to_border = min(local, vec2f(f32(CHUNK)) - local);
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

var<workgroup> wet_neighbourhood: atomic<u32>;

// Bit of the workgroup `offset` (each axis in -1..=1) away in a 3x3 mask.
fn neighbour_bit(offset: vec2i) -> u32 {
    return 1u << u32((offset.y + 1) * 3 + offset.x + 1);
}

// Marks for this step every workgroup that holds a cell with water or flow,
// and every workgroup bordering such a cell, so water can cross into it.
fn mark_active_groups(invocation_id: vec3<u32>, local_id: vec3<u32>, group_id: vec3<u32>) {
    if all(local_id.xy == vec2u(0u)) {
        atomicStore(&wet_neighbourhood, 0u);
    }
    workgroupBarrier();
    let e = active_cell(invocation_id);
    let wet = textureLoad(water, texel(e)).x > 0.0 || textureLoad(flows, texel(e)).x != 0u;
    if wet {
        let last = constants::WORKGROUP_SIZE - 1u;
        let low = vec2i(select(vec2u(0u), vec2u(1u), local_id.xy == vec2u(0u)));
        let high = vec2i(select(vec2u(0u), vec2u(1u), local_id.xy == vec2u(last)));
        var bits = 0u;
        for (var y = -low.y; y <= high.y; y += 1) {
            for (var x = -low.x; x <= high.x; x += 1) {
                bits |= neighbour_bit(vec2i(x, y));
            }
        }
        atomicOr(&wet_neighbourhood, bits);
    }
    workgroupBarrier();
    if all(local_id.xy == vec2u(0u)) {
        let bits = atomicLoad(&wet_neighbourhood);
        for (var y = -1; y <= 1; y += 1) {
            for (var x = -1; x <= 1; x += 1) {
                let group = vec2i(group_id.xy) + vec2i(x, y);
                if (bits & neighbour_bit(vec2i(x, y))) != 0u && all(group >= vec2i(0)) && all(group < vec2i(GROUPS)) {
                    atomicStore(&active_groups[group_index(group)], sim.step);
                }
            }
        }
    }
}

fn update_flows(invocation_id: vec3<u32>) {
    if !group_active(invocation_id) {
        return;
    }
    let e = active_cell(invocation_id);
    let local = e - active_first();
    let here = get_height(e);
    let here_rate = depth_change(e);
    var f = vec2f(0.0);

    if local.x > 0 {
        let midpoint = vec2f(f32(e.x), f32(e.y) + 0.5);
        f.x = next_flow(get_flow_x(e), get_height(e - vec2i(1, 0)), here, depth_change(e - vec2i(1, 0)) - here_rate) * edge_fade(midpoint);
    }

    if local.y > 0 {
        let midpoint = vec2f(f32(e.x) + 0.5, f32(e.y));
        f.y = next_flow(get_flow_y(e), get_height(e - vec2i(0, 1)), here, depth_change(e - vec2i(0, 1)) - here_rate) * edge_fade(midpoint);
    }
    set_flows(e, f);
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
    if !in_active(e) {
        return;
    }
    let local = e - active_first();
    var f = vec2f(get_flow_x(e), get_flow_y(e));
    if local.x > 0 {
        f.x = flow_past_object(f.x, e - vec2i(1, 0), e, vec2f(1.0, 0.0), object);
    }
    if local.y > 0 {
        f.y = flow_past_object(f.y, e - vec2i(0, 1), e, vec2f(0.0, 1.0), object);
    }
    set_flows(e, f);
}

// Fraction of its outflows a cell can afford: all of them while it holds more
// water than they carry away, proportionally less otherwise.
fn update_outflow_scale(invocation_id: vec3<u32>) {
    if !group_active(invocation_id) {
        return;
    }
    let l = active_cell(invocation_id);
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
    if !group_active(invocation_id) {
        return;
    }
    let e = active_cell(invocation_id);
    let f = vec2f(get_flow_x(e), get_flow_y(e));
    set_flows(e, vec2f(
        f.x * source_scale(f.x, e - vec2i(1, 0), e),
        f.y * source_scale(f.y, e - vec2i(0, 1), e),
    ));
}

fn update_water_height(invocation_id: vec3<u32>) {
    if !group_active(invocation_id) {
        return;
    }
    let l = active_cell(invocation_id);

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
fn main(
    @builtin(global_invocation_id) invocation_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
    @builtin(workgroup_id) group_id: vec3<u32>,
) {
    if sim.id == 6 {
        mark_active_groups(invocation_id, local_id, group_id);
    } else if sim.id == 0 {
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
