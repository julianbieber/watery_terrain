pub mod clipmap;

use bevy::{asset::embedded_asset, pbr::ExtendedMaterial, prelude::*};

use crate::{
    render::clipmap::{TerrainMaterial, WaterTerrainMaterial, TiledTerrainMaterial, TiledWaterTerrainMaterial, follow},
    screens::Screen,
    water_sim::{TiledWaterTextures, WaterMarker},
};

pub struct TerrainRanderPlugin;

impl Plugin for TerrainRanderPlugin {
    fn build(&self, app: &mut App) {
        // Embed all shaders
        embedded_asset!(app, "terrain.wgsl");
        embedded_asset!(app, "water.wgsl");
        embedded_asset!(app, "tiled_terrain.wgsl");
        embedded_asset!(app, "tiled_water.wgsl");

        // Register material plugins for both old and new materials
        app.add_plugins(MaterialPlugin::<
            ExtendedMaterial<StandardMaterial, WaterTerrainMaterial>,
        >::default());
        app.add_plugins(MaterialPlugin::<
            ExtendedMaterial<StandardMaterial, TerrainMaterial>,
        >::default());
        app.add_plugins(MaterialPlugin::<
            ExtendedMaterial<StandardMaterial, TiledWaterTerrainMaterial>,
        >::default());
        app.add_plugins(MaterialPlugin::<
            ExtendedMaterial<StandardMaterial, TiledTerrainMaterial>,
        >::default());

        app.add_systems(Update, follow.run_if(in_state(Screen::Gameplay)));
        app.add_systems(Update, swap_textures.run_if(in_state(Screen::Gameplay)));
    }
}

/// Swap textures for old single-texture water material (kept for backwards compat)
#[allow(dead_code)]
fn swap_textures_old(
    textures: ResMut<crate::water_sim::WaterHeightTexture>,
    water: Single<
        &MeshMaterial3d<ExtendedMaterial<StandardMaterial, WaterTerrainMaterial>>,
        With<WaterMarker>,
    >,
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, WaterTerrainMaterial>>>,
) {
    let a = materials.get_mut(water.0.id()).unwrap();
    if a.extension.water == textures.texture_a {
        a.extension.water = textures.texture_b.clone();
    } else {
        a.extension.water = textures.texture_a.clone();
    }
}

/// Swap textures for tiled water material
fn swap_textures(
    textures: ResMut<TiledWaterTextures>,
    water: Single<
        &MeshMaterial3d<ExtendedMaterial<StandardMaterial, TiledWaterTerrainMaterial>>,
        With<WaterMarker>,
    >,
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, TiledWaterTerrainMaterial>>>,
) {
    let a = materials.get_mut(water.0.id()).unwrap();
    
    // Swap texture_a and texture_b for all tiles
    for (i, tile_textures) in textures.tiles.iter().enumerate() {
        let water_a = &tile_textures.texture_a;
        let water_b = &tile_textures.texture_b;
        
        match i {
            0 => { 
                if a.extension.water_00 == *water_a { a.extension.water_00 = water_b.clone(); }
                else { a.extension.water_00 = water_a.clone(); }
            }
            1 => { 
                if a.extension.water_01 == *water_a { a.extension.water_01 = water_b.clone(); }
                else { a.extension.water_01 = water_a.clone(); }
            }
            2 => { 
                if a.extension.water_02 == *water_a { a.extension.water_02 = water_b.clone(); }
                else { a.extension.water_02 = water_a.clone(); }
            }
            3 => { 
                if a.extension.water_10 == *water_a { a.extension.water_10 = water_b.clone(); }
                else { a.extension.water_10 = water_a.clone(); }
            }
            4 => { 
                if a.extension.water_11 == *water_a { a.extension.water_11 = water_b.clone(); }
                else { a.extension.water_11 = water_a.clone(); }
            }
            5 => { 
                if a.extension.water_12 == *water_a { a.extension.water_12 = water_b.clone(); }
                else { a.extension.water_12 = water_a.clone(); }
            }
            6 => { 
                if a.extension.water_20 == *water_a { a.extension.water_20 = water_b.clone(); }
                else { a.extension.water_20 = water_a.clone(); }
            }
            7 => { 
                if a.extension.water_21 == *water_a { a.extension.water_21 = water_b.clone(); }
                else { a.extension.water_21 = water_a.clone(); }
            }
            8 => { 
                if a.extension.water_22 == *water_a { a.extension.water_22 = water_b.clone(); }
                else { a.extension.water_22 = water_a.clone(); }
            }
            _ => {}
        }
    }
}
