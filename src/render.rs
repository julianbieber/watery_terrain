use bevy::{
    asset::RenderAssetUsages,
    mesh::{MeshVertexAttribute, PrimitiveTopology},
    prelude::*,
    render::render_resource::RenderPassColorAttachment,
};

pub struct TerrainRanderPlugin;

impl Plugin for TerrainRanderPlugin {
    fn build(&self, app: &mut App) {
        todo!()
    }
}

#[derive(Component)]
pub struct TerrainHeightMap {
    pub half_length: f32,
    pub division_levels: u8,
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
            .extend_from_slice(&[o, o + 1, o + 2, o + 2, o + 1, o + 3]);
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
        let mut quads_per_side_at_level = self.division_levels * 2;
        let mut level_width = self.half_length * 2.0;
        let mut quad_width = level_width / quads_per_side_at_level as f32;
        let mut quad_depth = 1;
        let mut bottom_left = Vec3::new(-self.half_length, 0.0, -self.half_length);
        for level in 0..self.division_levels {
            // one full row
            for d in 0..quad_depth {
                for i in 0..quads_per_side_at_level {
                    m.add_quad(bottom_left + Vec3::X * i as f32 * quad_width, quad_width);
                }
                bottom_left += Vec3::new(0.0, 0.0, quad_width);
            }
            // TODO one full quad per side for the columns
            // TODO one full row for the top
            // prepare for next ring
            quad_width *= 0.5;
            quad_depth *= 2;
        }

        todo!()
    }

    /// division levels start on theouter most ring at 0 and go up to division_levels - 1 at the center
    fn quad_width_at_level(&self, division_level: u8) -> f32 {
        let mut level_width = self.half_length * 2.0;
        let mut quad_width = level_width / quads_per_side_at_level as f32;
        for _ in 1..division_level {
            level_width -= quad_width * 2.0;
            quad_width *= 0.5;
        }
        quad_width
    }

    fn center_distance(a: Vec3) -> f32 {
        let a = a.abs();
        a.x.max(a.y)
    }

    fn step_right(division_level: u8) -> Vec3 {
        todo!()
    }
}
