use avian3d::{
    PhysicsPlugins,
    prelude::{Collider, Gravity, GravityScale, LinearVelocity, RigidBody},
};
use bevy::{
    camera::Exposure,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    image::ImageLoaderSettings,
    pbr::ExtendedMaterial,
    prelude::*,
};

use crate::{
    heightmap::{create_terrain_heightmap, create_water_heightmap},
    render::clipmap::{
        ClipmapMarker, FollowTerrainMarker, TerrainHeightMapMesh, TerrainMaterial,
        WaterTerrainMaterial,
    },
    screens::Screen,
    water_sim::{WaterDisplacement, WaterMarker},
};

pub struct GameplayPlugin;

impl Plugin for GameplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsPlugins::default());
        app.insert_resource(Gravity::default());
        app.add_systems(OnEnter(Screen::Gameplay), spawn_player_camera);
        app.add_plugins(FreeCameraPlugin);
        app.add_systems(OnEnter(Screen::Gameplay), spawn_plane_dbg);
        app.add_systems(Update, move_boat.run_if(in_state(Screen::Gameplay)));
    }
}

fn spawn_player_camera(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        Camera3d::default(),
        Transform::from_translation(Vec3::new(0.0, 100.0, 1.0))
            .looking_at(Vec3::ZERO + Vec3::Y, Vec3::Y),
        FollowTerrainMarker,
        FreeCamera::default(),
        Exposure::from_physical_camera(bevy::camera::PhysicalCameraParameters {
            aperture_f_stops: 1.0,
            shutter_speed_s: 1.0 / 125.0,
            sensitivity_iso: 100.0,
            sensor_height: 0.01866,
        }),
        PointLight {
            shadow_maps_enabled: true,
            intensity: 400000.0,
            range: 200000.0,
            color: Color::Srgba(Srgba::RED),
            ..default()
        },
        // SkyboxMagnetTag, // At the moment the skybox radius seems to hide the terrain
    ));
}

fn spawn_plane_dbg(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, WaterTerrainMaterial>>>,
    mut rock_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
    mut images: ResMut<Assets<Image>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let clipmap = TerrainHeightMapMesh {
        smallest_quad: 0.05,
        rings: 5,
        smallest_quad_count: 16 * 10,
    };

    let heightmap = create_water_heightmap();
    let mesh = clipmap.create_base_mesh();
    let heightmap_texture = images.add(heightmap.image());

    let rock_heightmap = create_terrain_heightmap();
    let rock_mesh = clipmap.create_base_mesh();
    let rock_heightmap_texture = images.add(rock_heightmap.image());
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        ClipmapMarker,
        Mesh3d(meshes.add(mesh)),
        WaterMarker,
        MeshMaterial3d(
            materials.add(ExtendedMaterial {
                base: StandardMaterial {
                    base_color_texture: Some(asset_server.load("water/base_color.png")),
                    emissive_texture: Some(asset_server.load("water/emissive.png")),
                    normal_map_texture: Some(
                        asset_server
                            .load_builder()
                            .with_settings(|settings: &mut ImageLoaderSettings| {
                                settings.is_srgb = false
                            })
                            .load("water/normal.png"),
                    ),
                    metallic: 1.0,             // set, otherwise texture has no effect
                    perceptual_roughness: 1.0, // set, otherwise texture has no effect
                    metallic_roughness_texture: Some(
                        asset_server
                            .load_builder()
                            .with_settings(|settings: &mut ImageLoaderSettings| {
                                settings.is_srgb = false
                            })
                            .load("water/orm.png"),
                    ),
                    occlusion_texture: Some(
                        asset_server
                            .load_builder()
                            .with_settings(|settings: &mut ImageLoaderSettings| {
                                settings.is_srgb = false
                            })
                            .load("water/orm.png"),
                    ),
                    depth_map: Some(
                        asset_server
                            .load_builder()
                            .with_settings(|settings: &mut ImageLoaderSettings| {
                                settings.is_srgb = false
                            })
                            .load("water/depth.png"),
                    ),
                    flip_normal_map_y: true,
                    ior: 1.33,
                    ..Default::default()
                },
                extension: WaterTerrainMaterial {
                    water: heightmap_texture.clone(),
                    base: rock_heightmap_texture.clone(),
                },
            }),
        ),
    ));

    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        ClipmapMarker,
        Mesh3d(meshes.add(rock_mesh)),
        MeshMaterial3d(
            rock_materials.add(ExtendedMaterial::<StandardMaterial, TerrainMaterial> {
                base: StandardMaterial {
                    base_color_texture: Some(asset_server.load("rock/base_color.png")),
                    emissive_texture: Some(asset_server.load("rock/emissive.png")),
                    normal_map_texture: Some(
                        asset_server
                            .load_builder()
                            .with_settings(|settings: &mut ImageLoaderSettings| {
                                settings.is_srgb = false
                            })
                            .load("rock/normal.png"),
                    ),
                    metallic: 1.0,             // set, otherwise texture has no effect
                    perceptual_roughness: 1.0, // set, otherwise texture has no effect
                    metallic_roughness_texture: Some(
                        asset_server
                            .load_builder()
                            .with_settings(|settings: &mut ImageLoaderSettings| {
                                settings.is_srgb = false
                            })
                            .load("rock/orm.png"),
                    ),
                    occlusion_texture: Some(
                        asset_server
                            .load_builder()
                            .with_settings(|settings: &mut ImageLoaderSettings| {
                                settings.is_srgb = false
                            })
                            .load("rock/orm.png"),
                    ),
                    depth_map: Some(
                        asset_server
                            .load_builder()
                            .with_settings(|settings: &mut ImageLoaderSettings| {
                                settings.is_srgb = false
                            })
                            .load("rock/depth.png"),
                    ),
                    flip_normal_map_y: true,
                    ior: 1.33,
                    ..Default::default()
                },
                extension: TerrainMaterial {
                    height: rock_heightmap_texture,
                },
            }),
        ),
    ));
    // seperate entity so the collider does not follow the camera
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        rock_heightmap.avian(),
        RigidBody::Static,
    ));

    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        Transform::from_translation(Vec3::Y * 23.0 + Vec3::X * 19.0),
        WaterDisplacement {
            radius: 3.0,
            strength: 1.0,
        },
        Collider::sphere(3.0),
        RigidBody::Dynamic,
        GravityScale(1.0),
        Mesh3d(meshes.add(Sphere::new(5.0))),
        MeshMaterial3d(standard_materials.add(Color::srgb_u8(124, 144, 255))),
    ));
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        Transform::from_translation(Vec3::Y * 23.0 + Vec3::X * -39.0),
        WaterDisplacement {
            radius: 1.0,
            strength: 1.0,
        },
        Collider::sphere(1.0),
        RigidBody::Dynamic,
        GravityScale(1.0),
        Mesh3d(meshes.add(Sphere::new(1.0))),
        MeshMaterial3d(standard_materials.add(Color::srgb_u8(24, 144, 255))),
    ));
}

fn move_boat(
    mut boat: Query<&mut LinearVelocity, With<WaterDisplacement>>,
    time: Res<Time>,
    // cam: Single<&Transform, (With<Camera>, Without<WaterDisplacement>)>,
) {
    for mut b in &mut boat {
        b.0.z = time.elapsed_secs().sin() * 3.04;
        b.0.x =
            (time.elapsed_secs().cos() + ((time.elapsed_secs() * 0.3).sin().fract() * 2.0)) * 3.04;
    }
}
