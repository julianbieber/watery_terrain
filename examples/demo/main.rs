//! A scene to fly through over the shipped terrain, with floating spheres, a
//! spring and a drain: the library at work in a host app with its own menus,
//! sky and camera, drawn with the clipmap renderer.

#![recursion_limit = "256"]

use bevy::app::AppExit;

use crate::gameplay::Renderer;

mod app;
mod gameplay;
mod main_screen;
mod screens;
mod tooltip;

fn main() -> AppExit {
    app::run(Renderer::Clipmap)
}
