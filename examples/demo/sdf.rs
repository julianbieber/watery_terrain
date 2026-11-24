//! The demo scene drawn with the ray-marched renderer; V cycles the terrain
//! between smooth and voxels of three sizes.

#![recursion_limit = "256"]

use bevy::app::AppExit;

use crate::gameplay::Renderer;

mod app;
mod gameplay;
mod main_screen;
mod screens;
mod tooltip;

fn main() -> AppExit {
    app::run(Renderer::RayMarched)
}
