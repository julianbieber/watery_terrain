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
    heightmap::create_heightmap,
    render::clipmap::{FollowTerrainMarker, TerrainHeightMapMesh, TerrainMarker, TerrainMaterial},
    screens::Screen,
    water_sim::WaterDisplacement,
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
        // app.add_systems(
        //     Update,
        //     pulse_ambient_light.run_if(in_state(Screen::Gameplay)),
        // );
    }
}

fn spawn_player_camera(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        Camera3d::default(),
        Transform::from_translation(Vec3::new(0.0, 20.0, -1.0)).looking_at(Vec3::ZERO, Vec3::Y),
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
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
    mut images: ResMut<Assets<Image>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let terrain = TerrainHeightMapMesh {
        smallest_quad: 0.05,
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
                base_color_texture: Some(asset_server.load("water/base_color.png")),
                emissive_texture: Some(asset_server.load("water/emissive.png")),
                normal_map_texture: Some(asset_server.load_with_settings(
                    "water/normal.png",
                    |settings: &mut ImageLoaderSettings| settings.is_srgb = false,
                )),
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
                depth_map: Some(asset_server.load_with_settings(
                    "water/depth.png",
                    |settings: &mut ImageLoaderSettings| settings.is_srgb = false,
                )),
                flip_normal_map_y: true,
                ..Default::default()
            },
            extension: TerrainMaterial {
                height: heightmap_texture.clone(),
            },
        })),
    ));

    // commands.insert_resource(GlobalAmbientLight {
    //     color: WHITE.into(),
    //     brightness: 50.0,
    //     ..default()
    // });

    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        Transform::from_translation(Vec3::ZERO),
        WaterDisplacement {
            radius: 5.0,
            strength: 9.0,
        },
        Collider::sphere(5.0),
        RigidBody::Dynamic,
        GravityScale(1.0),
        Mesh3d(meshes.add(Sphere::new(5.0))),
        MeshMaterial3d(standard_materials.add(Color::srgb_u8(124, 144, 255))),
    ));
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        Transform::from_translation(Vec3::new(10.0, 0.0, 20.0)),
        WaterDisplacement {
            radius: 1.0,
            strength: 3.0,
        },
        Collider::sphere(15.0),
        RigidBody::Dynamic,
        GravityScale(1.0),
        Mesh3d(meshes.add(Sphere::new(1.0))),
        MeshMaterial3d(standard_materials.add(Color::srgb_u8(24, 144, 255))),
    ));

    // commands.spawn((
    //     DirectionalLight {
    //         illuminance: light_consts::lux::OVERCAST_DAY,
    //         shadows_enabled: true,
    //         ..default()
    //     },
    //     Transform {
    //         translation: Vec3::new(0.0, 2.0, 0.0),
    //         rotation: Quat::from_rotation_x(-PI / 4.),
    //         ..default()
    //     },
    //     // The default cascade config is designed to handle large scenes.
    //     // As this example has a much smaller world, we can tighten the shadow
    //     // bounds for better visual quality.
    //     CascadeShadowConfigBuilder {
    //         first_cascade_far_bound: 4.0,
    //         maximum_distance: 10.0,
    //         ..default()
    //     }
    //     .build(),
    // ));
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
// fn pulse_ambient_light(time: Res<Time>, mut ambient: ResMut<GlobalAmbientLight>) {
//     // --- tuneable constants ---
//     const MIN_BRIGHTNESS: f32 = 0.0;
//     const MAX_BRIGHTNESS: f32 = 100.0;
//     const CYCLE_SECS: f32 = 8.0; // full up-down-up period in seconds

//     // sin oscillates in [-1, 1]; remap to [0, 1] then scale to [MIN, MAX]
//     let t = time.elapsed_secs();
//     let wave = (t * std::f32::consts::TAU / CYCLE_SECS).sin(); // -1 ..= 1
//     let normalized = (wave + 1.0) * 0.5; //  0 ..= 1
//     ambient.brightness = MIN_BRIGHTNESS + normalized * (MAX_BRIGHTNESS - MIN_BRIGHTNESS);
// }
