// Noise that repeats exactly once per unit of UV along both axes, so maps built
// only from it wrap seamlessly when the terrain tiles them.
//
// Every `scale` is a cell count per unit of UV, rounded to the nearest integer
// and at least 1 on each axis: a lattice with a fractional cell count cannot
// wrap.

import bevy_pbr_generator::noise;

fn cells(scale: vec2<f32>) -> vec2<i32> {
    return max(vec2<i32>(round(scale)), vec2<i32>(1));
}

fn wrap(i: vec2<i32>, period: vec2<i32>) -> vec2<i32> {
    return ((i % period) + period) % period;
}

// Maps a value in [-1, 1] to [0, 1], clamping whatever overshoots.
fn unit(n: f32) -> f32 {
    return clamp(n * 0.5 + 0.5, 0.0, 1.0);
}

// Gradient noise with `scale` cells per unit of UV, roughly in [-1, 1] and zero
// at every lattice point.
fn perlin(uv: vec2<f32>, scale: vec2<f32>) -> f32 {
    let period = cells(scale);
    let scaled = uv * vec2<f32>(period);
    let cell = floor(scaled);
    let f = scaled - cell;
    let c0 = wrap(vec2<i32>(cell), period);
    let c1 = wrap(c0 + vec2<i32>(1), period);

    let a = noise::perm_lattice(c0.x, c0.y);
    let b = noise::perm_lattice(c1.x, c0.y);
    let c = noise::perm_lattice(c0.x, c1.y);
    let d = noise::perm_lattice(c1.x, c1.y);

    let u = noise::fade(f.x);
    let v = noise::fade(f.y);
    let ix0 = mix(noise::perlin_grad(f.x, f.y, a), noise::perlin_grad(f.x - 1.0, f.y, b), u);
    let ix1 = mix(noise::perlin_grad(f.x, f.y - 1.0, c), noise::perlin_grad(f.x - 1.0, f.y - 1.0, d), u);
    return mix(ix0, ix1, v);
}

// `octaves` layers of `perlin` starting at `scale`, each twice as fine and
// `persistence` times as strong as the last, divided by the summed amplitude
// so the result stays roughly in [-1, 1] whatever the octave count.
fn fbm(uv: vec2<f32>, scale: vec2<f32>, octaves: u32, persistence: f32) -> f32 {
    var value = 0.0;
    var total = 0.0;
    var amplitude = 1.0;
    var frequency = vec2<f32>(cells(scale));

    for (var i = 0u; i < max(octaves, 1u); i++) {
        value += amplitude * perlin(uv, frequency);
        total += amplitude;
        frequency *= 2.0;
        amplitude *= persistence;
    }

    return value / total;
}

// `fbm` folded at zero and inverted, so its sharp ridges sit where `fbm`
// crosses zero. Range [0, 1], with 1 on the ridges.
fn ridged(uv: vec2<f32>, scale: vec2<f32>, octaves: u32, persistence: f32) -> f32 {
    return 1.0 - clamp(abs(fbm(uv, scale, octaves, persistence)), 0.0, 1.0);
}

// Distances from `uv` to the nearest (x) and second-nearest (y) feature point,
// one feature point per cell of a lattice with `scale` cells per unit of UV.
// Measured in cells, so `y - x` is near zero along the borders between cells.
fn worley(uv: vec2<f32>, scale: vec2<f32>) -> vec2<f32> {
    let period = cells(scale);
    let scaled = uv * vec2<f32>(period);
    let base = floor(scaled);

    var nearest = 8.0;
    var second = 8.0;

    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let cell = base + vec2<f32>(f32(dx), f32(dy));
            let wrapped = vec2<f32>(wrap(vec2<i32>(cell), period));
            let feature = cell + noise::cell_offset(wrapped);
            let d = distance(scaled, feature);
            if (d < nearest) {
                second = nearest;
                nearest = d;
            } else if (d < second) {
                second = d;
            }
        }
    }

    return vec2<f32>(nearest, second);
}

// Three uncorrelated values in [0, 1], constant across each cell of a lattice
// with `scale` cells per unit of UV. Its derivative is zero or infinite, so it
// must not feed a height that normals are differentiated from.
fn speckle(uv: vec2<f32>, scale: vec2<f32>) -> vec3<f32> {
    let period = cells(scale);
    let c = wrap(vec2<i32>(floor(uv * vec2<f32>(period))), period);
    let h0 = noise::lattice_hash(c.x, c.y);
    let h1 = noise::integer_hash(h0 ^ bitcast<i32>(0x27d4eb2du));
    let h2 = noise::integer_hash(h1 ^ bitcast<i32>(0x165667b1u));
    return vec3<f32>(noise::unit_from_hash(h0), noise::unit_from_hash(h1), noise::unit_from_hash(h2));
}
