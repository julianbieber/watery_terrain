use bevy::{
    asset::{AssetPath, RenderAssetUsages, embedded_path},
    mesh::PrimitiveTopology,
    pbr::MaterialExtension,
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

#[derive(Component)]
pub struct FollowTerrainMarker;

#[derive(Component)]
pub struct ClipmapMarker;

pub fn follow(
    following: Single<&Transform, (With<FollowTerrainMarker>, Without<ClipmapMarker>)>,
    mut terrain: Query<&mut Transform, With<ClipmapMarker>>,
) {
    for mut t in &mut terrain {
        t.translation.x = (following.translation.x / 10.0).floor() * 10.0;
        t.translation.z = (following.translation.z / 10.0).floor() * 10.0;
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
        let uvs = vec![Vec2::ZERO; self.vertices.len()];
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        let tangents = vec![Vec4::ZERO; self.vertices.len()];
        m.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
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

        m.build()
    }
}

#[derive(Asset, AsBindGroup, Debug, Clone, Reflect)]
pub struct WaterTerrainMaterial {
    #[texture(100)]
    #[sampler(101)]
    pub water: Handle<Image>,
    #[texture(102)]
    #[sampler(103)]
    pub base: Handle<Image>,
}

impl MaterialExtension for WaterTerrainMaterial {
    fn vertex_shader() -> bevy::shader::ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("water.wgsl")).with_source("embedded"),
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
            AssetPath::from_path_buf(embedded_path!("water.wgsl")).with_source("embedded"),
        )
    }

    fn deferred_vertex_shader() -> bevy::shader::ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("water.wgsl")).with_source("embedded"),
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

// ============================================
// Tiled Materials for 3x3 grid
// ============================================

/// Tiled terrain material - uses 9 heightmap textures for a 3x3 grid
#[derive(Asset, AsBindGroup, Debug, Clone, Reflect)]
pub struct TiledTerrainMaterial {
    // 9 heightmap textures arranged in 3x3 grid (row-major order)
    #[texture(100)] #[sampler(101)] pub height_00: Handle<Image>, // top-left
    #[texture(102)] #[sampler(103)] pub height_01: Handle<Image>, // top-center
    #[texture(104)] #[sampler(105)] pub height_02: Handle<Image>, // top-right
    #[texture(106)] #[sampler(107)] pub height_10: Handle<Image>, // middle-left
    #[texture(108)] #[sampler(109)] pub height_11: Handle<Image>, // center
    #[texture(110)] #[sampler(111)] pub height_12: Handle<Image>, // middle-right
    #[texture(112)] #[sampler(113)] pub height_20: Handle<Image>, // bottom-left
    #[texture(114)] #[sampler(115)] pub height_21: Handle<Image>, // bottom-center
    #[texture(116)] #[sampler(117)] pub height_22: Handle<Image>, // bottom-right
}

impl MaterialExtension for TiledTerrainMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("tiled_terrain.wgsl")).with_source("embedded"),
        )
    }

    fn enable_prepass() -> bool {
        true
    }

    fn enable_shadows() -> bool {
        true
    }

    fn prepass_vertex_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("tiled_terrain.wgsl")).with_source("embedded"),
        )
    }

    fn deferred_vertex_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("tiled_terrain.wgsl")).with_source("embedded"),
        )
    }
}

impl TiledTerrainMaterial {
    /// Get height texture for a grid position
    pub fn get_height_texture(&self, grid_x: i8, grid_z: i8) -> &Handle<Image> {
        // Convert grid position to index
        // grid_z: -1 (top), 0 (middle), 1 (bottom)
        // grid_x: -1 (left), 0 (center), 1 (right)
        let row = (grid_z + 1) as usize;
        let col = (grid_x + 1) as usize;
        let index = row * 3 + col;
        
        match index {
            0 => &self.height_00,
            1 => &self.height_01,
            2 => &self.height_02,
            3 => &self.height_10,
            4 => &self.height_11,
            5 => &self.height_12,
            6 => &self.height_20,
            7 => &self.height_21,
            8 => &self.height_22,
            _ => &self.height_11, // fallback to center
        }
    }
}

/// Tiled water terrain material - uses 9 water and 9 base height textures
#[derive(Asset, AsBindGroup, Debug, Clone, Reflect)]
pub struct TiledWaterTerrainMaterial {
    // Water textures (9 tiles)
    #[texture(100)] #[sampler(101)] pub water_00: Handle<Image>,
    #[texture(102)] #[sampler(103)] pub water_01: Handle<Image>,
    #[texture(104)] #[sampler(105)] pub water_02: Handle<Image>,
    #[texture(106)] #[sampler(107)] pub water_10: Handle<Image>,
    #[texture(108)] #[sampler(109)] pub water_11: Handle<Image>,
    #[texture(110)] #[sampler(111)] pub water_12: Handle<Image>,
    #[texture(112)] #[sampler(113)] pub water_20: Handle<Image>,
    #[texture(114)] #[sampler(115)] pub water_21: Handle<Image>,
    #[texture(116)] #[sampler(117)] pub water_22: Handle<Image>,
    
    // Base height textures (9 tiles)
    #[texture(118)] #[sampler(119)] pub base_00: Handle<Image>,
    #[texture(120)] #[sampler(121)] pub base_01: Handle<Image>,
    #[texture(122)] #[sampler(123)] pub base_02: Handle<Image>,
    #[texture(124)] #[sampler(125)] pub base_10: Handle<Image>,
    #[texture(126)] #[sampler(127)] pub base_11: Handle<Image>,
    #[texture(128)] #[sampler(129)] pub base_12: Handle<Image>,
    #[texture(130)] #[sampler(131)] pub base_20: Handle<Image>,
    #[texture(132)] #[sampler(133)] pub base_21: Handle<Image>,
    #[texture(134)] #[sampler(135)] pub base_22: Handle<Image>,
}

impl MaterialExtension for TiledWaterTerrainMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("tiled_water.wgsl")).with_source("embedded"),
        )
    }

    fn enable_prepass() -> bool {
        true
    }

    fn enable_shadows() -> bool {
        true
    }

    fn prepass_vertex_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("tiled_water.wgsl")).with_source("embedded"),
        )
    }

    fn deferred_vertex_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("tiled_water.wgsl")).with_source("embedded"),
        )
    }
}

impl TiledWaterTerrainMaterial {
    /// Get water and base textures for a grid position
    pub fn get_textures(&self, grid_x: i8, grid_z: i8) -> (Handle<Image>, Handle<Image>) {
        let row = (grid_z + 1) as usize;
        let col = (grid_x + 1) as usize;
        let index = row * 3 + col;
        
        let water = match index {
            0 => self.water_00.clone(),
            1 => self.water_01.clone(),
            2 => self.water_02.clone(),
            3 => self.water_10.clone(),
            4 => self.water_11.clone(),
            5 => self.water_12.clone(),
            6 => self.water_20.clone(),
            7 => self.water_21.clone(),
            8 => self.water_22.clone(),
            _ => self.water_11.clone(),
        };
        
        let base = match index {
            0 => self.base_00.clone(),
            1 => self.base_01.clone(),
            2 => self.base_02.clone(),
            3 => self.base_10.clone(),
            4 => self.base_11.clone(),
            5 => self.base_12.clone(),
            6 => self.base_20.clone(),
            7 => self.base_21.clone(),
            8 => self.base_22.clone(),
            _ => self.base_11.clone(),
        };
        
        (water, base)
    }
    
    /// Create a tiled material from a single water and base texture
    /// This replicates the same texture to all 9 tiles
    pub fn from_single(water: Handle<Image>, base: Handle<Image>) -> Self {
        Self {
            water_00: water.clone(), water_01: water.clone(), water_02: water.clone(),
            water_10: water.clone(), water_11: water.clone(), water_12: water.clone(),
            water_20: water.clone(), water_21: water.clone(), water_22: water,
            base_00: base.clone(), base_01: base.clone(), base_02: base.clone(),
            base_10: base.clone(), base_11: base.clone(), base_12: base.clone(),
            base_20: base.clone(), base_21: base.clone(), base_22: base,
        }
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
