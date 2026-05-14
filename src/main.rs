use bevy::{
    feathers::{FeathersPlugins, palette::WHITE},
    prelude::*,
};
use bevy_sky_gradient::{
    ambient_driver::{AmbientColorsBuilder, ScalarGradientBuilder},
    aurora::{AuroraPlugin, AuroraSettings},
    cycle::SkyCyclePlugin,
    gradient::{GradientBuilder, SkyGradientBuilder},
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
    let m = 0.0;
    let sky_plugin = SkyPlugin::builder()
        .set_sun_driver(SunDriverPlugin {
            sun_settings: SunSettings {
                sun_color: vec4(1.0, 1.0, 0.5, 1.0),
                illuminance: 10000.0,
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
        .set_cycle(SkyCyclePlugin::default())
        .set_gradient_driver(GradientDriverPlugin {
            sky_colors_builder: SkyGradientBuilder::default()
                .with_div_stop0(100)
                .with_div_stop1(100)
                .with_div_stop2(100)
                .with_div_stop3(100),
        })
        .set_ambient_driver(AmbientDriverPlugin {
            ambient_colors_builder: AmbientColorsBuilder {
                color_gradient: GradientBuilder {
                    sunrise_color: [255, 255, 200, 255],
                    day_low_color: [255, 255, 150, 255],
                    day_high_color: [255, 255, 200, 255],
                    sunset_color: [240, 240, 255, 255],
                    night_low_color: [150, 150, 225, 255],
                    night_high_color: [100, 100, 150, 255],
                },
                scalar_gradient: ScalarGradientBuilder {
                    sunrise_color: 0.4 * m,
                    day_low_color: 0.6 * m,
                    day_high_color: 1.0 * m,
                    sunset_color: 0.4 * m,
                    night_low_color: 0.3 * m,
                    night_high_color: 0.15 * m,
                },
            },
            ambient_settings: bevy_sky_gradient::prelude::AmbientSettings {
                brightness_multiplier: 1.0,
            },
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
