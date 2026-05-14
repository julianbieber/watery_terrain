use bevy::{
    feathers::{FeathersPlugins, palette::WHITE},
    prelude::*,
};
use bevy_sky_gradient::{
    ambient_driver::AmbientPaletteBuilder,
    aurora::{AuroraPlugin, AuroraSettings},
    cycle::{SkyCyclePlugin, SkyTime, SkyTimeSettings},
    plugin::SkyPlugin,
    prelude::{AmbientDriverPlugin, GradientDriverPlugin, SkyPalette, SkyPaletteBuilder},
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
                sun_color: vec4(1.0, 1.0, 0.5, 1.0),
                illuminance: 1000.0,
                sun_strength: 4.2,
                sun_sharpness: 364.0,
                sun_light_color: WHITE,
            },
            spawn_default_sun_light: true,
        })
        .set_aurora(AuroraPlugin {
            aurora_settings: AuroraSettings {
                render_texture_percent: 0.25, // Render at 25% resolution
                ..default()
            },
        })
        .set_cycle(SkyCyclePlugin {
            sky_time_settings: SkyTimeSettings {
                day_time_sec: 30.0,
                night_time_sec: 45.0,
                sunrise_time_sec: 7.0,
                sunset_time_sec: 9.0,
            },
            sky_time: SkyTime::default(),
        })
        .set_gradient_driver(GradientDriverPlugin {
            sky_palette_builder: SkyPaletteBuilder::default()
                .with_day(
                    SkyPalette::default()
                        .with_a(v3![0.2, 0.2, 0.5])
                        .with_brightness(0.4),
                )
                .with_night(
                    SkyPalette::default()
                        .with_a(v3![0.1, 0.1, 0.1])
                        .with_brightness(0.0),
                ),
        })
        .set_ambient_driver(AmbientDriverPlugin {
            ambient_palette_builder: AmbientPaletteBuilder::default()
                .with_day_brightness(0.2)
                .with_night_brightness(0.01),
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

#[macro_export]
macro_rules! v3 {
    ($x:expr, $y:expr, $z:expr) => {
        bevy::math::Vec3::new($x as f32, $y as f32, $z as f32)
    };
}
