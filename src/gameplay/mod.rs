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
use bevy_sky_gradient::plugin::SkyboxMagnetTag;

use crate::{
    heightmap::{create_terrain_heightmap, create_water_heightmap},
    render::clipmap::{
        ClipmapMarker, FollowTerrainMarker, TerrainHeightMapMesh,
        TiledTerrainMaterial, TiledWaterTerrainMaterial,
    },
    screens::Screen,
    water_sim::{WaterDisplacement, WaterMarker},
};

pub struct GameplayPlugin;

impl Plugin for GameplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsPlugins::default());
        // app.add_plugins(PhysicsDebugPlugin {});
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
        Transform::from_translation(Vec3::new(0.0, 10.0, 1.0))
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
            shadows_enabled: true,
            intensity: 400000.0,
            range: 200000.0,
            color: Color::Srgba(Srgba::RED),
            ..default()
        },
        SkyboxMagnetTag,
    ));
}

fn spawn_plane_dbg(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, TiledWaterTerrainMaterial>>>,
    mut tiled_rock_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, TiledTerrainMaterial>>>,
    mut images: ResMut<Assets<Image>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let clipmap = TerrainHeightMapMesh {
        smallest_quad: 0.05,
        rings: 5,
        smallest_quad_count: 16 * 10,
    };

    // Create 9 heightmap tiles for water and terrain
    let mut water_heightmap_tiles = Vec::new();
    let mut rock_heightmap_tiles = Vec::new();
    
    for _ in 0..9 {
        let water_hm = create_water_heightmap();
        water_heightmap_tiles.push(images.add(water_hm.image()));
        
        let rock_hm = create_terrain_heightmap();
        rock_heightmap_tiles.push(images.add(rock_hm.image()));
    }

    let mesh = clipmap.create_base_mesh();
    let rock_mesh = clipmap.create_base_mesh();
    // Create tiled water material
    let water_material = TiledWaterTerrainMaterial {
        water_00: water_heightmap_tiles[0].clone(),
        water_01: water_heightmap_tiles[1].clone(),
        water_02: water_heightmap_tiles[2].clone(),
        water_10: water_heightmap_tiles[3].clone(),
        water_11: water_heightmap_tiles[4].clone(),
        water_12: water_heightmap_tiles[5].clone(),
        water_20: water_heightmap_tiles[6].clone(),
        water_21: water_heightmap_tiles[7].clone(),
        water_22: water_heightmap_tiles[8].clone(),
        base_00: rock_heightmap_tiles[0].clone(),
        base_01: rock_heightmap_tiles[1].clone(),
        base_02: rock_heightmap_tiles[2].clone(),
        base_10: rock_heightmap_tiles[3].clone(),
        base_11: rock_heightmap_tiles[4].clone(),
        base_12: rock_heightmap_tiles[5].clone(),
        base_20: rock_heightmap_tiles[6].clone(),
        base_21: rock_heightmap_tiles[7].clone(),
        base_22: rock_heightmap_tiles[8].clone(),
    };

    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        ClipmapMarker,
        Mesh3d(meshes.add(mesh)),
        WaterMarker,
        MeshMaterial3d(materials.add(ExtendedMaterial {
            base: StandardMaterial {
                base_color_texture: Some(asset_server.load("water/base_color.png")),
                emissive_texture: Some(asset_server.load("water/emissive.png")),
                normal_map_texture: Some(asset_server.load_with_settings(
                    "water/normal.png",
                    |settings: &mut ImageLoaderSettings| settings.is_srgb = false,
                )),
                metallic: 1.0,
                perceptual_roughness: 1.0,
                metallic_roughness_texture: Some(
                    asset_server.load_with_settings(
                        "water/orm.png",
                        |settings: &mut ImageLoaderSettings| settings.is_srgb = false,
                    ),
                ),
                occlusion_texture: Some(
                    asset_server.load_with_settings(
                        "water/orm.png",
                        |settings: &mut ImageLoaderSettings| settings.is_srgb = false,
                    ),
                ),
                depth_map: Some(
                    asset_server.load_with_settings(
                        "water/depth.png",
                        |settings: &mut ImageLoaderSettings| settings.is_srgb = false,
                    ),
                ),
                flip_normal_map_y: true,
                ior: 1.33,
                ..Default::default()
            },
            extension: water_material,
        })),
    ));

    // Create tiled terrain material
    let terrain_material = TiledTerrainMaterial {
        height_00: rock_heightmap_tiles[0].clone(),
        height_01: rock_heightmap_tiles[1].clone(),
        height_02: rock_heightmap_tiles[2].clone(),
        height_10: rock_heightmap_tiles[3].clone(),
        height_11: rock_heightmap_tiles[4].clone(),
        height_12: rock_heightmap_tiles[5].clone(),
        height_20: rock_heightmap_tiles[6].clone(),
        height_21: rock_heightmap_tiles[7].clone(),
        height_22: rock_heightmap_tiles[8].clone(),
    };

    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        ClipmapMarker,
        Mesh3d(meshes.add(rock_mesh)),
        MeshMaterial3d(tiled_rock_materials.add(ExtendedMaterial {
            base: StandardMaterial {
                base_color_texture: Some(asset_server.load("rock/base_color.png")),
                emissive_texture: Some(asset_server.load("rock/emissive.png")),
                normal_map_texture: Some(asset_server.load_with_settings(
                    "rock/normal.png",
                    |settings: &mut ImageLoaderSettings| settings.is_srgb = false,
                )),
                metallic: 1.0,
                perceptual_roughness: 1.0,
                metallic_roughness_texture: Some(
                    asset_server.load_with_settings(
                        "rock/orm.png",
                        |settings: &mut ImageLoaderSettings| settings.is_srgb = false,
                    ),
                ),
                occlusion_texture: Some(
                    asset_server.load_with_settings(
                        "rock/orm.png",
                        |settings: &mut ImageLoaderSettings| settings.is_srgb = false,
                    ),
                ),
                depth_map: Some(
                    asset_server.load_with_settings(
                        "rock/depth.png",
                        |settings: &mut ImageLoaderSettings| settings.is_srgb = false,
                    ),
                ),
                flip_normal_map_y: true,
                ior: 1.33,
                ..Default::default()
            },
            extension: terrain_material,
        })),
    ));
    
    // Use center rock heightmap for collider
    let center_rock_heightmap = create_terrain_heightmap();
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        center_rock_heightmap.avian(),
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
