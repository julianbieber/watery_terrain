use bevy::{feathers::FeathersPlugins, prelude::*};

use crate::{render::TerrainRanderPlugin, screens::ScreenPlugin, water_sim::WaterSimPlugin};

mod gameplay;
mod heightmap;
mod main_screen;
mod render;
mod screens;
mod tooltip;
mod water_sim;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            FeathersPlugins,
            ScreenPlugin,
            WaterSimPlugin,
            TerrainRanderPlugin,
        ))
        .run()
}
