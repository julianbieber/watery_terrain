// A rowing boat with a mast and sail, 3 m long with its bow towards -z.
// Materials: 0 hull, 1 bench and floor, 2 mast, 3 sail.
import sdf_mesh::sdf_lib::{
    Surface,
    surface,
    sd_box,
    sd_cylinder,
    sd_plane,
    op_union,
    op_intersect,
    op_subtract,
    op_smooth_intersect,
    op_translate,
    op_mirror_x,
};

const HALF_LENGTH: f32 = 1.5;
const HALF_BEAM: f32 = 0.55;
const GUNWALE: f32 = 0.3;
const WALL: f32 = 0.09;

fn sd_ellipsoid(p: vec3<f32>, radii: vec3<f32>) -> f32 {
    let k0 = length(p / radii);
    let k1 = length(p / (radii * radii));
    return k0 * (k0 - 1.0) / k1;
}

fn sd_planform(p: vec3<f32>, inset: f32) -> f32 {
    let offset = (HALF_LENGTH * HALF_LENGTH - HALF_BEAM * HALF_BEAM) / (2.0 * HALF_BEAM);
    let radius = offset + HALF_BEAM;
    let side = op_mirror_x(p);
    return length(side.xz + vec2(offset, 0.0)) - radius + inset;
}

fn hull_volume(p: vec3<f32>, inset: f32) -> f32 {
    let belly = sd_ellipsoid(op_translate(p, vec3(0.0, GUNWALE, 0.0)), vec3(0.66, 0.6, 1.75)) + inset;
    return max(sd_planform(p, inset), belly);
}

fn sdf_scene(p: vec3<f32>, time: f32) -> Surface {
    let outer = surface(hull_volume(p, 0.0), 0u);
    let below_gunwale = surface(sd_plane(p, vec3(0.0, 1.0, 0.0), -GUNWALE), 0u);
    var hull = op_smooth_intersect(outer, below_gunwale, 0.03);
    let hollow = surface(max(hull_volume(p, WALL), -(p.y + 0.05)), 0u);
    hull = op_subtract(hull, hollow);

    let floor = surface(sd_box(op_translate(p, vec3(0.0, -0.05, 0.0)), vec3(0.4, 0.04, 1.0)), 1u);
    let bench = surface(sd_box(op_translate(p, vec3(0.0, 0.15, 0.35)), vec3(0.6, 0.04, 0.14)), 1u);
    let fittings = op_intersect(op_union(floor, bench), surface(hull_volume(p, 0.02), 1u));
    var boat = op_union(hull, fittings);

    let mast = surface(sd_cylinder(op_translate(p, vec3(0.0, 0.7, -0.45)), 0.75, 0.06), 2u);
    boat = op_union(boat, mast);

    let sheet = sd_box(op_translate(p, vec3(0.0, 0.85, -0.05)), vec3(0.04, 0.55, 0.4));
    let hypotenuse = sd_plane(op_translate(p, vec3(0.0, 1.4, -0.45)), vec3(0.0, 0.8, 1.1), 0.0);
    let sail = surface(max(sheet, hypotenuse), 3u);
    return op_union(boat, sail);
}
