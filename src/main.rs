//! Entry point: builds a demo scene from the crate's plugins and runs it.

#![recursion_limit = "256"]

use clap::Parser;

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

use crate::{
    chunks::ChunkPlugin,
    gameplay::Shadows,
    grid::GridPlugin,
    render::TerrainRanderPlugin,
    screens::ScreenPlugin,
    water_sim::{
        DEFAULT_WIND_DEGREES, DEFAULT_WIND_HEAD, DEFAULT_WIND_SPEED, DEFAULT_WIND_WAVELENGTH,
        WaterSimPlugin, Wind,
    },
};

mod chunks;
mod gameplay;
mod grid;
mod heightmap;
mod main_screen;
mod render;
mod screens;
mod tooltip;
mod water_sim;

/// Watery terrain demo.
#[derive(Parser, Debug)]
struct Args {
    /// Which lights cast shadows.
    #[arg(long, value_enum, default_value_t)]
    shadows: Shadows,
    /// Direction the wind's waves travel towards, in degrees from world +x
    /// towards +z.
    #[arg(long, default_value_t = DEFAULT_WIND_DEGREES)]
    wind_direction: f32,
    /// Distance between the wind's wave crests, in metres; at least 1.
    #[arg(long, default_value_t = DEFAULT_WIND_WAVELENGTH, value_parser = at_least(1.0))]
    wind_wavelength: f32,
    /// Speed of the wind's wave crests, in metres per second; below 3.
    #[arg(long, default_value_t = DEFAULT_WIND_SPEED, value_parser = within(0.0, 3.0))]
    wind_speed: f32,
    /// Pressure head the wind pushes the water with, in metres of water; 0
    /// is calm.
    #[arg(long, default_value_t = DEFAULT_WIND_HEAD, value_parser = at_least(0.0))]
    wind_head: f32,
}

impl Args {
    fn wind(&self) -> Wind {
        Wind::towards_degrees(
            self.wind_direction,
            self.wind_wavelength,
            self.wind_speed,
            self.wind_head,
        )
    }
}

fn at_least(min: f32) -> impl Fn(&str) -> Result<f32, String> + Clone + Send + Sync + 'static {
    within(min, f32::INFINITY)
}

fn within(
    min: f32,
    below: f32,
) -> impl Fn(&str) -> Result<f32, String> + Clone + Send + Sync + 'static {
    move |text| {
        let value: f32 = text.parse().map_err(|e| format!("{e}"))?;
        if value >= min && value < below {
            Ok(value)
        } else if below.is_finite() {
            Err(format!("must be at least {min} and below {below}"))
        } else {
            Err(format!("must be at least {min}"))
        }
    }
}

fn main() -> AppExit {
    let args = Args::parse();
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
                night_time_sec: 4.0,
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
            GridPlugin,
            ChunkPlugin,
            WaterSimPlugin,
            TerrainRanderPlugin,
        ))
        .insert_resource(args.shadows)
        .insert_resource(args.wind())
        .run()
}

#[macro_export]
macro_rules! v3 {
    ($x:expr, $y:expr, $z:expr) => {
        bevy::math::Vec3::new($x as f32, $y as f32, $z as f32)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadows_default_to_the_sun() {
        assert_eq!(Args::parse_from(["demo"]).shadows, Shadows::Sun);
    }

    #[test]
    fn shadows_are_read_after_a_space_or_an_equals_sign() {
        assert_eq!(
            Args::parse_from(["demo", "--shadows", "off"]).shadows,
            Shadows::Off
        );
        assert_eq!(
            Args::parse_from(["demo", "--shadows=full"]).shadows,
            Shadows::Full
        );
    }

    #[test]
    fn wind_defaults_to_the_sims_default_wind() {
        assert_eq!(Args::parse_from(["demo"]).wind(), Wind::default());
    }

    #[test]
    fn wind_direction_is_degrees_from_x_towards_z() {
        let wind = Args::parse_from(["demo", "--wind-direction", "90"]).wind();
        assert!(wind.direction.abs_diff_eq(Vec2::Y, 1e-6));
    }

    #[test]
    fn wind_takes_wavelength_speed_and_head() {
        let wind = Args::parse_from([
            "demo",
            "--wind-wavelength=6",
            "--wind-speed=0.8",
            "--wind-head=0.02",
        ])
        .wind();
        assert_eq!((wind.wavelength, wind.speed, wind.head), (6.0, 0.8, 0.02));
    }

    #[test]
    fn wind_outside_what_the_sim_carries_is_rejected() {
        for args in [
            ["demo", "--wind-wavelength=0.5"],
            ["demo", "--wind-speed=3"],
            ["demo", "--wind-speed=-1"],
            ["demo", "--wind-head=-0.01"],
            ["demo", "--wind-head=calm"],
        ] {
            assert!(Args::try_parse_from(args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn unknown_or_missing_shadows_value_is_rejected() {
        assert!(Args::try_parse_from(["demo", "--shadows=soft"]).is_err());
        assert!(Args::try_parse_from(["demo", "--shadows"]).is_err());
    }
}
