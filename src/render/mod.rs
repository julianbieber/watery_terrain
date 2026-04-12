pub mod clipmap;

use bevy::{asset::embedded_asset, pbr::ExtendedMaterial, prelude::*};

use crate::{
    render::clipmap::{TerrainMaterial, follow},
    screens::Screen,
    water_sim::WaterHeightTexture,
};

pub struct TerrainRanderPlugin;

impl Plugin for TerrainRanderPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "terrain.wgsl");

        app.add_plugins(MaterialPlugin::<
            ExtendedMaterial<StandardMaterial, TerrainMaterial>,
        >::default());

        app.add_systems(Update, follow.run_if(in_state(Screen::Gameplay)));
        app.add_systems(Update, swap_textures.run_if(in_state(Screen::Gameplay)));
    }
}

fn swap_textures(
    textures: ResMut<WaterHeightTexture>,
    water: Single<&MeshMaterial3d<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
) {
    let a = materials.get_mut(water.0.id()).unwrap();
    if a.extension.height == textures.texture_a {
        a.extension.height = textures.texture_b.clone();
    } else {
        a.extension.height = textures.texture_a.clone();
    }
}
