//! The terrain and water of `watery_terrain` ray-marched over the window's
//! heightfields in the fragment shader, lit by Bevy's PBR from the host's
//! materials and sorted against meshes through the depth buffer. Add
//! [`SdfPlugin`] beside `WateryTerrainPlugin` and queue [`SpawnSdfSurfaces`]
//! after `SpawnWateryTerrain`.
//!
//! The water is see-through only for cameras with a `DepthPrepass`; for any
//! other camera it is opaque.

mod pyramid;

use bevy::{
    asset::{AssetPath, RenderAssetUsages, embedded_asset, embedded_path},
    camera::visibility::NoFrustumCulling,
    light::NotShadowCaster,
    material::OpaqueRendererMethod,
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline},
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    },
    shader::{ShaderRef, load_shader_library},
};
use watery_terrain::{
    WateryTerrainSystems,
    render::{ChunkLayout, TerrainScoped, WindowHeights, shift_window},
};

use crate::pyramid::{PyramidPlugin, Pyramids};

pub struct SdfPlugin;

impl Plugin for SdfPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "sdf.wesl");
        embedded_asset!(app, "sdf_forward.wesl");
        embedded_asset!(app, "sdf_prepass.wesl");
        app.add_plugins((MaterialPlugin::<SdfMaterial>::default(), PyramidPlugin));
        app.init_resource::<SdfShape>();
        app.add_systems(
            Update,
            follow_window
                .after(shift_window)
                .run_if(
                    resource_exists_and_changed::<ChunkLayout>
                        .or_eager(resource_exists_and_changed::<SdfShape>),
                )
                .in_set(WateryTerrainSystems),
        );
        app.add_systems(
            PostUpdate,
            drop_pyramids.run_if(resource_removed::<WindowHeights>),
        );
    }
}

/// Spawns the ray-marched water and land, looking like `water` and `land`,
/// over the window `SpawnWateryTerrain` brought up; queue it after that
/// command. The heightfields supply their shape and [`SdfShape`] its style.
pub struct SpawnSdfSurfaces {
    pub water: StandardMaterial,
    pub land: StandardMaterial,
}

impl Command for SpawnSdfSurfaces {
    type Out = ();

    fn apply(self, world: &mut World) {
        world
            .run_system_cached_with(spawn_surfaces, self)
            .expect("spawning the ray-marched surfaces");
    }
}

/// The shape the ray-marched terrain and water are drawn with. Changing it
/// takes effect on the next frame.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SdfShape {
    /// The bilinear surface through the heightmap's samples.
    #[default]
    Smooth,
    /// Boxes `2^level` cells wide, each as high as the highest point of the
    /// smooth surface under it; levels above 10 draw 10, a box per chunk.
    /// Colliders keep the smooth surface, so objects rest up to a box's step
    /// off the drawn one.
    Voxels { level: u8 },
}

impl SdfShape {
    fn voxel_level(self) -> u32 {
        match self {
            SdfShape::Smooth => u32::MAX,
            SdfShape::Voxels { level } => level.into(),
        }
    }
}

type SdfMaterial = ExtendedMaterial<StandardMaterial, SdfSurface>;

const LAND: u32 = 0;

const WATER: u32 = 1;

#[derive(ShaderType, Debug, Clone, Copy, Reflect)]
struct SdfParams {
    window_origin: UVec2,
    centre: UVec2,
    surface: u32,
    voxel_level: u32,
}

#[derive(Asset, AsBindGroup, Debug, Clone, Reflect)]
struct SdfSurface {
    #[texture(100, sample_type = "float", filterable = false, visibility(fragment))]
    ground: Handle<Image>,
    #[texture(101, sample_type = "float", filterable = false, visibility(fragment))]
    terrain_max: Handle<Image>,
    #[texture(102, sample_type = "float", filterable = false, visibility(fragment))]
    water: Handle<Image>,
    #[texture(103, sample_type = "float", filterable = false, visibility(fragment))]
    coarse_water: Handle<Image>,
    #[texture(104, sample_type = "float", filterable = false, visibility(fragment))]
    coarse_ground: Handle<Image>,
    #[texture(105, sample_type = "float", filterable = false, visibility(fragment))]
    water_max: Handle<Image>,
    #[texture(106, sample_type = "float", filterable = false, visibility(fragment))]
    centre_water_max: Handle<Image>,
    #[uniform(107, visibility(fragment))]
    params: SdfParams,
}

fn embedded_shader(path: AssetPath<'static>) -> ShaderRef {
    ShaderRef::Path(path.with_source("embedded"))
}

impl MaterialExtension for SdfSurface {
    fn vertex_shader() -> ShaderRef {
        embedded_shader(AssetPath::from_path_buf(embedded_path!("sdf_forward.wesl")))
    }

    fn fragment_shader() -> ShaderRef {
        embedded_shader(AssetPath::from_path_buf(embedded_path!("sdf_forward.wesl")))
    }

    fn prepass_vertex_shader() -> ShaderRef {
        embedded_shader(AssetPath::from_path_buf(embedded_path!("sdf_prepass.wesl")))
    }

    fn prepass_fragment_shader() -> ShaderRef {
        embedded_shader(AssetPath::from_path_buf(embedded_path!("sdf_prepass.wesl")))
    }

    fn enable_prepass() -> bool {
        true
    }

    fn enable_shadows() -> bool {
        true
    }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

fn proxy_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[-1.0, -1.0, 0.0], [3.0, -1.0, 0.0], [-1.0, 3.0, 0.0]],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0, 0.0, 0.0, 1.0]; 3]);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
    mesh
}

fn spawn_surfaces(
    In(surfaces): In<SpawnSdfSurfaces>,
    mut commands: Commands,
    heights: Res<WindowHeights>,
    layout: Res<ChunkLayout>,
    shape: Res<SdfShape>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SdfMaterial>>,
) {
    let pyramids = Pyramids::new(&mut images);
    let mesh = meshes.add(proxy_mesh());
    let mut surface = |base: StandardMaterial, kind: u32| {
        MeshMaterial3d(materials.add(ExtendedMaterial {
            base,
            extension: SdfSurface {
                ground: heights.ground.clone(),
                terrain_max: pyramids.terrain.clone(),
                water: heights.water.clone(),
                coarse_water: heights.coarse_water.clone(),
                coarse_ground: heights.coarse_ground.clone(),
                water_max: pyramids.water.clone(),
                centre_water_max: pyramids.centre_water.clone(),
                params: SdfParams {
                    window_origin: layout.origin_uvec(),
                    centre: layout.centre_uvec(),
                    surface: kind,
                    voxel_level: shape.voxel_level(),
                },
            },
        }))
    };
    let land = surface(
        StandardMaterial {
            alpha_mode: AlphaMode::Mask(0.5),
            opaque_render_method: OpaqueRendererMethod::Forward,
            ..surfaces.land
        },
        LAND,
    );
    let water = surface(
        StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            ..surfaces.water
        },
        WATER,
    );
    commands.spawn((TerrainScoped, NoFrustumCulling, Mesh3d(mesh.clone()), land));
    commands.spawn((
        TerrainScoped,
        NoFrustumCulling,
        NotShadowCaster,
        Mesh3d(mesh),
        water,
    ));
    commands.insert_resource(pyramids);
}

fn follow_window(
    layout: If<Res<ChunkLayout>>,
    shape: Res<SdfShape>,
    surfaces: Query<&MeshMaterial3d<SdfMaterial>>,
    mut materials: ResMut<Assets<SdfMaterial>>,
) {
    for surface in &surfaces {
        if let Some(mut material) = materials.get_mut(surface.id()) {
            let params = &mut material.extension.params;
            params.window_origin = layout.origin_uvec();
            params.centre = layout.centre_uvec();
            params.voxel_level = shape.voxel_level();
        }
    }
}

fn drop_pyramids(mut commands: Commands) {
    commands.remove_resource::<Pyramids>();
}
