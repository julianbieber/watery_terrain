//! The demo scene a free camera flies through and its physics, with a boat
//! the arrow keys sail.

use avian3d::prelude::{
    Collider, Forces, GravityScale, LinearVelocity, RigidBody, WriteRigidBodyForces,
};
use bevy::{
    camera::Exposure,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    core_pipeline::prepass::DepthPrepass,
    image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    light::CascadeShadowConfigBuilder,
    mesh::VertexAttributeValues,
    prelude::*,
};
use bevy_sky_gradient::plugin::SkyboxMagnetTag;
use watery_terrain::{
    Boat, BoatHull, FollowTerrainMarker, SpawnWateryTerrain, WaterDisplacement, WaterSink,
    WaterSource, WateryTerrainPlugin, WateryTerrainSystems, WorldChunks,
};
use watery_terrain_clipmap::{ClipmapPlugin, SpawnClipmapSurfaces, TerrainHeightMapMesh};
use watery_terrain_sdf::{SdfPlugin, SdfShape, SpawnSdfSurfaces};

use crate::screens::Screen;

const TERRAIN_DIR: &str = "assets/terrain";

/// Spawns the gameplay scene on entering [`Screen::Gameplay`], its terrain
/// drawn by `renderer`, and drives it while there.
pub struct GameplayPlugin {
    pub renderer: Renderer,
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code, reason = "each demo binary constructs one renderer")]
pub enum Renderer {
    Clipmap,
    RayMarched,
}

impl Plugin for GameplayPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.renderer);
        match self.renderer {
            Renderer::Clipmap => app.add_plugins(ClipmapPlugin),
            Renderer::RayMarched => app
                .add_plugins(SdfPlugin)
                .add_systems(Update, cycle_sdf_shape.run_if(in_state(Screen::Gameplay))),
        };
        app.init_resource::<Shadows>();
        app.add_systems(Update, apply_sun_shadows);
        app.add_plugins(WateryTerrainPlugin {
            active: Screen::Gameplay,
        });
        app.add_systems(OnEnter(Screen::Gameplay), spawn_player_camera);
        app.add_plugins(FreeCameraPlugin);
        app.add_systems(OnEnter(Screen::Gameplay), spawn_scene);
        app.add_systems(Update, circle_spheres.run_if(in_state(Screen::Gameplay)));
        app.add_systems(Update, paint_boat_model.run_if(in_state(Screen::Gameplay)));
        app.add_systems(
            FixedUpdate,
            steer_boat
                .before(WateryTerrainSystems)
                .run_if(in_state(Screen::Gameplay)),
        );
    }
}

/// Which lights of the scene cast shadows. Read when a light appears, so it
/// is set before the app runs.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Shadows {
    /// The sun with its default cascades, and the camera's point light.
    Full,
    /// The sun alone, with a single cascade.
    #[default]
    Sun,
    /// No light casts shadows.
    Off,
}

fn apply_sun_shadows(
    mut commands: Commands,
    mut suns: Query<(Entity, &mut DirectionalLight), Added<DirectionalLight>>,
    shadows: Res<Shadows>,
) {
    for (entity, mut sun) in &mut suns {
        sun.shadow_maps_enabled = *shadows != Shadows::Off;
        if *shadows == Shadows::Sun {
            commands.entity(entity).insert(
                CascadeShadowConfigBuilder {
                    num_cascades: 1,
                    ..default()
                }
                .build(),
            );
        }
    }
}

const CAMERA_SPAWN: Vec3 = Vec3::new(POND.x, POND.y + 100.0, POND.z + 1.0);

const POND: Vec3 = Vec3::new(-49.0, 65.0, 18.0);

const DROP_HEIGHT: f32 = 23.0;

const SPRING: Vec3 = Vec3::new(POND.x + 20.0, POND.y, POND.z);

const DRAIN: Vec3 = Vec3::new(POND.x + 12.0, POND.y, POND.z);

const SPHERE_RADIUS: f32 = 0.6;

const CIRCLING_SPEED: f32 = 2.0;

const CIRCLING_RADIUS: f32 = 2.5;

#[derive(Component)]
struct CirclingSphere;

const BOAT_SPAWN: Vec3 = Vec3::new(POND.x - 4.0, POND.y + 2.0, POND.z + 6.0);

const BOAT_LENGTH: f32 = 2.2;

const BOAT_HULL_RADIUS: f32 = 0.45;

const BOAT_MODEL: &str = "models/bake/boat.glb";

const BOAT_MODEL_DRAFT: f32 = 0.1;

const BOAT_THRUST: f32 = 3.0;

const BOAT_RUDDER: f32 = 2.0;

const BOAT_PAINT: [Srgba; 4] = [
    Srgba::rgb(0.36, 0.2, 0.1),
    Srgba::rgb(0.72, 0.55, 0.34),
    Srgba::rgb(0.25, 0.18, 0.12),
    Srgba::rgb(0.93, 0.9, 0.82),
];

#[derive(Component)]
struct BoatModel(Handle<Mesh>);

fn spawn_player_camera(mut commands: Commands, shadows: Res<Shadows>, renderer: Res<Renderer>) {
    let mut camera = commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        Camera3d::default(),
        Transform::from_translation(CAMERA_SPAWN).looking_at(POND + Vec3::Y, Vec3::Y),
        FollowTerrainMarker,
        FreeCamera::default(),
        Exposure::from_physical_camera(bevy::camera::PhysicalCameraParameters {
            aperture_f_stops: 1.0,
            shutter_speed_s: 1.0 / 125.0,
            sensitivity_iso: 100.0,
            sensor_height: 0.01866,
        }),
        PointLight {
            shadow_maps_enabled: *shadows == Shadows::Full,
            intensity: 400000.0,
            range: 200000.0,
            color: Color::Srgba(Srgba::RED),
            ..default()
        },
        SkyboxMagnetTag,
    ));
    if *renderer == Renderer::RayMarched {
        camera.insert(DepthPrepass);
    }
}

const SDF_SHAPES: [SdfShape; 4] = [
    SdfShape::Smooth,
    SdfShape::Voxels { level: 0 },
    SdfShape::Voxels { level: 2 },
    SdfShape::Voxels { level: 4 },
];

fn cycle_sdf_shape(keys: Res<ButtonInput<KeyCode>>, mut shape: ResMut<SdfShape>) {
    if keys.just_pressed(KeyCode::KeyV) {
        let at = SDF_SHAPES.iter().position(|s| s == &*shape).unwrap_or(0);
        *shape = SDF_SHAPES[(at + 1) % SDF_SHAPES.len()];
        info!(shape = ?*shape, "terrain shape");
    }
}

fn spawn_scene(
    mut commands: Commands,
    renderer: Res<Renderer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    commands.queue(SpawnWateryTerrain {
        terrain: WorldChunks::load(TERRAIN_DIR),
        centre: CAMERA_SPAWN,
    });
    let water = water_material(&asset_server);
    let land = land_material(&asset_server);
    match *renderer {
        Renderer::Clipmap => commands.queue(SpawnClipmapSurfaces {
            clipmap: TerrainHeightMapMesh {
                smallest_quad: 0.1,
                rings: 5,
                smallest_quad_count: 160,
            },
            water,
            land,
        }),
        Renderer::RayMarched => commands.queue(SpawnSdfSurfaces { water, land }),
    };

    commands.spawn((
        floating_sphere(
            SPHERE_RADIUS,
            POND + Vec3::new(CIRCLING_RADIUS, DROP_HEIGHT, 0.0),
            &mut meshes,
            standard_materials.add(Color::srgb_u8(124, 144, 255)),
        ),
        CirclingSphere,
    ));
    commands.spawn(floating_sphere(
        SPHERE_RADIUS,
        POND + Vec3::new(-8.0, DROP_HEIGHT, 0.0),
        &mut meshes,
        standard_materials.add(Color::srgb_u8(24, 144, 255)),
    ));
    let model = asset_server.load(
        GltfAssetLabel::Primitive {
            mesh: 0,
            primitive: 0,
        }
        .from_asset(BOAT_MODEL),
    );
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        Transform::from_translation(BOAT_SPAWN),
        Boat {
            length: BOAT_LENGTH,
            radius: BOAT_HULL_RADIUS,
        },
        BoatModel(model.clone()),
        children![(
            Transform::from_xyz(0.0, -BOAT_MODEL_DRAFT, 0.0),
            Mesh3d(model),
            MeshMaterial3d(standard_materials.add(StandardMaterial {
                perceptual_roughness: 0.8,
                ..default()
            })),
        )],
    ));
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        Transform::from_translation(SPRING),
        WaterSource {
            radius: 1.5,
            rate: 0.5,
        },
    ));
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        Transform::from_translation(DRAIN),
        WaterSink {
            radius: 1.5,
            rate: 1.0,
        },
    ));
}

fn floating_sphere(
    radius: f32,
    position: Vec3,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
) -> impl Bundle {
    (
        DespawnOnExit(Screen::Gameplay),
        Transform::from_translation(position),
        WaterDisplacement { radius },
        Collider::sphere(radius),
        RigidBody::Dynamic,
        GravityScale(1.0),
        Mesh3d(meshes.add(Sphere::new(radius))),
        MeshMaterial3d(material),
    )
}

fn water_material(asset_server: &AssetServer) -> StandardMaterial {
    exported_material(asset_server, "output/water", ImageAddressMode::Repeat)
}

fn land_material(asset_server: &AssetServer) -> StandardMaterial {
    exported_material(
        asset_server,
        "output/combined",
        ImageAddressMode::ClampToEdge,
    )
}

fn exported_material(
    asset_server: &AssetServer,
    dir: &str,
    address_mode_u: ImageAddressMode,
) -> StandardMaterial {
    let load = |name: &str, is_srgb: bool| -> Handle<Image> {
        asset_server
            .load_builder()
            .with_settings(move |settings: &mut ImageLoaderSettings| {
                settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u,
                    address_mode_v: ImageAddressMode::Repeat,
                    ..ImageSamplerDescriptor::linear()
                });
                settings.is_srgb = is_srgb;
            })
            .load(format!("{dir}/{name}.ktx2"))
    };
    let orm = load("orm", false);
    StandardMaterial {
        base_color_texture: Some(load("base_color", true)),
        normal_map_texture: Some(load("normal", false)),
        occlusion_texture: Some(orm.clone()),
        metallic_roughness_texture: Some(orm),
        perceptual_roughness: 1.0,
        metallic: 1.0,
        reflectance: 1.0,
        ..default()
    }
}

fn circle_spheres(mut spheres: Query<&mut LinearVelocity, With<CirclingSphere>>, time: Res<Time>) {
    let angle = time.elapsed_secs() * CIRCLING_SPEED / CIRCLING_RADIUS;
    for mut velocity in &mut spheres {
        velocity.0.x = -angle.sin() * CIRCLING_SPEED;
        velocity.0.z = angle.cos() * CIRCLING_SPEED;
    }
}

fn paint_boat_model(
    mut events: MessageReader<AssetEvent<Mesh>>,
    boats: Query<&BoatModel>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for event in events.read() {
        let AssetEvent::LoadedWithDependencies { id } = event else {
            continue;
        };
        if !boats.iter().any(|model| model.0.id() == *id) {
            continue;
        }
        let Some(mut mesh) = meshes.get_mut(*id) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x4(weights)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR).cloned()
        else {
            continue;
        };
        let paint: Vec<[f32; 4]> = weights.iter().map(|w| paint_of(*w)).collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, paint);
    }
}

fn paint_of(weights: [f32; 4]) -> [f32; 4] {
    let total = weights.iter().sum::<f32>().max(f32::EPSILON);
    let rgb = weights
        .iter()
        .zip(BOAT_PAINT)
        .fold(Vec3::ZERO, |sum, (w, paint)| {
            sum + paint.to_vec3() * (w / total)
        });
    [rgb.x, rgb.y, rgb.z, 1.0]
}

fn steer_boat(
    keys: Res<ButtonInput<KeyCode>>,
    boats: Query<(&BoatHull, &Transform)>,
    mut hulls: Query<Forces>,
) {
    let thrust = keys.pressed(KeyCode::ArrowUp) as i8 - keys.pressed(KeyCode::ArrowDown) as i8;
    let turn = keys.pressed(KeyCode::ArrowRight) as i8 - keys.pressed(KeyCode::ArrowLeft) as i8;
    if thrust == 0 && turn == 0 {
        return;
    }
    for (hull, transform) in &boats {
        let forward = transform.forward().with_y(0.0).normalize_or_zero();
        let right = transform.right().with_y(0.0).normalize_or_zero();
        if let Ok(mut stern) = hulls.get_mut(hull.stern) {
            stern.apply_linear_acceleration(forward * BOAT_THRUST * f32::from(thrust));
            stern.apply_linear_acceleration(-right * BOAT_RUDDER * f32::from(turn));
        }
        if let Ok(mut bow) = hulls.get_mut(hull.bow) {
            bow.apply_linear_acceleration(right * BOAT_RUDDER * f32::from(turn));
        }
    }
}
