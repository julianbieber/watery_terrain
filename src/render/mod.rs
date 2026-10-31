pub mod clipmap;

use bevy::{
    asset::embedded_asset,
    pbr::ExtendedMaterial,
    prelude::*,
    shader::{ShaderDefVal, ShaderSettings, load_shader_library},
};

use crate::{
    render::clipmap::{TerrainMaterial, WaterTerrainMaterial, follow},
    screens::Screen,
    water_sim::DRY_DEPTH_MM,
};

pub struct TerrainRanderPlugin;

impl Plugin for TerrainRanderPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "terrain.wesl");
        load_shader_library!(app, "water.wesl", |settings: &mut ShaderSettings| {
            settings.shader_defs = vec![ShaderDefVal::UInt("DRY_DEPTH_MM".into(), DRY_DEPTH_MM)];
        });
        load_shader_library!(app, "heightfield.wesl");
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
