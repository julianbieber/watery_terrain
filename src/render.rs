use bevy::{
    asset::{AssetPath, RenderAssetUsages, embedded_asset, embedded_path},
    mesh::PrimitiveTopology,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

use crate::{heightmap::create_heightmap, screens::Screen};

pub struct TerrainRanderPlugin;

impl Plugin for TerrainRanderPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "terrain.wgsl");

        app.add_plugins(MaterialPlugin::<
            ExtendedMaterial<StandardMaterial, TerrainMaterial>,
        >::default());
        app.add_systems(OnEnter(Screen::Gameplay), spawn_plane_dbg);
        app.add_systems(Update, follow.run_if(in_state(Screen::Gameplay)));
    }
}

#[derive(Component)]
pub struct FollowTerrainMarker;

#[derive(Component)]
struct TerrainMarker;

fn spawn_plane_dbg(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
    mut images: ResMut<Assets<Image>>,
) {
    let terrain = TerrainHeightMapMesh {
        smallest_quad: 0.2,
        rings: 12,
    };

    let heightmap = create_heightmap();
    let mesh = terrain.create_base_mesh();
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        TerrainMarker,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(ExtendedMaterial {
            base: StandardMaterial {
                base_color: Color::Srgba(Srgba::GREEN),
                ..Default::default()
            },
            extension: TerrainMaterial {
                height: images.add(heightmap.image()),
            },
        })),
        heightmap,
    ));
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            intensity: 4000.0,
            range: 200.0,
            ..default()
        },
        Transform::from_xyz(4.0, 20.0, 4.0),
    ));
}

fn follow(
    following: Single<&Transform, (With<FollowTerrainMarker>, Without<TerrainMarker>)>,
    mut terrain: Query<&mut Transform, With<TerrainMarker>>,
) {
    for mut t in &mut terrain {
        t.translation.x = following.translation.x;
        t.translation.z = following.translation.z;
    }
}

#[derive(Component)]
pub struct TerrainHeightMapMesh {
    pub smallest_quad: f32,
    pub rings: u8,
}

struct QuadMeshBuilder {
    vertices: Vec<Vec3>,
    indices: Vec<u32>,
}

impl QuadMeshBuilder {
    fn empty() -> QuadMeshBuilder {
        QuadMeshBuilder {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    fn add_quad(&mut self, bottom_left: Vec3, width: f32) {
        let o = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&[
            bottom_left,
            bottom_left.with_x(bottom_left.x + width),
            bottom_left.with_z(bottom_left.z + width),
            bottom_left
                .with_x(bottom_left.x + width)
                .with_z(bottom_left.z + width),
        ]);
        self.indices
            .extend_from_slice(&[o, o + 2, o + 1, o + 2, o + 3, o + 1]);
    }

    fn add_subdivided_quad(&mut self, bottom_left: Vec3, quad_width: f32, divisions: u8) {
        for x in 0..divisions {
            for y in 0..divisions {
                let local_bottom_left =
                    bottom_left + Vec3::X * quad_width * x as f32 + Vec3::Z * quad_width * y as f32;
                self.add_quad(local_bottom_left, quad_width);
            }
        }
    }

    fn build(&self) -> Mesh {
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.vertices.clone());
        m.insert_indices(bevy::mesh::Indices::U32(self.indices.clone()));

        m
    }
}

impl TerrainHeightMapMesh {
    fn create_base_mesh(&self) -> Mesh {
        let mut m = QuadMeshBuilder::empty();
        let mut bottom_left = Vec3::new(-self.smallest_quad * 8.0, 0.0, -self.smallest_quad * 8.0);
        m.add_subdivided_quad(bottom_left, self.smallest_quad, 16);
        let mut quad_size = self.smallest_quad;

        for _ in 0..self.rings {
            quad_size *= 2.0;
            bottom_left -= Vec3::new(quad_size * 4.0, 0.0, quad_size * 4.0);
            for (x, y) in [
                (0.0, 0.0),
                (0.0, 1.0),
                (0.0, 2.0),
                (0.0, 3.0),
                (1.0, 0.0),
                (1.0, 3.0),
                (2.0, 0.0),
                (2.0, 3.0),
                (3.0, 0.0),
                (3.0, 1.0),
                (3.0, 2.0),
                (3.0, 3.0),
            ] {
                m.add_subdivided_quad(
                    bottom_left + Vec3::new(quad_size * x * 4.0, 0.0, quad_size * y * 4.0),
                    quad_size,
                    4,
                );
            }
        }
        debug!(bottom_left = ?bottom_left, "last bottom left");

        m.build()
    }
}

#[derive(Asset, AsBindGroup, Debug, Clone, Reflect)]
struct TerrainMaterial {
    #[texture(100)]
    #[sampler(101)]
    height: Handle<Image>,
}

impl MaterialExtension for TerrainMaterial {
    fn vertex_shader() -> bevy::shader::ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("terrain.wgsl")).with_source("embedded"),
        )
    }

    fn enable_prepass() -> bool {
        true
    }

    fn enable_shadows() -> bool {
        true
    }

    fn prepass_vertex_shader() -> bevy::shader::ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("terrain.wgsl")).with_source("embedded"),
        )
    }

    fn deferred_vertex_shader() -> bevy::shader::ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("terrain.wgsl")).with_source("embedded"),
        )
    }
}
