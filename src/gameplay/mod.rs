//! The gameplay screen: the scene the player flies through and its physics.

use avian3d::{
    PhysicsPlugins,
    prelude::{Collider, ColliderDisabled, Gravity, GravityScale, LinearVelocity, RigidBody},
};
use bevy::{
    asset::RenderAssetUsages,
    camera::Exposure,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    pbr::ExtendedMaterial,
    prelude::*,
    render::storage::ShaderBuffer,
};

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
        app.add_plugins(PhysicsPlugins::default());
        app.insert_resource(Gravity::default());
        app.add_systems(OnEnter(Screen::Gameplay), spawn_player_camera);
        app.add_plugins(FreeCameraPlugin);
        app.add_systems(OnEnter(Screen::Gameplay), spawn_plane_dbg);
        app.add_systems(Update, move_boat.run_if(in_state(Screen::Gameplay)));
    }
}

const CAMERA_SPAWN: Vec3 = Vec3::new(0.0, 100.0, 1.0);

fn spawn_player_camera(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        Camera3d::default(),
        Transform::from_translation(CAMERA_SPAWN).looking_at(Vec3::ZERO + Vec3::Y, Vec3::Y),
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
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let clipmap = TerrainHeightMapMesh {
        smallest_quad: 0.05,
        rings: 5,
        smallest_quad_count: 16 * 10,
    };

    let world = WorldChunks::generate();
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

    commands.spawn(floating_sphere(
        3.0,
        Vec3::Y * 23.0 + Vec3::X * 19.0,
        &mut meshes,
        standard_materials.add(Color::srgb_u8(124, 144, 255)),
    ));
    commands.spawn(floating_sphere(
        1.0,
        Vec3::Y * 23.0 + Vec3::X * -39.0,
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
        WaterDisplacement {
            radius,
            strength: 1.0,
        },
        Collider::sphere(radius),
        RigidBody::Dynamic,
        GravityScale(1.0),
        Mesh3d(meshes.add(Sphere::new(radius))),
        MeshMaterial3d(material),
    )
}

fn water_material(asset_server: &AssetServer) -> StandardMaterial {
    let repeat = |settings: &mut ImageLoaderSettings| {
        settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            ..ImageSamplerDescriptor::linear()
        })
    };
    let linear = move |settings: &mut ImageLoaderSettings| {
        repeat(settings);
        settings.is_srgb = false;
    };

    let base_color: Handle<Image> = asset_server
        .load_builder()
        .with_settings(repeat)
        .load("output/water/base_color.png");
    let normal: Handle<Image> = asset_server
        .load_builder()
        .with_settings(linear)
        .load("output/water/normal.png");
    let orm: Handle<Image> = asset_server
        .load_builder()
        .with_settings(linear)
        .load("output/water/orm.png");
    let emissive: Handle<Image> = asset_server
        .load_builder()
        .with_settings(repeat)
        .load("output/water/emissive.png");
    let transmission: Handle<Image> = asset_server
        .load_builder()
        .with_settings(linear)
        .load("output/water/transmission.png");
    let specular: Handle<Image> = asset_server
        .load_builder()
        .with_settings(repeat)
        .load("output/water/specular.png");
    let clearcoat: Handle<Image> = asset_server
        .load_builder()
        .with_settings(linear)
        .load("output/water/clearcoat.png");
    let clearcoat_normal: Handle<Image> = asset_server
        .load_builder()
        .with_settings(linear)
        .load("output/water/clearcoat_normal.png");
    let anisotropy: Handle<Image> = asset_server
        .load_builder()
        .with_settings(linear)
        .load("output/water/anisotropy.png");
    let depth: Handle<Image> = asset_server
        .load_builder()
        .with_settings(linear)
        .load("output/water/depth.png");
    let material = StandardMaterial {
        base_color_texture: Some(base_color.clone()),
        normal_map_texture: Some(normal.clone()),
        occlusion_texture: Some(orm.clone()),
        metallic_roughness_texture: Some(orm.clone()),
        emissive_texture: Some(emissive.clone()),
        specular_transmission_texture: Some(transmission.clone()),
        thickness_texture: Some(transmission.clone()),
        diffuse_transmission_texture: Some(transmission.clone()),
        specular_tint_texture: Some(specular.clone()),
        specular_texture: Some(specular.clone()),
        clearcoat_texture: Some(clearcoat.clone()),
        clearcoat_roughness_texture: Some(clearcoat.clone()),
        clearcoat_normal_texture: Some(clearcoat_normal.clone()),
        anisotropy_texture: Some(anisotropy.clone()),
        depth_map: Some(depth.clone()),
        emissive: LinearRgba::WHITE,
        perceptual_roughness: 1.0,
        metallic: 1.0,
        reflectance: 1.0,
        clearcoat: 1.0,
        clearcoat_perceptual_roughness: 1.0,
        anisotropy_strength: 1.0,
        specular_transmission: 1.0,
        diffuse_transmission: 1.0,
        thickness: 1.0,
        ..default()
    };
    material
}

fn land_material(asset_server: &AssetServer) -> StandardMaterial {
    let repeat_v = |settings: &mut ImageLoaderSettings| {
        settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_v: ImageAddressMode::Repeat,
            ..ImageSamplerDescriptor::linear()
        })
    };
    let linear = move |settings: &mut ImageLoaderSettings| {
        repeat_v(settings);
        settings.is_srgb = false;
    };

    let base_color: Handle<Image> = asset_server
        .load_builder()
        .with_settings(repeat_v)
        .load("output/combined/base_color.png");
    let normal: Handle<Image> = asset_server
        .load_builder()
        .with_settings(linear)
        .load("output/combined/normal.png");
    let orm: Handle<Image> = asset_server
        .load_builder()
        .with_settings(linear)
        .load("output/combined/orm.png");
    let emissive: Handle<Image> = asset_server
        .load_builder()
        .with_settings(repeat_v)
        .load("output/combined/emissive.png");
    let transmission: Handle<Image> = asset_server
        .load_builder()
        .with_settings(linear)
        .load("output/combined/transmission.png");
    let specular: Handle<Image> = asset_server
        .load_builder()
        .with_settings(repeat_v)
        .load("output/combined/specular.png");
    let clearcoat: Handle<Image> = asset_server
        .load_builder()
        .with_settings(linear)
        .load("output/combined/clearcoat.png");
    let clearcoat_normal: Handle<Image> = asset_server
        .load_builder()
        .with_settings(linear)
        .load("output/combined/clearcoat_normal.png");
    let anisotropy: Handle<Image> = asset_server
        .load_builder()
        .with_settings(linear)
        .load("output/combined/anisotropy.png");
    let depth: Handle<Image> = asset_server
        .load_builder()
        .with_settings(linear)
        .load("output/combined/depth.png");
    let material = StandardMaterial {
        base_color_texture: Some(base_color.clone()),
        normal_map_texture: Some(normal.clone()),
        occlusion_texture: Some(orm.clone()),
        metallic_roughness_texture: Some(orm.clone()),
        emissive_texture: Some(emissive.clone()),
        specular_transmission_texture: Some(transmission.clone()),
        thickness_texture: Some(transmission.clone()),
        diffuse_transmission_texture: Some(transmission.clone()),
        specular_tint_texture: Some(specular.clone()),
        specular_texture: Some(specular.clone()),
        clearcoat_texture: Some(clearcoat.clone()),
        clearcoat_roughness_texture: Some(clearcoat.clone()),
        clearcoat_normal_texture: Some(clearcoat_normal.clone()),
        anisotropy_texture: Some(anisotropy.clone()),
        depth_map: Some(depth.clone()),
        emissive: LinearRgba::WHITE,
        perceptual_roughness: 1.0,
        metallic: 1.0,
        reflectance: 1.0,
        clearcoat: 1.0,
        clearcoat_perceptual_roughness: 1.0,
        anisotropy_strength: 1.0,
        specular_transmission: 1.0,
        diffuse_transmission: 1.0,
        thickness: 1.0,
        ..default()
    };
    material
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
