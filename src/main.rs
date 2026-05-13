use bevy::{feathers::FeathersPlugins, prelude::*};
use bevy_sky_gradient::{
    aurora::{AuroraPlugin, AuroraSettings},
    cycle::SkyCyclePlugin,
    plugin::SkyPlugin,
    prelude::{AmbientDriverPlugin, GradientDriverPlugin},
    sun::{SunDriverPlugin, SunSettings},
};

use crate::{render::TerrainRanderPlugin, screens::ScreenPlugin, water_sim::WaterSimPlugin};

mod gameplay;
mod heightmap;
mod main_screen;
mod render;
mod screens;
mod tooltip;
mod water_sim;

fn main() -> AppExit {
    let sky_plugin = SkyPlugin::builder()
        .set_sun_driver(SunDriverPlugin {
            sun_settings: SunSettings {
                sun_color: vec4(0.0, 0.0, 0.1, 1.0),
                illuminance: 1.0,
                sun_strength: 0.2,
                sun_sharpness: 364.0,
            },
            spawn_default_sun_light: false,
        })
        .set_aurora(AuroraPlugin {
            aurora_settings: AuroraSettings {
                render_texture_percent: 0.25, // Render at 25% resolution
                ..default()
            },
        })
        .set_cycle(SkyCyclePlugin::default())
        .set_gradient_driver(GradientDriverPlugin::default())
        .set_ambient_driver(AmbientDriverPlugin {
            ..Default::default()
        })
        .build();

    App::new()
        .add_plugins((
            DefaultPlugins,
            sky_plugin,
            FeathersPlugins,
            ScreenPlugin,
            WaterSimPlugin,
            TerrainRanderPlugin,
        ))
        .run()
}
