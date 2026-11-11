//! The demo scene a free camera flies through and its physics.

use avian3d::{
    PhysicsPlugins,
    prelude::{Collider, ColliderDisabled, Gravity, GravityScale, LinearVelocity, RigidBody},
};
use bevy::{
    asset::RenderAssetUsages,
    camera::Exposure,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    light::CascadeShadowConfigBuilder,
    pbr::ExtendedMaterial,
    prelude::*,
    render::storage::ShaderBuffer,
};

use bevy_sky_gradient::plugin::SkyboxMagnetTag;

use crate::{
    chunks::{
        CHUNK_BYTES, ChunkCollider, ChunkLayout, STAGING_BUFFERS, StagingBuffers, WorldChunks,
    },
    grid::{chunk_of, chunk_origin},
    render::clipmap::{
        ClipmapMarker, FollowTerrainMarker, TerrainHeightMapMesh, TerrainMaterial,
        WaterTerrainMaterial,
    },
    screens::Screen,
    water_sim::{WaterDisplacement, WaterMarker},
};

/// Spawns the gameplay scene on entering [`Screen::Gameplay`] and drives it
/// while there.
pub struct GameplayPlugin;

impl Plugin for GameplayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shadows>();
        app.add_systems(Update, apply_sun_shadows);
        app.add_plugins(PhysicsPlugins::default());
        app.insert_resource(Gravity::default());
        app.add_systems(OnEnter(Screen::Gameplay), spawn_player_camera);
        app.add_plugins(FreeCameraPlugin);
        app.add_systems(OnEnter(Screen::Gameplay), spawn_plane_dbg);
        app.add_systems(Update, circle_spheres.run_if(in_state(Screen::Gameplay)));
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

const SPHERE_RADIUS: f32 = 0.6;

const CIRCLING_SPEED: f32 = 2.0;

const CIRCLING_RADIUS: f32 = 2.5;

#[derive(Component)]
struct CirclingSphere;

fn spawn_player_camera(mut commands: Commands, shadows: Res<Shadows>) {
    commands.spawn((
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
}

fn spawn_plane_dbg(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, WaterTerrainMaterial>>>,
    mut rock_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
    mut images: ResMut<Assets<Image>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let clipmap = TerrainHeightMapMesh {
        smallest_quad: 0.05,
        rings: 5,
        smallest_quad_count: 16 * 10,
    };

    let world = WorldChunks::load();
    let layout = ChunkLayout::centred_on(chunk_of(CAMERA_SPAWN.xz()));
    let window_image = |water: bool| {
        let mut image = world.window_heightmap(&layout, water).image();
        image.asset_usage = RenderAssetUsages::RENDER_WORLD;
        image
    };

    let mesh = clipmap.create_base_mesh();
    let heightmap_texture = images.add(window_image(true));

    let rock_mesh = clipmap.create_base_mesh();
    let rock_heightmap_texture = images.add(window_image(false));

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
    let material = water_material(&asset_server);
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        ClipmapMarker,
        Mesh3d(meshes.add(mesh)),
        WaterMarker,
        MeshMaterial3d(materials.add(ExtendedMaterial {
            base: material,
            extension: WaterTerrainMaterial {
                water: heightmap_texture.clone(),
                base: rock_heightmap_texture.clone(),
                window_origin: layout.origin_uvec(),
                shape: clipmap.shader_shape(),
            },
        })),
    ));

    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        ClipmapMarker,
        Mesh3d(meshes.add(rock_mesh)),
        MeshMaterial3d(
            rock_materials.add(ExtendedMaterial::<StandardMaterial, TerrainMaterial> {
                base: land_material(&asset_server),
                extension: TerrainMaterial {
                    height: rock_heightmap_texture,
                    window_origin: layout.origin_uvec(),
                    shape: clipmap.shader_shape(),
                },
            }),
        ),
    ));
    for coord in world.coords() {
        let mut collider = commands.spawn((
            DespawnOnExit(Screen::Gameplay),
            world.collider_heights(coord).avian(chunk_origin(coord)),
            RigidBody::Static,
            ChunkCollider { coord },
        ));
        if !layout.contains(coord) {
            collider.insert(ColliderDisabled);
        }
    }
    commands.insert_resource(layout);
    commands.insert_resource(world);

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
