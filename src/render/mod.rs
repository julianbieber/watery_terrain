pub mod clipmap;

use bevy::{asset::embedded_asset, pbr::ExtendedMaterial, prelude::*, shader::load_shader_library};

use crate::{
    render::clipmap::{TerrainMaterial, WaterTerrainMaterial, follow},
    screens::Screen,
    water_sim::{WaterHeightTexture, WaterMarker},
};

pub struct TerrainRanderPlugin;

impl Plugin for TerrainRanderPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "terrain.wesl");
        load_shader_library!(app, "water.wesl");
        embedded_asset!(app, "terrain_forward.wesl");
        embedded_asset!(app, "terrain_prepass.wesl");
        embedded_asset!(app, "water_forward.wesl");
        embedded_asset!(app, "water_prepass.wesl");

        app.add_plugins(MaterialPlugin::<
            ExtendedMaterial<StandardMaterial, WaterTerrainMaterial>,
        >::default());
        app.add_plugins(MaterialPlugin::<
            ExtendedMaterial<StandardMaterial, TerrainMaterial>,
        >::default());

        app.add_systems(Update, follow.run_if(in_state(Screen::Gameplay)));
        app.add_systems(Update, swap_textures.run_if(in_state(Screen::Gameplay)));
    }
}

fn swap_textures(
    textures: ResMut<WaterHeightTexture>,
    water: Single<
        &MeshMaterial3d<ExtendedMaterial<StandardMaterial, WaterTerrainMaterial>>,
        With<WaterMarker>,
    >,
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, WaterTerrainMaterial>>>,
) {
    let mut a = materials.get_mut(water.0.id()).unwrap();
    if a.extension.water == textures.texture_a {
        a.extension.water = textures.texture_b.clone();
    } else {
        a.extension.water = textures.texture_a.clone();
    }
}
