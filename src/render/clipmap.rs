use bevy::{
    asset::{AssetPath, RenderAssetUsages, embedded_asset, embedded_path},
    mesh::PrimitiveTopology,
    pbr::MaterialExtension,
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

#[derive(Component)]
pub struct FollowTerrainMarker;

#[derive(Component)]
pub struct TerrainMarker;

pub fn follow(
    following: Single<&Transform, (With<FollowTerrainMarker>, Without<TerrainMarker>)>,
    mut terrain: Query<&mut Transform, With<TerrainMarker>>,
) {
    for mut t in &mut terrain {
        t.translation.x = following.translation.x.floor();
        t.translation.z = following.translation.z.floor();
    }
}

#[derive(Component)]
pub struct TerrainHeightMapMesh {
    pub smallest_quad: f32,
    pub rings: u8,
    pub smallest_quad_count: u8,
}

struct QuadMeshBuilder {
    vertices: Vec<Vec3>,
    indices: Vec<u32>,
}

enum DirectionForTiple {
    Up,
    Down,
    Left,
    Right,
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

    fn add_triple_divided_quad(
        &mut self,
        bottom_left: Vec3,
        width: f32,
        direction_for_triple: DirectionForTiple,
    ) {
        match direction_for_triple {
            DirectionForTiple::Up => {
                let o = self.vertices.len() as u32;
                self.vertices.extend_from_slice(&[
                    bottom_left,
                    bottom_left.with_x(bottom_left.x + width),
                    bottom_left.with_z(bottom_left.z + width),
                    bottom_left
                        .with_x(bottom_left.x + width)
                        .with_z(bottom_left.z + width),
                    bottom_left
                        .with_x(bottom_left.x + width * 0.5)
                        .with_z(bottom_left.z + width),
                ]);
                self.indices.extend_from_slice(&[
                    o,
                    o + 2,
                    o + 4,
                    o,
                    o + 4,
                    o + 1,
                    o + 4,
                    o + 3,
                    o + 1,
                ]);
            }
            DirectionForTiple::Down => {
                let o = self.vertices.len() as u32;
                self.vertices.extend_from_slice(&[
                    bottom_left,
                    bottom_left.with_x(bottom_left.x + width),
                    bottom_left.with_z(bottom_left.z + width),
                    bottom_left
                        .with_x(bottom_left.x + width)
                        .with_z(bottom_left.z + width),
                    bottom_left.with_x(bottom_left.x + width * 0.5),
                ]);
                self.indices.extend_from_slice(&[
                    o,
                    o + 2,
                    o + 4,
                    o + 2,
                    o + 3,
                    o + 4,
                    o + 3,
                    o + 1,
                    o + 4,
                ]);
            }
            DirectionForTiple::Left => {
                let o = self.vertices.len() as u32;
                self.vertices.extend_from_slice(&[
                    bottom_left,
                    bottom_left.with_x(bottom_left.x + width),
                    bottom_left.with_z(bottom_left.z + width),
                    bottom_left
                        .with_x(bottom_left.x + width)
                        .with_z(bottom_left.z + width),
                    bottom_left
                        .with_x(bottom_left.x + width)
                        .with_z(bottom_left.z + width * 0.5),
                ]);
                self.indices.extend_from_slice(&[
                    o,
                    o + 2,
                    o + 4,
                    o,
                    o + 4,
                    o + 1,
                    o + 2,
                    o + 3,
                    o + 4,
                ]);
            }
            DirectionForTiple::Right => {
                let o = self.vertices.len() as u32;
                self.vertices.extend_from_slice(&[
                    bottom_left,
                    bottom_left.with_x(bottom_left.x + width),
                    bottom_left.with_z(bottom_left.z + width),
                    bottom_left
                        .with_x(bottom_left.x + width)
                        .with_z(bottom_left.z + width),
                    bottom_left.with_z(bottom_left.z + width * 0.5),
                ]);
                self.indices.extend_from_slice(&[
                    o,
                    o + 4,
                    o + 1,
                    o + 4,
                    o + 3,
                    o + 1,
                    o + 4,
                    o + 2,
                    o + 3,
                ]);
            }
        }
    }

    fn add_subdivided_quad(
        &mut self,
        bottom_left: Vec3,
        quad_width: f32,
        divisions: u8,
        direction_for_triple: Option<DirectionForTiple>,
    ) {
        match direction_for_triple {
            Some(DirectionForTiple::Up) => {
                for x in 0..divisions {
                    for y in 0..divisions - 1 {
                        let local_bottom_left = bottom_left
                            + Vec3::X * quad_width * x as f32
                            + Vec3::Z * quad_width * y as f32;
                        self.add_quad(local_bottom_left, quad_width);
                    }
                }
                for x in 0..divisions {
                    let y = divisions - 1;
                    let local_bottom_left = bottom_left
                        + Vec3::X * quad_width * x as f32
                        + Vec3::Z * quad_width * y as f32;
                    self.add_triple_divided_quad(
                        local_bottom_left,
                        quad_width,
                        DirectionForTiple::Up,
                    );
                }
            }
            Some(DirectionForTiple::Down) => {
                for x in 0..divisions {
                    for y in 1..divisions {
                        let local_bottom_left = bottom_left
                            + Vec3::X * quad_width * x as f32
                            + Vec3::Z * quad_width * y as f32;
                        self.add_quad(local_bottom_left, quad_width);
                    }
                }
                for x in 0..divisions {
                    let y = 0;
                    let local_bottom_left = bottom_left
                        + Vec3::X * quad_width * x as f32
                        + Vec3::Z * quad_width * y as f32;
                    self.add_triple_divided_quad(
                        local_bottom_left,
                        quad_width,
                        DirectionForTiple::Down,
                    );
                }
            }
            Some(DirectionForTiple::Right) => {
                for x in 0..divisions - 1 {
                    for y in 0..divisions {
                        let local_bottom_left = bottom_left
                            + Vec3::X * quad_width * x as f32
                            + Vec3::Z * quad_width * y as f32;
                        self.add_quad(local_bottom_left, quad_width);
                    }
                }
                for y in 0..divisions {
                    let x = divisions - 1;
                    let local_bottom_left = bottom_left
                        + Vec3::X * quad_width * x as f32
                        + Vec3::Z * quad_width * y as f32;
                    self.add_triple_divided_quad(
                        local_bottom_left,
                        quad_width,
                        DirectionForTiple::Left,
                    );
                }
            }
            Some(DirectionForTiple::Left) => {
                for x in 1..divisions {
                    for y in 0..divisions {
                        let local_bottom_left = bottom_left
                            + Vec3::X * quad_width * x as f32
                            + Vec3::Z * quad_width * y as f32;
                        self.add_quad(local_bottom_left, quad_width);
                    }
                }
                for y in 0..divisions {
                    let x = 0;
                    let local_bottom_left = bottom_left
                        + Vec3::X * quad_width * x as f32
                        + Vec3::Z * quad_width * y as f32;
                    self.add_triple_divided_quad(
                        local_bottom_left,
                        quad_width,
                        DirectionForTiple::Right,
                    );
                }
            }
            None => {
                for x in 0..divisions {
                    for y in 0..divisions {
                        let local_bottom_left = bottom_left
                            + Vec3::X * quad_width * x as f32
                            + Vec3::Z * quad_width * y as f32;
                        self.add_quad(local_bottom_left, quad_width);
                    }
                }
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
    pub fn create_base_mesh(&self) -> Mesh {
        assert!(self.smallest_quad_count % 4 == 0);
        let mut m = QuadMeshBuilder::empty();
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

        for _ in 0..self.rings {
            quad_size *= 2.0;
            bottom_left -= Vec3::new(
                quad_size * (self.smallest_quad_count / 4) as f32,
                0.0,
                quad_size * (self.smallest_quad_count / 4) as f32,
            );
            for (x, y, dir) in [
                (0.0, 0.0, None),
                (0.0, 1.0, Some(DirectionForTiple::Right)),
                (0.0, 2.0, Some(DirectionForTiple::Right)),
                (0.0, 3.0, None),
                (1.0, 0.0, Some(DirectionForTiple::Up)),
                (1.0, 3.0, Some(DirectionForTiple::Down)),
                (2.0, 0.0, Some(DirectionForTiple::Up)),
                (2.0, 3.0, Some(DirectionForTiple::Down)),
                (3.0, 0.0, None),
                (3.0, 1.0, Some(DirectionForTiple::Left)),
                (3.0, 2.0, Some(DirectionForTiple::Left)),
                (3.0, 3.0, None),
            ] {
                m.add_subdivided_quad(
                    bottom_left
                        + Vec3::new(
                            quad_size * x * (self.smallest_quad_count / 4) as f32,
                            0.0,
                            quad_size * y * (self.smallest_quad_count / 4) as f32,
                        ),
                    quad_size,
                    self.smallest_quad_count / 4,
                    dir,
                );
            }
        }
        debug!(bottom_left = ?bottom_left, "last bottom left");

        m.build()
    }
}

#[derive(Asset, AsBindGroup, Debug, Clone, Reflect)]
pub struct TerrainMaterial {
    #[texture(100)]
    #[sampler(101)]
    pub height: Handle<Image>,
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

    // fn specialize(
    //     _: &bevy::pbr::MaterialExtensionPipeline,
    //     descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
    //     _: &bevy::mesh::MeshVertexBufferLayoutRef,
    //     _key: bevy::pbr::MaterialExtensionKey<Self>,
    // ) -> std::result::Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
    //     descriptor.primitive.polygon_mode = bevy::render::render_resource::PolygonMode::Line;
    //     descriptor.depth_stencil.as_mut().unwrap().bias.slope_scale = 1.0;
    //     Ok(())
    // }
}
