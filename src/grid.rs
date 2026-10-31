//! The cell grid shared by the heightmaps, the water simulation, the terrain
//! collider and the shaders, and the one place its dimensions are defined.

use bevy::{
    prelude::*,
    shader::{ShaderDefVal, ShaderSettings, load_shader_library},
};

/// Cells along each side of the grid.
pub const DIM: u32 = 2048;

/// Cells per world metre along each axis. The shaders read it as
/// `constants::CELLS_PER_METER` through `package::grid`.
pub const CELLS_PER_METER: u32 = 10;

/// Side length of one cell in metres.
pub const CELL_SIZE: f32 = 1.0 / CELLS_PER_METER as f32;

/// The cell whose lower corner lies at `world` (xz), or `None` when `world`
/// is outside the grid. The grid is centred on the world origin.
pub fn cell_at(world: Vec2) -> Option<UVec2> {
    let cell = (world * CELLS_PER_METER as f32 + DIM as f32 / 2.0).floor();
    let inside = cell.cmpge(Vec2::ZERO).all() && cell.cmplt(Vec2::splat(DIM as f32)).all();
    inside.then(|| cell.as_uvec2())
}

/// World position (xz) of the lower corner of `cell`, where the heightmap
/// sample of that cell sits.
pub fn corner_of(cell: UVec2) -> Vec2 {
    (cell.as_vec2() - DIM as f32 / 2.0) * CELL_SIZE
}

/// Loads `package::grid` with the grid constants the shaders need.
pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "grid.wesl", |settings: &mut ShaderSettings| {
            settings.shader_defs = vec![ShaderDefVal::UInt(
                "CELLS_PER_METER".into(),
                CELLS_PER_METER,
            )];
        });
    }
}
