//! The world's cell and chunk grid shared by the heightmaps, the water
//! simulation, the terrain colliders and the shaders, and the one place its
//! dimensions are defined.

use bevy::{
    prelude::*,
    shader::{ShaderDefVal, ShaderSettings, load_shader_library},
};

/// Cells along each side of one chunk.
pub const CHUNK_DIM: u32 = 1024;

/// Chunks along each side of the world.
pub const WORLD_CHUNKS: u32 = 6;

/// Chunks along each side of the active window that is drawn, collides and is
/// simulated.
pub const WINDOW_CHUNKS: u32 = 3;

/// Cells along each side of the world.
pub const WORLD_DIM: u32 = WORLD_CHUNKS * CHUNK_DIM;

/// Cells, and texels of every window texture, along each side of the window.
pub const WINDOW_DIM: u32 = WINDOW_CHUNKS * CHUNK_DIM;

/// Cells per world metre along each axis. The shaders read it as
/// `constants::CELLS_PER_METER` through `package::grid`.
pub const CELLS_PER_METER: u32 = 10;

/// Side length of one cell in metres.
pub const CELL_SIZE: f32 = 1.0 / CELLS_PER_METER as f32;

/// A chunk of the world grid; `x` runs along world x and `z` along world z.
/// Chunk (0, 0) holds the world's lower corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkCoord {
    pub x: i32,
    pub z: i32,
}

impl ChunkCoord {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}

/// World position (xz) of the lower corner of world cell `cell`, where the
/// heightmap sample of that cell sits. World cell (x, z) is texel (u = x,
/// v = z) and `Heightmap::get(z, x)`.
pub fn corner_of(cell: UVec2) -> Vec2 {
    (cell.as_vec2() - WORLD_DIM as f32 / 2.0) * CELL_SIZE
}

/// The chunk whose cells contain `world_xz`; it lies outside the world when
/// the position does.
pub fn chunk_of(world_xz: Vec2) -> ChunkCoord {
    let cell = (world_xz / CELL_SIZE + WORLD_DIM as f32 / 2.0).floor();
    let chunk = (cell / CHUNK_DIM as f32).floor();
    ChunkCoord::new(chunk.x as i32, chunk.y as i32)
}

/// World cell of the lower corner of `chunk`.
///
/// Panics if `chunk` lies outside the world.
pub fn chunk_origin(chunk: ChunkCoord) -> UVec2 {
    assert!(
        (0..WORLD_CHUNKS as i32).contains(&chunk.x) && (0..WORLD_CHUNKS as i32).contains(&chunk.z),
        "chunk {chunk:?} lies outside the world"
    );
    UVec2::new(chunk.x as u32, chunk.z as u32) * CHUNK_DIM
}

/// Texel of the window textures that holds world cell `cell` while its chunk
/// is in the window: the cell modulo `WINDOW_DIM` on each axis. The CPU twin
/// of `grid::window_texel` in the shaders.
pub fn texel_of(cell: UVec2) -> UVec2 {
    cell % WINDOW_DIM
}

/// Loads `package::grid` with the grid constants the shaders need.
pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "grid.wesl", |settings: &mut ShaderSettings| {
            settings.shader_defs = vec![
                ShaderDefVal::UInt("CELLS_PER_METER".into(), CELLS_PER_METER),
                ShaderDefVal::UInt("CHUNK_DIM".into(), CHUNK_DIM),
                ShaderDefVal::UInt("WORLD_CHUNKS".into(), WORLD_CHUNKS),
                ShaderDefVal::UInt("WINDOW_CHUNKS".into(), WINDOW_CHUNKS),
            ];
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The world is centred on the origin, so its middle cell sits at world (0, 0).
    #[test]
    fn corner_of_centres_the_world_on_the_origin() {
        assert_eq!(corner_of(UVec2::splat(WORLD_DIM / 2)), Vec2::ZERO);
        assert_eq!(
            corner_of(UVec2::ZERO),
            Vec2::splat(-(WORLD_DIM as f32) / 2.0 * CELL_SIZE)
        );
    }

    // A position on a chunk seam belongs to the chunk above it, and positions outside the world map to chunks outside it.
    #[test]
    fn chunk_of_splits_on_seams_and_reaches_outside() {
        let seam = corner_of(UVec2::new(3 * CHUNK_DIM, 2 * CHUNK_DIM));
        assert_eq!(chunk_of(seam), ChunkCoord::new(3, 2));
        assert_eq!(
            chunk_of(seam - Vec2::splat(CELL_SIZE / 2.0)),
            ChunkCoord::new(2, 1)
        );
        assert_eq!(
            chunk_of(corner_of(UVec2::ZERO) - Vec2::ONE),
            ChunkCoord::new(-1, -1)
        );
    }

    // chunk_of and chunk_origin are inverses for every chunk of the world.
    #[test]
    fn chunk_origin_inverts_chunk_of() {
        for x in 0..WORLD_CHUNKS as i32 {
            for z in 0..WORLD_CHUNKS as i32 {
                let chunk = ChunkCoord::new(x, z);
                assert_eq!(chunk_of(corner_of(chunk_origin(chunk))), chunk);
            }
        }
    }

    // A chunk's slot is its coordinate mod WINDOW_CHUNKS, and its cells keep their offset inside the slot.
    #[test]
    fn texel_of_places_each_chunk_in_its_slot() {
        let origin = chunk_origin(ChunkCoord::new(4, 5));
        assert_eq!(texel_of(origin), UVec2::new(1, 2) * CHUNK_DIM);
        assert_eq!(
            texel_of(origin + UVec2::new(7, 9)),
            UVec2::new(CHUNK_DIM + 7, 2 * CHUNK_DIM + 9)
        );
    }
}
