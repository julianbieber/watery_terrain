use bevy::{
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    pbr::ExtendedMaterial,
    prelude::*,
};

use crate::{
    heightmap::create_heightmap,
    render::clipmap::{FollowTerrainMarker, TerrainHeightMapMesh, TerrainMarker, TerrainMaterial},
    screens::Screen,
};

pub struct GameplayPlugin;

impl Plugin for GameplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::Gameplay), spawn_player_camera);
        app.add_plugins(FreeCameraPlugin);
        app.add_systems(OnEnter(Screen::Gameplay), spawn_plane_dbg);
    }
}

fn spawn_player_camera(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        Camera3d::default(),
        Transform::from_translation(Vec3::new(0.0, 20.0, -1.0)).looking_at(Vec3::ZERO, Vec3::Y),
        FollowTerrainMarker,
        FreeCamera::default(),
    ));
}

fn spawn_plane_dbg(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
    mut images: ResMut<Assets<Image>>,
) {
    let terrain = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 5,
        smallest_quad_count: 16 * 10,
    };

    let heightmap = create_heightmap();
    let mesh = terrain.create_base_mesh();
    let heightmap_texture = images.add(heightmap.image());
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        TerrainMarker,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(ExtendedMaterial {
            base: StandardMaterial {
                base_color: Color::Srgba(Srgba::GREEN),
                unlit: false,
                ..Default::default()
            },
            extension: TerrainMaterial {
                height: heightmap_texture.clone(),
            },
        })),
    ));
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            intensity: 40000.0,
            range: 2000.0,
            color: Color::Srgba(Srgba::BLUE),
            ..default()
        },
        Transform::from_xyz(4.0, 220.0, 4.0),
    ));
}
