//! Bringing a world up from a host's terrain, for the plugin's systems to run
//! on.

use avian3d::prelude::{ColliderDisabled, RigidBody};
use bevy::{asset::RenderAssetUsages, prelude::*, render::storage::ShaderBuffer};

use crate::{
    TerrainScoped,
    chunks::{
        CHUNK_BYTES, ChunkCollider, ChunkLayout, STAGING_BUFFERS, StagingBuffers, WindowHeights,
        WorldChunks,
    },
    grid::{chunk_of, chunk_origin},
    water_sim::start_simulation,
};

/// Brings up `terrain` with a static collider per chunk and the window of
/// active chunks centred on the chunk under `centre`, and starts the
/// simulation on it. Nothing is drawn until a renderer crate's command spawns
/// the surfaces, queued after this one.
///
/// Queue it once, from a system running in [`WateryTerrainPlugin`]'s state;
/// leaving that state despawns what it spawned. The window then follows the
/// entity carrying [`FollowTerrainMarker`], which should start over `centre`.
///
/// [`WateryTerrainPlugin`]: crate::WateryTerrainPlugin
/// [`FollowTerrainMarker`]: crate::FollowTerrainMarker
pub struct SpawnWateryTerrain {
    pub terrain: WorldChunks,
    pub centre: Vec3,
}

impl Command for SpawnWateryTerrain {
    type Out = ();

    fn apply(self, world: &mut World) {
        let SpawnWateryTerrain { terrain, centre } = self;
        world
            .run_system_cached_with(spawn, (terrain, centre))
            .expect("spawning the watery terrain");
        world
            .run_system_cached(start_simulation)
            .expect("starting the water simulation");
    }
}

fn spawn(
    In((terrain, centre)): In<(WorldChunks, Vec3)>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let layout = ChunkLayout::centred_on(chunk_of(centre.xz()));
    commands.insert_resource(WindowHeights::new(&terrain, &layout, &mut images));

    commands.insert_resource(StagingBuffers {
        buffers: (0..STAGING_BUFFERS)
            .map(|_| {
                buffers.add(ShaderBuffer::with_size(
                    CHUNK_BYTES,
                    RenderAssetUsages::RENDER_WORLD,
                ))
            })
            .collect(),
        in_use_mask: 0,
    });

    for coord in terrain.coords() {
        let mut collider = commands.spawn((
            TerrainScoped,
            terrain.collider_heights(coord).avian(chunk_origin(coord)),
            RigidBody::Static,
            ChunkCollider { coord },
        ));
        if !layout.contains(coord) {
            collider.insert(ColliderDisabled);
        }
    }
    commands.insert_resource(layout);
    commands.insert_resource(terrain);
}
