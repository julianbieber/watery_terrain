use std::borrow::Cow;

use bevy::{
    asset::{AssetPath, RenderAssetUsages, embedded_asset, embedded_path},
    mesh::PrimitiveTopology,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_graph::{self, RenderGraph, RenderLabel},
        render_resource::{
            AsBindGroup, BindGroup, BindGroupEntries, BindGroupLayoutDescriptor,
            BindGroupLayoutEntries, CachedComputePipelineId, ComputePassDescriptor,
            ComputePipelineDescriptor, PipelineCache, ShaderStages,
            binding_types::texture_storage_2d,
        },
        renderer::RenderDevice,
        texture::GpuImage,
    },
    shader::ShaderRef,
};

use crate::{heightmap::create_heightmap, screens::Screen};

pub struct TerrainRanderPlugin;

impl Plugin for TerrainRanderPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "terrain.wgsl");
        embedded_asset!(app, "water.wgsl");

        app.add_plugins(MaterialPlugin::<
            ExtendedMaterial<StandardMaterial, TerrainMaterial>,
        >::default());

        let render_app = app.sub_app_mut(RenderApp);
        render_app.add_systems(RenderStartup, init_water_render);
        render_app.add_systems(Render, prepare_water_bindgroups);
        render_app.insert_resource(WaterBindGroupsSwap(true));

        let mut render_graph = render_app.world_mut().resource_mut::<RenderGraph>();
        render_graph.add_node(WaterRenderLabel, WaterRenderNode);
        render_graph.add_node_edge(WaterRenderLabel, bevy::render::graph::CameraDriverLabel);

        app.add_plugins(ExtractResourcePlugin::<WaterHeightTexture>::default());
        app.add_systems(OnEnter(Screen::Gameplay), spawn_plane_dbg);
        app.add_systems(Update, follow.run_if(in_state(Screen::Gameplay)));
        app.add_systems(Update, swap_textures.run_if(in_state(Screen::Gameplay)));
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct WaterRenderLabel;

#[derive(Resource)]
struct WaterRenderPipeline {
    layout: BindGroupLayoutDescriptor,
    pipeline: CachedComputePipelineId,
}

#[derive(Resource)]
struct WaterBindGroups([BindGroup; 2]);

#[derive(Resource)]
struct WaterBindGroupsSwap(bool);

struct WaterRenderNode;
impl bevy::render::render_graph::Node for WaterRenderNode {
    fn run<'w>(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        render_context: &mut bevy::render::renderer::RenderContext<'w>,
        world: &'w World,
    ) -> std::result::Result<(), render_graph::NodeRunError> {
        if let Some(bind_groups) = world.get_resource::<WaterBindGroups>() {
            let pipeline_cache = world.resource::<PipelineCache>();
            let pipeline = world.resource::<WaterRenderPipeline>();
            let swap = world.resource::<WaterBindGroupsSwap>();

            let mut pass = render_context
                .command_encoder()
                .begin_compute_pass(&ComputePassDescriptor::default());

            let update_pipeline = pipeline_cache
                .get_compute_pipeline(pipeline.pipeline)
                .unwrap();
            pass.set_bind_group(0, &bind_groups.0[swap.0 as usize], &[]);
            pass.set_pipeline(&update_pipeline);
            pass.dispatch_workgroups(2048 / 8, 2048 / 8, 1);
        }
        Ok(())
    }

    fn update(&mut self, world: &mut World) {
        let mut s = world.resource_mut::<WaterBindGroupsSwap>();
        s.0 = !s.0;
    }
}

fn init_water_render(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    pipeline_cache: Res<PipelineCache>,
) {
    let texture_bind_group_layout = BindGroupLayoutDescriptor::new(
        "WaterUpdate",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                texture_storage_2d(
                    bevy::render::render_resource::TextureFormat::Rgba32Float,
                    bevy::render::render_resource::StorageTextureAccess::ReadOnly,
                ),
                texture_storage_2d(
                    bevy::render::render_resource::TextureFormat::Rgba32Float,
                    bevy::render::render_resource::StorageTextureAccess::WriteOnly,
                ),
            ),
        ),
    );
    let shader: Handle<Shader> = asset_server
        .load(AssetPath::from_path_buf(embedded_path!("water.wgsl")).with_source("embedded"));
    let update_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        layout: vec![texture_bind_group_layout.clone()],
        shader,
        entry_point: Some(Cow::from("main")),
        ..Default::default()
    });
    commands.insert_resource(WaterRenderPipeline {
        layout: texture_bind_group_layout,
        pipeline: update_pipeline,
    });
}

fn prepare_water_bindgroups(
    mut commands: Commands,
    pipeline: Res<WaterRenderPipeline>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    water_images: If<Res<WaterHeightTexture>>,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
) {
    let tex_a = gpu_images.get(&water_images.texture_a).unwrap();
    let tex_b = gpu_images.get(&water_images.texture_b).unwrap();

    let bind_group_0 = render_device.create_bind_group(
        None,
        &pipeline_cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((&tex_a.texture_view, &tex_b.texture_view)),
    );
    let bind_group_1 = render_device.create_bind_group(
        None,
        &pipeline_cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((&tex_b.texture_view, &tex_a.texture_view)),
    );
    commands.insert_resource(WaterBindGroups([bind_group_0, bind_group_1]));
}

#[derive(Component)]
pub struct FollowTerrainMarker;

#[derive(Component)]
struct TerrainMarker;

#[derive(Resource, Clone, ExtractResource)]
struct WaterHeightTexture {
    texture_a: Handle<Image>,
    texture_b: Handle<Image>,
}

fn swap_textures(
    textures: ResMut<WaterHeightTexture>,
    water: Single<&MeshMaterial3d<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
) {
    let a = materials.get_mut(water.0.id()).unwrap();
    if a.extension.height == textures.texture_a {
        a.extension.height = textures.texture_b.clone();
    } else {
        a.extension.height = textures.texture_a.clone();
    }
}

fn spawn_plane_dbg(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
    mut images: ResMut<Assets<Image>>,
) {
    let terrain = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 5,
        smallest_quad_count: 16 * 10,
    };

    let heightmap = create_heightmap();
    let mesh = terrain.create_base_mesh();
    let heightmap_texture = images.add(heightmap.image());
    let heightmap_texture_b = images.add(heightmap.image());
    commands.insert_resource(WaterHeightTexture {
        texture_a: heightmap_texture.clone(),
        texture_b: heightmap_texture_b,
    });
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        TerrainMarker,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(ExtendedMaterial {
            base: StandardMaterial {
                base_color: Color::Srgba(Srgba::GREEN),
                unlit: false,
                ..Default::default()
            },
            extension: TerrainMaterial {
                height: heightmap_texture.clone(),
            },
        })),
        heightmap,
    ));
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            intensity: 40000.0,
            range: 2000.0,
            color: Color::Srgba(Srgba::BLUE),
            ..default()
        },
        Transform::from_xyz(4.0, 220.0, 4.0),
    ));
}

fn follow(
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
    fn create_base_mesh(&self) -> Mesh {
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
