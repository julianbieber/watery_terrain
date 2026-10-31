pub mod clipmap;

use bevy::{asset::embedded_asset, pbr::ExtendedMaterial, prelude::*, shader::load_shader_library};

use crate::{
    render::clipmap::{TerrainMaterial, WaterTerrainMaterial, follow},
    screens::Screen,
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
    }
}
