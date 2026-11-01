//! Heightmaps: generating a chunk's terrain and water heights, and turning
//! heights into textures and colliders.

use core::f32;

use avian3d::prelude::*;
use bevy::{
    asset::RenderAssetUsages,
    ecs::component::Component,
    image::Image,
    math::{Mat3, UVec2, Vec2, Vec3, Vec3Swizzles},
    render::render_resource::{Extent3d, TextureUsages},
    transform::components::Transform,
};

use crate::grid::{CELL_SIZE, CHUNK_DIM, ChunkCoord, chunk_origin, corner_of};

/// A chunk's water heightmap with every cell at the starting depth.
pub fn create_water_heightmap() -> Heightmap {
    Heightmap {
        values: vec![0.2; (CHUNK_DIM * CHUNK_DIM) as usize],
        dim: CHUNK_DIM,
    }
}

/// The terrain heightmap of `chunk`, from noise sampled by world cell, so the
/// chunks together equal one generation of the whole world.
///
/// Panics if `chunk` lies outside the world.
pub fn create_terrain_heightmap(chunk: ChunkCoord) -> Heightmap {
    let mut m = Heightmap::zero(CHUNK_DIM);
    let origin = chunk_origin(chunk);

    for z in 0..CHUNK_DIM {
        for x in 0..CHUNK_DIM {
            let world = origin + UVec2::new(x, z);
            let v = Vec2::new(world.y as f32 * 0.03, world.x as f32 * 0.03);
            let h: f32 = (mountain_noise(Vec3::new(v.x, v.y, 1.0) * 0.1)).abs() * 5.0
                + 0.01
                + value_noise(Vec2::new(v.x, v.y) * 0.1) * 1.2
                + voronoise(Vec2::new(v.x, v.y) * 0.1, 0.2, 0.2) * 1.0;
            m.set(z, x, h.abs() * 3.0);
        }
    }
    m
}

/// A square grid of heights; `get(a, b)` is the value at row `a`, column `b`,
/// which `image` uploads as texel (u = b, v = a).
#[derive(Component)]
pub struct Heightmap {
    pub values: Vec<f32>,
    pub dim: u32,
}

impl Heightmap {
    /// A `dim` x `dim` heightmap of zeros.
    pub fn zero(dim: u32) -> Heightmap {
        Heightmap {
            values: vec![0.0; (dim * dim) as usize],
            dim,
        }
    }

    /// A `dim` x `dim` R32Float image of the values, usable as a storage
    /// texture, copy source and copy destination.
    pub fn image(&self) -> Image {
        let data: Vec<u8> = self.values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let mut image = Image::new(
            Extent3d {
                width: self.dim,
                height: self.dim,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            data,
            bevy::render::render_resource::TextureFormat::R32Float,
            RenderAssetUsages::all(),
        );

        image.texture_descriptor.usage |=
            TextureUsages::STORAGE_BINDING | TextureUsages::COPY_SRC | TextureUsages::COPY_DST;
        image
    }

    /// The value at row `x`, column `y`. Panics off the grid.
    pub fn get(&self, x: u32, y: u32) -> f32 {
        self.values[self.index(x, y)]
    }

    /// Writes the value at row `x`, column `y`. Panics off the grid.
    pub fn set(&mut self, x: u32, y: u32, h: f32) {
        let index = self.index(x, y);
        self.values[index] = h;
    }

    fn index(&self, x: u32, y: u32) -> usize {
        assert!(x < self.dim);
        assert!(y < self.dim);
        x as usize * self.dim as usize + y as usize
    }

    /// The values, row after row, for a chunk to share without copying.
    pub fn into_values(self) -> Vec<f32> {
        self.values
    }

    /// A static heightfield collider of `dim` x `dim` samples spanning
    /// `dim - 1` cells, and the transform that puts sample (0, 0) on the
    /// world corner of `first_cell` and every other sample where the shaders
    /// draw the same world cell.
    pub fn avian(&self, first_cell: UVec2) -> (Collider, Transform) {
        let mut h = Vec::with_capacity(self.dim as usize);
        for z in 0..self.dim {
            let mut row = Vec::with_capacity(self.dim as usize);
            for x in 0..self.dim {
                let height = self.get(x, z);
                row.push(height);
            }
            h.push(row);
        }
        let span = (self.dim - 1) as f32 * CELL_SIZE;
        let first = corner_of(first_cell);
        let center = first + Vec2::splat(span / 2.0);
        (
            Collider::heightfield(h, Vec3::new(span, 1.0, span)),
            Transform::from_xyz(center.x, 0.0, center.y),
        )
    }
}

#[allow(dead_code)]
fn hash3(p: Vec2) -> Vec3 {
    let p3 = (Vec3::new(p.x, p.x, p.y) * 0.1031).fract();
    let p3 = p3 + p3.dot(Vec3::new(p3.y, p3.z, p3.x) + 33.33);
    ((p3.xxy() + p3.yzz()) * p3.zyx() * Vec3::splat(f32::consts::FRAC_1_PI)).fract()
}

#[allow(dead_code)]
fn voronoise(x: Vec2, u: f32, v: f32) -> f32 {
    let p = x.floor();
    let f = x.fract();

    let k = 1.0 + 63.0 * (1.0 - v).powf(4.0);
    let mut va = 0.0;
    let mut wt = 0.0;
    for j in -2..=2 {
        for i in -2..=2 {
            let g = Vec2::new(i as f32, j as f32);
            let o = hash3(p + g) * Vec3::new(u, u, 1.0);
            let r = g - f + o.xy();
            let d = r.dot(r);
            let w = (1.0 - smoothstep(0.0, 1.414, d.sqrt())).powf(k);
            va += w * o.z;
            wt += w;
        }
    }
    va / wt
}

#[allow(dead_code)]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = (x - edge0) / (edge1 - edge0);
    let clamped = t.clamp(0.0, 1.0);
    clamped * clamped * (3.0 - 2.0 * clamped)
}

/// A pseudo-random value in [0, 1) per component for lattice point `i`.
#[allow(dead_code)]
pub fn hash22(i: Vec2) -> Vec2 {
    let mut h = i * Vec2::new(127.1, 311.7);
    h = (h.fract() * 43_758.547).fract();
    h
}

/// A pseudo-random value in [0, 1) for lattice point `i`.
pub fn hash21(i: Vec2) -> f32 {
    let mut h = i.dot(Vec2::new(127.1, 311.7));
    h = (h.fract() * 43_758.547).fract();
    h
}

/// Gradient noise at `x`, interpolated with a quintic curve between lattice
/// points.
#[allow(dead_code)]
pub fn gradient_noise(x: Vec2) -> f32 {
    let i = x.floor();
    let f = x.fract();
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);

    let ga = hash22(i + Vec2::ZERO);
    let gb = hash22(i + Vec2::new(1.0, 0.0));
    let gc = hash22(i + Vec2::new(0.0, 1.0));
    let gd = hash22(i + Vec2::new(1.0, 1.0));

    let va = ga.dot(f - Vec2::ZERO);
    let vb = gb.dot(f - Vec2::new(1.0, 0.0));
    let vc = gc.dot(f - Vec2::new(0.0, 1.0));
    let vd = gd.dot(f - Vec2::new(1.0, 1.0));

    va + u.x * (vb - va) + u.y * (vc - va) + u.x * u.y * (va - vb - vc + vd)
}

/// Value noise at `x` in [-1, 1), interpolated with a quintic curve between
/// lattice points.
#[allow(dead_code)]
pub fn value_noise(x: Vec2) -> f32 {
    let p = x.floor();
    let w = x.fract();
    let u = w * w * w * (w * (w * 6.0 - 15.0) + 10.0);

    let a = hash21(p + Vec2::ZERO);
    let b = hash21(p + Vec2::X);
    let c = hash21(p + Vec2::new(0.0, 1.0));
    let d = hash21(p + Vec2::new(1.0, 1.0));

    let k0 = a;
    let k1 = b - a;
    let k2 = c - a;
    let k4 = a - b - c + d;

    -1.0 + 2.0 * (k0 + k1 * u.x + k2 * u.y + k4 * u.x * u.y)
}

#[allow(dead_code)]
fn rot(x: f32, y: f32, z: f32) -> Mat3 {
    Mat3::from_euler(bevy::math::EulerRot::XYZ, x, y, z)
}

#[allow(dead_code)]
fn gyroid(x: Vec3) -> f32 {
    let c = Vec3::new(x.x.cos(), x.y.cos(), x.z.cos());
    let s = Vec3::new(x.y.sin(), x.z.sin(), x.x.sin());
    c.dot(s)
}

#[allow(dead_code)]
fn dotnoise(mut x: Vec3) -> f32 {
    let mut a = 0.0;
    for _ in 0..4 {
        x = rot(0.1, 0.2, 0.3) * x;
        let v = gyroid(x);

        a += v * 0.25;
    }
    a
}

#[allow(dead_code)]
fn mountain_noise(x: Vec3) -> f32 {
    let mut a = 0.0;
    let mut f = 1.0;
    let mut amp = 1.5;
    for _ in 0..5 {
        a += dotnoise(x * f).abs() * amp;
        f *= 2.5;
        amp *= 0.5;
    }

    (1.0 - a).tanh()
}
