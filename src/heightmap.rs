//! Heightmaps: square grids of heights, and turning them into textures and
//! colliders.

use avian3d::prelude::*;
use bevy::{
    asset::RenderAssetUsages,
    ecs::component::Component,
    image::Image,
    math::{UVec2, Vec2, Vec3},
    render::render_resource::{Extent3d, TextureUsages},
    transform::components::Transform,
};

use crate::grid::{CELL_SIZE, corner_of};

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
