//! Chunked terrain with a GPU shallow-water simulation on it, colliding and
//! simulated in a window of chunks that follows one entity, with boats on it,
//! for a Bevy app to add with [`WateryTerrainPlugin`] and bring up with
//! [`SpawnWateryTerrain`].
//! Drawing it is a renderer crate's job, built on [`render`].

mod boat;
mod chunks;
pub mod grid;
mod heightmap;
pub mod render;
mod setup;
mod water_sim;

use avian3d::{PhysicsPlugins, prelude::PhysicsSchedulePlugin};
use bevy::prelude::*;

use crate::{
    boat::BoatPlugin, chunks::ChunkPlugin, grid::GridPlugin, render::SurfaceShaderPlugin,
    water_sim::WaterSimPlugin,
};

pub use crate::{
    boat::{Boat, BoatHull},
    chunks::{FollowTerrainMarker, WorldChunks},
    setup::SpawnWateryTerrain,
    water_sim::{
        DEFAULT_WIND_DEGREES, DEFAULT_WIND_HEAD, DEFAULT_WIND_SPEED, DEFAULT_WIND_WAVELENGTH,
        WaterAtObjects, WaterColumn, WaterDisplacement, WaterSink, WaterSource, Wind,
    },
};

/// Simulates and collides the terrain and its water while the app is in
/// the state `active`. Leaving it stops the simulation, despawns every entity
/// [`SpawnWateryTerrain`] and the simulation spawned, and drops the world with
/// its GPU state; a new [`SpawnWateryTerrain`] brings one up again.
///
/// Adds avian's [`PhysicsPlugins`] unless the app already has them.
pub struct WateryTerrainPlugin<S: States> {
    pub active: S,
}

/// The library's `Update` and `FixedUpdate` systems, all gated on
/// [`WateryTerrainPlugin`]'s state, for ordering a host's systems around them.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct WateryTerrainSystems;

/// An entity that leaving [`WateryTerrainPlugin`]'s state despawns.
#[derive(Component, Default)]
pub struct TerrainScoped;

impl<S: States> Plugin for WateryTerrainPlugin<S> {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<PhysicsSchedulePlugin>() {
            app.add_plugins(PhysicsPlugins::default());
        }
        let active = || in_state(self.active.clone());
        app.configure_sets(Update, WateryTerrainSystems.run_if(active()));
        app.configure_sets(FixedUpdate, WateryTerrainSystems.run_if(active()));
        app.add_systems(
            OnExit(self.active.clone()),
            (
                despawn_scoped,
                chunks::drop_world,
                water_sim::stop_simulation,
            ),
        );
        app.add_plugins((
            GridPlugin,
            ChunkPlugin,
            WaterSimPlugin,
            SurfaceShaderPlugin,
            BoatPlugin,
        ));
    }
}

fn despawn_scoped(mut commands: Commands, scoped: Query<Entity, With<TerrainScoped>>) {
    for entity in &scoped {
        commands.entity(entity).try_despawn();
    }
}
