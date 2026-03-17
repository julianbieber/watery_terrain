use bevy::{feathers::FeathersPlugins, prelude::*};

use crate::{render::TerrainRanderPlugin, screens::ScreenPlugin};

mod gameplay;
mod main_screen;
mod render;
mod screens;
mod tooltip;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            FeathersPlugins,
            ScreenPlugin,
            TerrainRanderPlugin,
        ))
        .run()
}
