// Ray marching of the window's terrain and water heightfields for the SDF
// surface material: the view ray of a pixel, the max-mip traversal that finds
// where it first meets a surface, and the depth that hit writes, shared by the
// prepass and the forward entry points.

import watery_terrain::grid;
import watery_terrain::render::{heightfield, water_column};
import bevy_pbr::render::{
    mesh_view_bindings::view,
    view_transformations::{
        direction_view_to_world, frag_coord_to_uv, position_ndc_to_view, position_ndc_to_world,
        uv_to_ndc,
    },
};

struct Params {
    window_origin: vec2u,
    centre: vec2u,
    surface: u32,
    voxel_level: u32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var ground_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var terrain_max: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var water_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var coarse_water_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var coarse_ground_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(105) var water_max: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(106) var centre_water_max: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(107) var<uniform> params: Params;

const SURFACE_LAND: u32 = 0u;

const SMOOTH: u32 = 0xffffffffu;

const TOP_LEVEL: u32 = 10u;

const MAX_ITERATIONS: u32 = 320u;

const NUDGE: f32 = 1e-2;

const FAR: f32 = 1e30;

const NO_HEIGHT: f32 = -1e30;

// The fullscreen triangle of the proxy mesh, whose vertices are given in
// clip space.
struct ProxyOutput {
    @builtin(position) position: vec4f,
    @location(0) @interpolate(flat) instance_index: u32,
}

fn proxy(position: vec3f, instance_index: u32) -> ProxyOutput {
    return ProxyOutput(vec4f(position.xy, 0.5, 1.0), instance_index);
}

struct Ray {
    origin: vec3f,
    direction: vec3f,
    // Where the ray starts; below zero for orthographic views, whose rays
    // reach back past the near plane to the casters behind it.
    t_min: f32,
    // The pyramid level the march starts at: TOP_LEVEL, or lower for a ray
    // starting just before the surface it will meet.
    level: u32,
}

fn is_orthographic() -> bool {
    return view.clip_from_view[3].w == 1.0;
}

fn ray_at(frag_coord: vec2f) -> Ray {
    let ndc = uv_to_ndc(frag_coord_to_uv(frag_coord));
    if is_orthographic() {
        let origin = position_ndc_to_world(vec3f(ndc, 1.0));
        return Ray(origin, normalize(direction_view_to_world(vec3f(0.0, 0.0, -1.0))), -FAR, TOP_LEVEL);
    }
    let direction = normalize(direction_view_to_world(position_ndc_to_view(vec3f(ndc, 1.0))));
    return Ray(view.world_position, direction, 0.0, TOP_LEVEL);
}

// Distance along `ray` to the point at NDC depth `depth` of the pixel at
// `frag_coord`; FAR where the depth is the far plane.
fn t_at_depth(ray: Ray, frag_coord: vec2f, depth: f32) -> f32 {
    if depth <= 0.0 {
        return FAR;
    }
    let ndc = uv_to_ndc(frag_coord_to_uv(frag_coord));
    return dot(position_ndc_to_world(vec3f(ndc, depth)) - ray.origin, ray.direction);
}

// NDC depth of `world_position` in the current view, shadow views included.
fn depth_at(world_position: vec3f) -> f32 {
    let clip = view.clip_from_world * vec4f(world_position, 1.0);
    return clamp(clip.z / clip.w, 0.0, 1.0);
}

const NORMAL_STEP_PER_METRE: f32 = grid::CELLS_PER_METER / 80.0;

fn normal_step(t: f32) -> f32 {
    return max(1.0, t * NORMAL_STEP_PER_METRE);
}

fn window_first_cell() -> vec2i {
    return grid::window_first_cell(params.window_origin);
}

fn ground_at(cell: vec2i) -> f32 {
    return textureLoad(ground_texture, grid::window_texel(cell, params.window_origin), 0).r;
}

fn water_column_at(cell: vec2i) -> water_column::Column {
    return water_column::column(
        water_texture,
        ground_texture,
        coarse_water_texture,
        coarse_ground_texture,
        cell,
        params.window_origin,
        params.centre,
    );
}

fn corners(cell: vec2i) -> vec4f {
    if params.surface == SURFACE_LAND {
        return vec4f(
            ground_at(cell),
            ground_at(cell + vec2i(1, 0)),
            ground_at(cell + vec2i(0, 1)),
            ground_at(cell + vec2i(1, 1)),
        );
    }
    let c00 = water_column_at(cell);
    let c10 = water_column_at(cell + vec2i(1, 0));
    let c01 = water_column_at(cell + vec2i(0, 1));
    let c11 = water_column_at(cell + vec2i(1, 1));
    if !(water_column::is_wet(c00) || water_column::is_wet(c10) || water_column::is_wet(c01) || water_column::is_wet(c11)) {
        return vec4f(NO_HEIGHT);
    }
    return vec4f(water_column::drawn(c00), water_column::drawn(c10), water_column::drawn(c01), water_column::drawn(c11));
}

fn node_max(node: vec2u, level: u32) -> f32 {
    if params.surface == SURFACE_LAND {
        return textureLoad(terrain_max, node, level - 1u).r;
    }
    if level >= 2u {
        return textureLoad(water_max, node, level - 2u).r;
    }
    let chunk = node * 2u / grid::CHUNK_DIM + params.window_origin;
    if all(chunk == params.centre) {
        return textureLoad(centre_water_max, node - (params.centre - params.window_origin) * (grid::CHUNK_DIM / 2u), 0).r;
    }
    return textureLoad(water_max, node / 2u, 0).r;
}

fn surface_height(cell: vec2f) -> f32 {
    if params.surface == SURFACE_LAND {
        return heightfield::sample(ground_texture, cell, params.window_origin);
    }
    let c = heightfield::corners(cell);
    return heightfield::mix_corners(vec4f(
        water_column::surface(water_column_at(c.origin)),
        water_column::surface(water_column_at(c.origin + vec2i(1, 0))),
        water_column::surface(water_column_at(c.origin + vec2i(0, 1))),
        water_column::surface(water_column_at(c.origin + vec2i(1, 1))),
    ), c.fraction);
}

fn surface_normal(xz: vec2f, step: f32) -> vec3f {
    let cell = grid::world_cell(xz);
    return heightfield::normal(
        surface_height(cell - vec2f(step, 0.0)),
        surface_height(cell + vec2f(step, 0.0)),
        surface_height(cell - vec2f(0.0, step)),
        surface_height(cell + vec2f(0.0, step)),
        step,
    );
}

// Where a ray met the surface. `face` is the unit normal of a voxel's face,
// and zero on the smooth surface, whose normal `hit_normal` works out.
struct Hit {
    found: bool,
    t: f32,
    position: vec3f,
    face: vec3f,
}

fn hit_normal(hit: Hit) -> vec3f {
    if any(hit.face != vec3f(0.0)) {
        return hit.face;
    }
    return surface_normal(hit.position.xz, normal_step(hit.t));
}

fn miss() -> Hit {
    return Hit(false, FAR, vec3f(0.0), vec3f(0.0, 1.0, 0.0));
}

fn patch_root(h: vec4f, uv0: vec2f, duv: vec2f, y0: f32, dy: f32, length: f32) -> f32 {
    let b = h.y - h.x;
    let c = h.z - h.x;
    let e = h.w - h.y - h.z + h.x;
    let h0 = h.x + b * uv0.x + c * uv0.y + e * uv0.x * uv0.y;
    let h1 = b * duv.x + c * duv.y + e * (uv0.x * duv.y + uv0.y * duv.x);
    let h2 = e * duv.x * duv.y;
    let qa = -h2;
    let qb = dy - h1;
    let qc = y0 - h0;
    if qc <= 0.0 {
        return 0.0;
    }
    if abs(qa) < 1e-9 {
        if qb >= 0.0 {
            return -1.0;
        }
        let t = -qc / qb;
        return select(-1.0, t, t <= length);
    }
    let discriminant = qb * qb - 4.0 * qa * qc;
    if discriminant < 0.0 {
        return -1.0;
    }
    let root = sqrt(discriminant);
    let t0 = (-qb - root) / (2.0 * qa);
    let t1 = (-qb + root) / (2.0 * qa);
    let first = min(t0, t1);
    let second = max(t0, t1);
    if first >= 0.0 && first <= length {
        return first;
    }
    if second >= 0.0 && second <= length {
        return second;
    }
    return -1.0;
}

// Where `ray` first meets the marched surface within the window, before
// `t_limit`. With `params.voxel_level` set, the surface is the boxes of that
// pyramid level instead, each as high as the highest point under it.
fn march(ray: Ray, t_limit: f32) -> Hit {
    let window = f32(grid::WINDOW_DIM);
    let first = window_first_cell();
    let oc = grid::world_cell(ray.origin.xz) - vec2f(first);
    let dc = ray.direction.xz * grid::CELLS_PER_METER;
    let moving = abs(dc) > vec2f(1e-12);
    let inv = select(vec2f(0.0), 1.0 / dc, moving);
    let towards = select(vec2f(-1.0), vec2f(1.0), dc > vec2f(0.0));
    let least_step = NUDGE / max(max(abs(dc.x), abs(dc.y)), 1e-6);

    let slab_a = select(vec2f(-FAR), (vec2f(0.0) - oc) * inv, moving);
    let slab_b = select(vec2f(FAR), (vec2f(window) - oc) * inv, moving);
    let inside = moving | ((oc >= vec2f(0.0)) & (oc < vec2f(window)));
    if !all(inside) {
        return miss();
    }
    let slab_low = min(slab_a, slab_b);
    let slab_high = max(slab_a, slab_b);
    var t = max(max(slab_low.x, slab_low.y), ray.t_min);
    let t_end = min(min(slab_high.x, slab_high.y), t_limit);
    if t >= t_end {
        return miss();
    }

    let voxels = params.voxel_level != SMOOTH;
    let stop = select(0u, min(params.voxel_level, TOP_LEVEL), voxels);
    var axis = select(1u, 0u, slab_low.x >= slab_low.y);
    var level = clamp(ray.level, stop, TOP_LEVEL);

    for (var i = 0u; i < MAX_ITERATIONS; i++) {
        if t >= t_end {
            break;
        }
        let size = f32(1u << level);
        let nodes = window / size;
        let c = oc + dc * t + towards * NUDGE;
        let node_f = clamp(floor(c / size), vec2f(0.0), vec2f(nodes - 1.0));
        let node = vec2u(node_f);
        let exit_border = node_f * size + select(vec2f(0.0), vec2f(size), dc > vec2f(0.0));
        let exits = select(vec2f(FAR), (exit_border - oc) * inv, moving);
        let t_exit = max(min(min(exits.x, exits.y), t_end), t + least_step);
        let exit_axis = select(1u, 0u, exits.x <= exits.y);
        let climb = min(level + 1u, TOP_LEVEL);
        let y = ray.origin.y + ray.direction.y * t;

        if level == stop && voxels {
            var top = NO_HEIGHT;
            if level == 0u {
                let h = corners(vec2i(node) + first);
                top = max(max(h.x, h.y), max(h.z, h.w));
            } else {
                top = node_max(node, level);
            }
            if y <= top {
                var normal = vec3f(0.0, 1.0, 0.0);
                if axis == 0u {
                    normal = vec3f(-towards.x, 0.0, 0.0);
                } else if axis == 1u {
                    normal = vec3f(0.0, 0.0, -towards.y);
                }
                return Hit(true, t, ray.origin + ray.direction * t, normal);
            }
            if ray.direction.y < 0.0 {
                let t_top = (top - ray.origin.y) / ray.direction.y;
                if t_top < t_exit {
                    return Hit(true, t_top, ray.origin + ray.direction * t_top, vec3f(0.0, 1.0, 0.0));
                }
            }
            t = t_exit;
            axis = exit_axis;
            level = climb;
            continue;
        }

        if level == 0u {
            let cell = vec2i(node) + first;
            let h = corners(cell);
            let uv0 = c - node_f;
            let root = patch_root(h, uv0, dc, y, ray.direction.y, t_exit - t);
            if root >= 0.0 {
                let t_hit = t + root;
                let position = ray.origin + ray.direction * t_hit;
                return Hit(true, t_hit, position, vec3f(0.0));
            }
            t = t_exit;
            axis = exit_axis;
            level = 1u;
            continue;
        }

        let top = node_max(node, level);
        if y > top {
            var t_down = FAR;
            if ray.direction.y < 0.0 {
                t_down = (top - ray.origin.y) / ray.direction.y;
            }
            if t_down >= t_exit {
                t = t_exit;
                axis = exit_axis;
                level = climb;
                continue;
            }
            t = t_down;
            axis = 2u;
        }
        level -= 1u;
    }
    return miss();
}
