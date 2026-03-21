use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology, prelude::*};

use crate::screens::Screen;

pub struct TerrainRanderPlugin;

impl Plugin for TerrainRanderPlugin {
    fn build(&self, app: &mut App) {
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
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let terrain = TerrainHeightMap {
        smallest_quad: 0.2,
        rings: 12,
    };

    let mesh = terrain.create_base_mesh();
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        TerrainMarker,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.5, 0.2))),
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
pub struct TerrainHeightMap {
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

impl TerrainHeightMap {
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

        m.build()
    }
}
