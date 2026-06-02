use core::f32;

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::{
    asset::RenderAssetUsages,
    ecs::component::Component,
    image::Image,
    math::{Mat3, Vec2, Vec3, Vec3Swizzles},
    render::render_resource::{Extent3d, TextureUsages},
};

pub fn create_water_heightmap() -> Heightmap {
    let mut m = Heightmap::zero();

    for y in 0..Heightmap::DIM {
        for x in 0..Heightmap::DIM {
            let v = Vec2::new(x as f32 * 0.03, y as f32 * 0.03);
            let scope = mountain_noise(Vec3::new(v.y * 0.1, v.x * 0.1, 100.0));
            let h: f32 = mountain_noise(Vec3::new(v.x, v.y, 1.0)) * scope;
            m.set(x, y, h.abs());
        }
    }
    m
}

pub fn create_terrain_heightmap() -> Heightmap {
    let mut m = Heightmap::zero();

    for y in 0..Heightmap::DIM {
        for x in 0..Heightmap::DIM {
            let v = Vec2::new(x as f32 * 0.03, y as f32 * 0.03);
            let h: f32 = (mountain_noise(Vec3::new(v.x, v.y, 1.0) * 0.1)).abs() * 5.0
                + 0.01
                + value_noise(Vec2::new(v.x, v.y) * 0.1) * 1.2
                + voronoise(Vec2::new(v.x, v.y) * 0.1, 0.2, 0.2) * 1.0;
            m.set(x, y, h.abs() * 3.0);
        }
    }
    m
}

#[allow(dead_code)]
pub fn create_heightmap_spike() -> Heightmap {
    let mut m = Heightmap::zero();

    m.set(1024, 1024, 5.0);

    m
}

#[derive(Component, Clone)]
pub struct Heightmap {
    pub values: Vec<f32>,
}

impl Heightmap {
    pub const DIM: u32 = 128 * 16;
    fn zero() -> Heightmap {
        Heightmap {
            values: vec![0.0; (Self::DIM * Self::DIM) as usize],
        }
    }
    pub fn image(&self) -> Image {
        let data: Vec<u8> = self.values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let mut image = Image::new(
            Extent3d {
                width: Self::DIM,
                height: Self::DIM,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            data,
            bevy::render::render_resource::TextureFormat::R32Float,
            RenderAssetUsages::all(),
        );

        image.texture_descriptor.usage |= TextureUsages::STORAGE_BINDING | TextureUsages::COPY_DST;
        image
    }

    #[allow(dead_code)]
    pub fn get(&self, x: u32, y: u32) -> f32 {
        assert!(x < Self::DIM);
        assert!(y < Self::DIM);

        let index = x as usize * (Self::DIM as usize) + y as usize;

        assert!(index < (Self::DIM * Self::DIM) as usize);

        self.values[index]
    }

    pub fn set(&mut self, x: u32, y: u32, h: f32) {
        assert!(x < Self::DIM);
        assert!(y < Self::DIM);

        let index = x as usize * (Self::DIM as usize) + y as usize;

        assert!(index < (Self::DIM * Self::DIM) as usize);

        self.values[index] = h;
    }

    pub fn avian(&self) -> Collider {
        let mut h = Vec::with_capacity(Self::DIM as usize);
        for z in 0..Self::DIM {
            let mut row = Vec::with_capacity(Self::DIM as usize);
            for x in 0..Self::DIM {
                let height = self.get(x, z);
                row.push(height);
            }
            h.push(row);
        }
        Collider::heightfield(h, Vec3::new(2048.0 / 10.0, 1.0, 2048.0 / 10.0))
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

#[allow(dead_code)]
pub fn hash22(i: Vec2) -> Vec2 {
    let mut h = i * Vec2::new(127.1, 311.7);
    h = (h.fract() * 43_758.547).fract();
    h
}

pub fn hash21(i: Vec2) -> f32 {
    let mut h = i.dot(Vec2::new(127.1, 311.7));
    h = (h.fract() * 43_758.547).fract();
    h
}

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

// ============================================
// Tiled Heightmap Support
// ============================================

/// Tile constants for the 3x3 grid system
pub const TILE_DIM: u32 = 1024; // Texture pixels per tile
pub const TILE_SIZE_WORLD: f32 = 102.4; // World units per tile (1024 pixels * 0.1 world/pixel)

/// Grid position in 3x3 layout where grid_x, grid_z ∈ {-1, 0, 1}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TilePosition {
    pub grid_x: i8,
    pub grid_z: i8,
}

impl TilePosition {
    /// All 9 positions in 3x3 grid (row-major: top-left to bottom-right)
    pub const ALL: [TilePosition; 9] = [
        TilePosition { grid_x: -1, grid_z: -1 }, // top-left
        TilePosition { grid_x: 0, grid_z: -1 },  // top-center
        TilePosition { grid_x: 1, grid_z: -1 },  // top-right
        TilePosition { grid_x: -1, grid_z: 0 },  // middle-left
        TilePosition { grid_x: 0, grid_z: 0 },   // center
        TilePosition { grid_x: 1, grid_z: 0 },   // middle-right
        TilePosition { grid_x: -1, grid_z: 1 },  // bottom-left
        TilePosition { grid_x: 0, grid_z: 1 },   // bottom-center
        TilePosition { grid_x: 1, grid_z: 1 },   // bottom-right
    ];

    /// Get grid position from world coordinates
    pub fn from_world(world_pos: Vec2) -> Self {
        let gx = ((world_pos.x + 51.2) / TILE_SIZE_WORLD).floor() as i8;
        let gz = ((world_pos.y + 51.2) / TILE_SIZE_WORLD).floor() as i8;
        // Clamp to valid range
        let gx = gx.clamp(-1, 1);
        let gz = gz.clamp(-1, 1);
        TilePosition { grid_x: gx, grid_z: gz }
    }

    /// Convert grid position to tile index (0-8) using row-major ordering
    pub fn to_index(self) -> usize {
        ((self.grid_z + 1) as usize * 3) + (self.grid_x + 1) as usize
    }
}

/// Mapping of tile indices to grid positions
/// Allows flexible assignment: tile index 0-8 can be placed at any grid position
#[derive(Debug, Clone)]
pub struct TileLayout {
    /// For each grid position, which tile index is placed there
    pub assignment: HashMap<TilePosition, usize>,
}

impl Default for TileLayout {
    fn default() -> Self {
        // Default: row-major ordering (tile 0 at top-left, tile 8 at bottom-right)
        let mut assignment = HashMap::new();
        for (i, &pos) in TilePosition::ALL.iter().enumerate() {
            assignment.insert(pos, i);
        }
        Self { assignment }
    }
}

impl TileLayout {
    /// Create a new layout with explicit assignment mapping
    pub fn new_flexible(assignment: HashMap<TilePosition, usize>) -> Self {
        Self { assignment }
    }

    /// Get the tile index at a specific grid position
    pub fn get_tile_index(&self, pos: TilePosition) -> Option<usize> {
        self.assignment.get(&pos).copied()
    }

    /// Get the tile index for a world position
    pub fn get_tile_index_at_world(&self, world_pos: Vec2) -> Option<usize> {
        let pos = TilePosition::from_world(world_pos);
        self.get_tile_index(pos)
    }

    /// Get world bounds (min, max) for a grid position
    pub fn get_world_bounds(&self, pos: TilePosition) -> (Vec2, Vec2) {
        let x_min = pos.grid_x as f32 * TILE_SIZE_WORLD - 51.2;
        let x_max = pos.grid_x as f32 * TILE_SIZE_WORLD + 51.2;
        let z_min = pos.grid_z as f32 * TILE_SIZE_WORLD - 51.2;
        let z_max = pos.grid_z as f32 * TILE_SIZE_WORLD + 51.2;
        (Vec2::new(x_min, z_min), Vec2::new(x_max, z_max))
    }

    /// Get the grid position for a tile index (inverse lookup)
    pub fn get_grid_position(&self, tile_index: usize) -> Option<TilePosition> {
        self.assignment
            .iter()
            .find(|(_, idx)| **idx == tile_index)
            .map(|(pos, _)| *pos)
    }
}

/// A heightmap composed of 9 tiles in a 3x3 grid
#[derive(Clone)]
pub struct TiledHeightmap {
    pub tiles: Vec<Heightmap>,
    pub tile_dim: u32,
    pub layout: TileLayout,
}

impl TiledHeightmap {
    /// Create a new tiled heightmap with explicit layout
    pub fn new_with_layout(tiles: Vec<Heightmap>, layout: TileLayout) -> Self {
        assert_eq!(tiles.len(), 9, "Expected exactly 9 tiles");
        Self {
            tiles,
            tile_dim: TILE_DIM,
            layout,
        }
    }

    /// Create a new tiled heightmap with default row-major layout
    pub fn new_row_major(tiles: Vec<Heightmap>) -> Self {
        Self::new_with_layout(tiles, TileLayout::default())
    }

    /// Create a tiled heightmap from a single heightmap by splitting it
    /// Note: Current single heightmap is 2048x2048, but tiles are 1024x1024
    /// This will extract 9 regions from the source, expanding to 3072x3072 total
    pub fn from_single(heightmap: &Heightmap, layout: TileLayout) -> Self {
        let mut tiles = Vec::with_capacity(9);

        // For each tile position, extract the appropriate region
        for pos in TilePosition::ALL.iter() {
            let mut tile = Heightmap::zero();
            // Map tile coordinates to source heightmap coordinates
            // Center tile (0,0) uses center 1024x1024 of the 2048x2048 source
            // Other tiles need to wrap or extend - for now just copy center
            let src_offset_x = match pos.grid_x {
                -1 => 0,
                0 => 512,
                1 => 1024,
                _ => 0,
            };
            let src_offset_y = match pos.grid_z {
                -1 => 0,
                0 => 512,
                1 => 1024,
                _ => 0,
            };

            for ty in 0..TILE_DIM {
                for tx in 0..TILE_DIM {
                    let sx = (src_offset_x + tx) % Heightmap::DIM;
                    let sy = (src_offset_y + ty) % Heightmap::DIM;
                    tile.set(tx, ty, heightmap.get(sx, sy));
                }
            }
            tiles.push(tile);
        }

        Self::new_with_layout(tiles, layout)
    }

    /// Get height at a world position, selecting the appropriate tile
    pub fn get_height_at_world(&self, world_pos: Vec2) -> f32 {
        if let Some(tile_idx) = self.layout.get_tile_index_at_world(world_pos) {
            let pos = TilePosition::from_world(world_pos);
            let (min, _) = self.layout.get_world_bounds(pos);

            // Calculate local position within tile in texture pixels
            let local_x = (world_pos.x - min.x) * 10.0;
            let local_y = (world_pos.y - min.y) * 10.0;

            // Clamp to tile bounds and convert to integers
            let tx = local_x.clamp(0.0, TILE_DIM as f32 - 1.0) as u32;
            let ty = local_y.clamp(0.0, TILE_DIM as f32 - 1.0) as u32;

            self.tiles[tile_idx].get(tx, ty)
        } else {
            0.0
        }
    }

    /// Create a tiled heightmap with all tiles set to zero
    pub fn zero_with_layout(layout: TileLayout) -> Self {
        let mut tiles = Vec::with_capacity(9);
        for _ in 0..9 {
            tiles.push(Heightmap::zero());
        }
        Self::new_with_layout(tiles, layout)
    }

    /// Create a tiled heightmap with all tiles set to zero using default layout
    pub fn zero() -> Self {
        Self::zero_with_layout(TileLayout::default())
    }

    /// Get the heightmap for a specific tile index
    pub fn get_tile(&self, index: usize) -> Option<&Heightmap> {
        self.tiles.get(index)
    }

    /// Get the heightmap for a grid position
    pub fn get_tile_at_position(&self, pos: TilePosition) -> Option<&Heightmap> {
        self.layout.get_tile_index(pos).and_then(|idx| self.tiles.get(idx))
    }
}
