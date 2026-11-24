//! What a renderer crate draws the terrain and water from: the window's
//! height textures and layout, the shader libraries that define their
//! surfaces, and the scope and ordering its entities and systems join.
//!
//! The shader libraries are imported as `watery_terrain::grid`,
//! `watery_terrain::render::heightfield` and
//! `watery_terrain::render::water_column`.

use bevy::{
    prelude::*,
    shader::{ShaderDefVal, ShaderSettings, load_shader_library},
};

pub use crate::{
    TerrainScoped,
    chunks::{ChunkLayout, WindowHeights, shift_window},
    water_sim::{DRY_DEPTH_MM, WaterStep},
};

pub(crate) struct SurfaceShaderPlugin;

impl Plugin for SurfaceShaderPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "heightfield.wesl");
        load_shader_library!(app, "water_column.wesl", |settings: &mut ShaderSettings| {
            settings.shader_defs = vec![ShaderDefVal::UInt("DRY_DEPTH_MM".into(), DRY_DEPTH_MM)];
        });
    }
}
