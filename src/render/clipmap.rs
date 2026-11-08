//! The camera-following clipmap mesh the terrain and the water are drawn
//! with, and the materials that displace it onto their heightfields.

use bevy::{
    asset::{AssetPath, RenderAssetUsages, embedded_path},
    mesh::PrimitiveTopology,
    pbr::MaterialExtension,
    platform::collections::HashMap,
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

/// The entity the clipmaps and the chunk window follow. The systems that
/// follow it do nothing unless exactly one exists.
#[derive(Component)]
pub struct FollowTerrainMarker;

/// A clipmap mesh that [`follow`] keeps under the [`FollowTerrainMarker`].
#[derive(Component)]
pub struct ClipmapMarker;

/// Moves every [`ClipmapMarker`] entity to the [`FollowTerrainMarker`] entity on
/// x and z. The vertex shaders read this position as the viewer and snap the
/// mesh from it, so every render pass sees the same surface.
pub fn follow(
    following: Single<&Transform, (With<FollowTerrainMarker>, Without<ClipmapMarker>)>,
    mut terrain: Query<&mut Transform, With<ClipmapMarker>>,
) {
    for mut t in &mut terrain {
        t.translation.x = following.translation.x;
        t.translation.z = following.translation.z;
    }
}

/// What the vertex shaders need to know about a clipmap mesh's layout.
#[derive(ShaderType, Debug, Clone, Copy, Reflect)]
pub struct ClipmapShape {
    /// The grid in metres the mesh centre stays on: the size of its coarsest
    /// quad, so moving by one step maps every vertex onto a vertex's old place.
    pub snap_step: f32,
    /// A level's half width in metres divided by its quad size.
    pub half_quads: f32,
}

/// Shape of a clipmap mesh: a centre block of `smallest_quad_count` x
/// `smallest_quad_count` quads of side `smallest_quad` metres, surrounded by
/// `rings` rings, each with quads twice the size of the one inside it.
pub struct TerrainHeightMapMesh {
    pub smallest_quad: f32,
    pub rings: u8,
    pub smallest_quad_count: u8,
}

struct QuadMeshBuilder {
    grid: f32,
    vertex_at: HashMap<(i32, i32), u32>,
    vertices: Vec<Vec3>,
    quad_sizes: Vec<f32>,
    indices: Vec<u32>,
}

#[derive(Clone, Copy)]
enum DirectionForTriple {
    Up,
    Down,
    Left,
    Right,
}

impl QuadMeshBuilder {
    fn new(grid: f32) -> QuadMeshBuilder {
        QuadMeshBuilder {
            grid,
            vertex_at: HashMap::new(),
            vertices: Vec::new(),
            quad_sizes: Vec::new(),
            indices: Vec::new(),
        }
    }

    fn vertex(&mut self, position: Vec3, quad_size: f32) -> u32 {
        let key = (
            (position.x / self.grid).round() as i32,
            (position.z / self.grid).round() as i32,
        );
        *self.vertex_at.entry(key).or_insert_with(|| {
            self.vertices.push(position);
            self.quad_sizes.push(quad_size);
            self.vertices.len() as u32 - 1
        })
    }

    fn corners(&mut self, bottom_left: Vec3, width: f32) -> [u32; 4] {
        [
            self.vertex(bottom_left, width),
            self.vertex(bottom_left + Vec3::X * width, width),
            self.vertex(bottom_left + Vec3::Z * width, width),
            self.vertex(bottom_left + (Vec3::X + Vec3::Z) * width, width),
        ]
    }

    fn add_quad(&mut self, bottom_left: Vec3, width: f32) {
        let [a, b, c, d] = self.corners(bottom_left, width);
        self.indices.extend_from_slice(&[a, c, b, c, d, b]);
    }

    fn add_triple_divided_quad(
        &mut self,
        bottom_left: Vec3,
        width: f32,
        direction: DirectionForTriple,
    ) {
        let [a, b, c, d] = self.corners(bottom_left, width);
        let middle = match direction {
            DirectionForTriple::Up => Vec3::new(0.5, 0.0, 1.0),
            DirectionForTriple::Down => Vec3::new(0.5, 0.0, 0.0),
            DirectionForTriple::Left => Vec3::new(1.0, 0.0, 0.5),
            DirectionForTriple::Right => Vec3::new(0.0, 0.0, 0.5),
        };
        let m = self.vertex(bottom_left + middle * width, width * 0.5);
        self.indices.extend_from_slice(&match direction {
            DirectionForTriple::Up => [a, c, m, a, m, b, m, d, b],
            DirectionForTriple::Down => [a, c, m, c, d, m, d, b, m],
            DirectionForTriple::Left => [a, c, m, a, m, b, c, d, m],
            DirectionForTriple::Right => [a, m, b, m, d, b, m, c, d],
        });
    }

    fn add_subdivided_quad(
        &mut self,
        bottom_left: Vec3,
        quad_width: f32,
        divisions: u8,
        direction: Option<DirectionForTriple>,
    ) {
        let last = divisions - 1;
        for x in 0..divisions {
            for y in 0..divisions {
                let local_bottom_left =
                    bottom_left + Vec3::X * quad_width * x as f32 + Vec3::Z * quad_width * y as f32;
                let on_seam = match direction {
                    Some(DirectionForTriple::Up) => y == last,
                    Some(DirectionForTriple::Down) => y == 0,
                    Some(DirectionForTriple::Left) => x == last,
                    Some(DirectionForTriple::Right) => x == 0,
                    None => false,
                };
                match direction.filter(|_| on_seam) {
                    Some(direction) => {
                        self.add_triple_divided_quad(local_bottom_left, quad_width, direction)
                    }
                    None => self.add_quad(local_bottom_left, quad_width),
                }
            }
        }
    }

    fn build(&self) -> Mesh {
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.vertices.clone());
        let uvs: Vec<Vec2> = self.quad_sizes.iter().map(|q| Vec2::new(*q, 0.0)).collect();
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        let tangents = vec![Vec4::ZERO; self.vertices.len()];
        m.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
        m.insert_indices(bevy::mesh::Indices::U32(self.indices.clone()));

        m
    }
}

impl TerrainHeightMapMesh {
    pub fn shader_shape(&self) -> ClipmapShape {
        ClipmapShape {
            snap_step: self.smallest_quad * 2.0_f32.powi(self.rings as i32),
            half_quads: self.smallest_quad_count as f32 * 0.5,
        }
    }

    /// The flat clipmap mesh centred on the origin, every height zero for the
    /// vertex shaders to displace. Each vertex's first UV coordinate is the
    /// size of the quads of the finest level it belongs to.
    ///
    /// Panics unless `smallest_quad_count` is a multiple of 4 and at least
    /// `2^(rings + 2)`, the size at which a level's transition to the next
    /// fits between its neighbours wherever the viewer is within a snap step.
    pub fn create_base_mesh(&self) -> Mesh {
        self.builder().build()
    }

    fn builder(&self) -> QuadMeshBuilder {
        assert!(self.smallest_quad_count % 4 == 0);
        assert!(self.smallest_quad_count as u32 >= 1 << (self.rings as u32 + 2));
        let mut m = QuadMeshBuilder::new(self.smallest_quad * 0.5);
        let mut bottom_left = Vec3::new(
            -self.smallest_quad * self.smallest_quad_count as f32 * 0.5,
            0.0,
            -self.smallest_quad * self.smallest_quad_count as f32 * 0.5,
        );
        m.add_subdivided_quad(
            bottom_left,
            self.smallest_quad,
            self.smallest_quad_count,
            None,
        );
        let mut quad_size = self.smallest_quad;
        let base_divisions = self.smallest_quad_count / 4;

        for _ in 0..self.rings {
            quad_size *= 2.0;

            bottom_left -= Vec3::new(
                quad_size * base_divisions as f32,
                0.0,
                quad_size * base_divisions as f32,
            );
            for (x, y, dir) in [
                (0.0, 0.0, None),
                (0.0, 1.0, Some(DirectionForTriple::Left)),
                (0.0, 2.0, Some(DirectionForTriple::Left)),
                (0.0, 3.0, None),
                (1.0, 0.0, Some(DirectionForTriple::Up)),
                (1.0, 3.0, Some(DirectionForTriple::Down)),
                (2.0, 0.0, Some(DirectionForTriple::Up)),
                (2.0, 3.0, Some(DirectionForTriple::Down)),
                (3.0, 0.0, None),
                (3.0, 1.0, Some(DirectionForTriple::Right)),
                (3.0, 2.0, Some(DirectionForTriple::Right)),
                (3.0, 3.0, None),
            ] {
                m.add_subdivided_quad(
                    bottom_left
                        + Vec3::new(
                            quad_size * x * base_divisions as f32,
                            0.0,
                            quad_size * y * base_divisions as f32,
                        ),
                    quad_size,
                    self.smallest_quad_count / 4,
                    dir,
                );
            }
        }
        debug!(bottom_left = ?bottom_left, "last bottom left");

        m
    }
}

/// Displaces a clipmap onto the water surface. `water` is the depth texture
/// the simulation updates in place and `base` the terrain heights, both window
/// textures; `window_origin` is the window's lower chunk.
#[derive(Asset, AsBindGroup, Debug, Clone, Reflect)]
pub struct WaterTerrainMaterial {
    #[texture(100, visibility(vertex, fragment))]
    #[sampler(101, visibility(vertex, fragment))]
    pub water: Handle<Image>,
    #[texture(102, visibility(vertex, fragment))]
    #[sampler(103, visibility(vertex, fragment))]
    pub base: Handle<Image>,
    #[uniform(104, visibility(vertex, fragment))]
    pub window_origin: UVec2,
    #[uniform(105, visibility(vertex, fragment))]
    pub shape: ClipmapShape,
}

impl MaterialExtension for WaterTerrainMaterial {
    fn vertex_shader() -> bevy::shader::ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("water_forward.wesl")).with_source("embedded"),
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
            AssetPath::from_path_buf(embedded_path!("water_prepass.wesl")).with_source("embedded"),
        )
    }

    fn deferred_vertex_shader() -> bevy::shader::ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("water_prepass.wesl")).with_source("embedded"),
        )
    }
}

/// Displaces a clipmap onto the terrain. `height` is the terrain's window
/// texture and `window_origin` the window's lower chunk.
#[derive(Asset, AsBindGroup, Debug, Clone, Reflect)]
pub struct TerrainMaterial {
    #[texture(100, visibility(vertex, fragment))]
    #[sampler(101, visibility(vertex, fragment))]
    pub height: Handle<Image>,
    #[uniform(104, visibility(vertex, fragment))]
    pub window_origin: UVec2,
    #[uniform(105, visibility(vertex, fragment))]
    pub shape: ClipmapShape,
}

impl MaterialExtension for TerrainMaterial {
    fn vertex_shader() -> bevy::shader::ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("terrain_forward.wesl"))
                .with_source("embedded"),
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
            AssetPath::from_path_buf(embedded_path!("terrain_prepass.wesl"))
                .with_source("embedded"),
        )
    }

    fn deferred_vertex_shader() -> bevy::shader::ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("terrain_prepass.wesl"))
                .with_source("embedded"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape() -> TerrainHeightMapMesh {
        TerrainHeightMapMesh {
            smallest_quad: 0.05,
            rings: 5,
            smallest_quad_count: 160,
        }
    }

    #[test]
    fn snap_step_is_the_coarsest_quad() {
        assert!((shape().shader_shape().snap_step - 1.6).abs() < 1e-5);
    }

    #[test]
    fn snapping_by_one_step_keeps_every_vertex_on_a_vertex() {
        let builder = shape().builder();
        let step = shape().shader_shape().snap_step;
        let half = shape().smallest_quad * shape().smallest_quad_count as f32 * 0.5;
        for vertex in builder.vertices.iter().zip(&builder.quad_sizes) {
            let (position, quad) = vertex;
            let shifted = *position + Vec3::X * step;
            if shifted.x.abs().max(shifted.z.abs()) > half {
                continue;
            }
            let index = shifted.x / quad;
            assert!((index - index.round()).abs() < 1e-2, "{shifted} off {quad}");
        }
    }

    #[test]
    fn only_the_outer_perimeter_is_open() {
        let mesh = shape();
        let builder = mesh.builder();
        let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
        for triangle in builder.indices.as_chunks::<3>().0 {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let outer = mesh.smallest_quad * mesh.shader_shape().half_quads * 2.0_f32.powi(5);
        for ((a, b), count) in edges {
            assert!(count <= 2);
            if count == 1 {
                for vertex in [a, b] {
                    let p = builder.vertices[vertex as usize];
                    assert!(
                        (p.x.abs().max(p.z.abs()) - outer).abs() < 1e-2,
                        "open at {p}"
                    );
                }
            }
        }
    }

    #[test]
    fn triangles_face_up() {
        let builder = shape().builder();
        for triangle in builder.indices.as_chunks::<3>().0 {
            let [a, b, c] = [0, 1, 2].map(|i| builder.vertices[triangle[i] as usize]);
            assert!((b - a).cross(c - a).y > 0.0);
        }
    }

    #[test]
    #[should_panic]
    fn refuses_a_centre_too_small_for_the_transitions() {
        let mesh = TerrainHeightMapMesh {
            smallest_quad: 1.0,
            rings: 6,
            smallest_quad_count: 160,
        };
        mesh.create_base_mesh();
    }
}
