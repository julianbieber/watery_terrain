//! Bringing a world up from a host's terrain and materials, for the
//! plugin's systems to run on.

use avian3d::prelude::{ColliderDisabled, RigidBody};
use bevy::{
    asset::RenderAssetUsages, camera::visibility::NoFrustumCulling, pbr::ExtendedMaterial,
    prelude::*, render::storage::ShaderBuffer,
};

use crate::{
    TerrainScoped,
    chunks::{
        CHUNK_BYTES, ChunkCollider, ChunkLayout, STAGING_BUFFERS, StagingBuffers, WorldChunks,
    },
    grid::{chunk_of, chunk_origin},
    render::clipmap::{ClipmapMarker, TerrainHeightMapMesh, TerrainMaterial, WaterTerrainMaterial},
    water_sim::{WaterMarker, coarse_window_texture},
};

/// Spawns the water and terrain surfaces of `terrain` and a static collider
/// per chunk, with the window of active chunks centred on the chunk under
/// `centre`, and starts the simulation on it.
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
    pub clipmap: TerrainHeightMapMesh,
    /// The water surface's look; the simulation supplies its shape.
    pub water: StandardMaterial,
    /// The land surface's look; the terrain supplies its shape.
    pub land: StandardMaterial,
}

impl Command for SpawnWateryTerrain {
    type Out = ();

    fn apply(self, world: &mut World) {
        world
            .run_system_cached_with(spawn, self)
            .expect("spawning the watery terrain");
    }
}

fn spawn(
    In(setup): In<SpawnWateryTerrain>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut water_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, WaterTerrainMaterial>>>,
    mut land_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
    mut images: ResMut<Assets<Image>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let SpawnWateryTerrain {
        terrain,
        centre,
        clipmap,
        water,
        land,
    } = setup;
    let layout = ChunkLayout::centred_on(chunk_of(centre.xz()));
    let mut window_image = |water: bool| {
        let mut image = terrain.window_heightmap(&layout, water).image();
        image.asset_usage = RenderAssetUsages::RENDER_WORLD;
        images.add(image)
    };
    let water_heights = window_image(true);
    let land_heights = window_image(false);

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

    commands.spawn((
        TerrainScoped,
        ClipmapMarker,
        NoFrustumCulling,
        Mesh3d(meshes.add(clipmap.create_base_mesh())),
        WaterMarker,
        MeshMaterial3d(water_materials.add(ExtendedMaterial {
            base: water,
            extension: WaterTerrainMaterial {
                water: water_heights,
                base: land_heights.clone(),
                window_origin: layout.origin_uvec(),
                shape: clipmap.shader_shape(),
                coarse_water: images.add(coarse_window_texture()),
                coarse_base: images.add(coarse_window_texture()),
                centre: layout.centre_uvec(),
            },
        })),
    ));

    commands.spawn((
        TerrainScoped,
        ClipmapMarker,
        NoFrustumCulling,
        Mesh3d(meshes.add(clipmap.create_base_mesh())),
        MeshMaterial3d(land_materials.add(ExtendedMaterial {
            base: land,
            extension: TerrainMaterial {
                height: land_heights,
                window_origin: layout.origin_uvec(),
                shape: clipmap.shader_shape(),
            },
        })),
    ));

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
