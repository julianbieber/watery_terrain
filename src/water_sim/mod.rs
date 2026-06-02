use avian3d::prelude::LinearVelocity;
use bevy::{
    asset::{AssetPath, RenderAssetUsages, embedded_asset, embedded_path},
    ecs::system::If,
    pbr::ExtendedMaterial,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_graph::{self, RenderGraph, RenderLabel},
        render_resource::{
            BindGroup, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            CachedComputePipelineId, ComputePassDescriptor, ComputePipelineDescriptor, Extent3d,
            PipelineCache, PushConstantRange, ShaderStages, StorageBuffer, TextureDimension,
            TextureUsages,
            binding_types::{storage_buffer_read_only, texture_storage_2d},
        },
        renderer::{RenderDevice, RenderQueue},
        texture::GpuImage,
    },
};
use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;

use crate::{
    heightmap::{TileLayout, TilePosition, TILE_DIM},
    render::clipmap::WaterTerrainMaterial,
    screens::Screen,
};

pub struct WaterSimPlugin;

#[derive(Component)]
pub struct WaterMarker;

impl Plugin for WaterSimPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "water.wgsl");
        // Support both old and new systems for migration
        app.add_plugins(ExtractResourcePlugin::<WaterHeightTexture>::default());
        app.add_plugins(ExtractResourcePlugin::<TiledWaterTextures>::default());
        app.add_plugins(ExtractResourcePlugin::<DisplacementBufferMain>::default());
        app.insert_resource(DisplacementBufferMain { buffer: Vec::new() });
        
        // Use the tiled collect_displacements function
        app.add_systems(
            Update,
            collect_displacements_tiled.run_if(in_state(Screen::Gameplay)),
        );
        
        let render_app = app.sub_app_mut(RenderApp);
        render_app.add_systems(RenderStartup, init_water_render);
        render_app.add_systems(Render, prepare_tiled_water_bindgroups);
        render_app.insert_resource(WaterBindGroupsSwap(true));
        let displacements = StorageBuffer::<Vec<Vec4>>::from(Vec::new());
        render_app.insert_resource(DisplacementBuffer {
            buffer: displacements,
        });

        let mut render_graph = render_app.world_mut().resource_mut::<RenderGraph>();
        render_graph.add_node(WaterRenderLabel, WaterRenderNode);
        render_graph.add_node_edge(WaterRenderLabel, bevy::render::graph::CameraDriverLabel);
        
        // Observer for tiled textures initialization
        app.add_observer(init_internal_tiled_textures);
    }
}

#[derive(Component)]
pub struct WaterDisplacement {
    pub radius: f32,
    pub strength: f32,
}

/// Old single-texture version (kept for reference)
#[allow(dead_code)]
fn collect_displacements(
    mut d: Query<(&mut LinearVelocity, &Transform, &WaterDisplacement)>,
    mut buffer: ResMut<DisplacementBufferMain>,
    water_height: Res<WaterHeightTexture>,
    textures: Res<Assets<Image>>,
) {
    let water = textures.get(water_height.texture_a.id()).unwrap();
    let base = textures.get(water_height.base_height.id()).unwrap();
    buffer.buffer.clear();
    for (mut velocity, transform, w) in &mut d {
        let h = height_from_texture(water, transform.translation.xz());
        let base_h = height_from_texture(base, transform.translation.xz());
        let t_h = transform.translation.y;
        let relative_to_water = (t_h - w.radius) - base_h;
        if relative_to_water < h && h > 0.0 {
            let in_water = (h - relative_to_water).min(w.radius * 2.0) / (w.radius * 2.0);
            buffer.buffer.push(Vec4::new(
                transform.translation.x,
                transform.translation.z,
                w.radius,
                w.strength * in_water,
            ));
            velocity.0.y = in_water * 0.8;
        }
    }
}

/// New tiled version - uses TiledWaterTextures
fn collect_displacements_tiled(
    mut d: Query<(&mut LinearVelocity, &Transform, &WaterDisplacement)>,
    mut buffer: ResMut<DisplacementBufferMain>,
    water_height: Res<TiledWaterTextures>,
    textures: Res<Assets<Image>>,
) {
    buffer.buffer.clear();
    for (mut velocity, transform, w) in &mut d {
        let world_pos = transform.translation.xz();
        
        // Get the appropriate tile for this world position
        if let Some(tile_textures) = water_height.get_textures_at_world(world_pos) {
            let water = textures.get(tile_textures.texture_a.id()).unwrap();
            let base = textures.get(tile_textures.base_height.id()).unwrap();
            
            // Calculate tile-local UV
            let pos = TilePosition::from_world(world_pos);
            let (min, _) = water_height.get_world_bounds(pos);
            let local_uv = Vec2::new(
                (world_pos.x - min.x) * 10.0,
                (world_pos.y - min.y) * 10.0,
            );
            
            let h = height_from_tiled_texture(water, local_uv);
            let base_h = height_from_tiled_texture(base, local_uv);
            let t_h = transform.translation.y;
            let relative_to_water = (t_h - w.radius) - base_h;
            
            if relative_to_water < h && h > 0.0 {
                let in_water = (h - relative_to_water).min(w.radius * 2.0) / (w.radius * 2.0);
                buffer.buffer.push(Vec4::new(
                    transform.translation.x,
                    transform.translation.z,
                    w.radius,
                    w.strength * in_water,
                ));
                velocity.0.y = in_water * 0.8;
            }
        }
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
        // Try tiled version first
        if let Some(tile_bind_groups) = world.get_resource::<WaterTileBindGroups>() {
            let pipeline_cache = world.resource::<PipelineCache>();
            let pipeline = world.resource::<WaterRenderPipeline>();
            let swap = world.resource::<WaterBindGroupsSwap>();

            let mut pass = render_context
                .command_encoder()
                .begin_compute_pass(&ComputePassDescriptor::default());

            let update_pipeline = pipeline_cache
                .get_compute_pipeline(pipeline.pipeline)
                .unwrap();
            pass.set_pipeline(update_pipeline);

            // Process each tile
            for (tile_idx, bind_group_pair) in tile_bind_groups.bindgroups.iter().enumerate() {
                // Pass 0: update flows
                pass.set_bind_group(0, &bind_group_pair[swap.0 as usize], &[]);
                pass.set_push_constants(
                    0,
                    bytemuck::bytes_of(&SimParams {
                        id: 0,
                        tile_index: tile_idx as i32,
                        _pad: Vec2::ZERO,
                    }),
                );
                pass.dispatch_workgroups(TILE_DIM / 8, TILE_DIM / 8, 1);

                // Pass 1: update water height
                pass.set_push_constants(
                    0,
                    bytemuck::bytes_of(&SimParams {
                        id: 1,
                        tile_index: tile_idx as i32,
                        _pad: Vec2::ZERO,
                    }),
                );
                pass.dispatch_workgroups(TILE_DIM / 8, TILE_DIM / 8, 1);
            }
        }
        // Fallback to old single-texture version
        else if let Some(bind_groups) = world.get_resource::<WaterBindGroups>() {
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
            pass.set_pipeline(update_pipeline);

            pass.set_push_constants(
                0,
                bytemuck::bytes_of(&SimParams {
                    id: 0,
                    tile_index: 0,
                    _pad: Vec2::ZERO,
                }),
            );
            pass.dispatch_workgroups(2048 / 8, 2048 / 8, 1);

            pass.set_push_constants(
                0,
                bytemuck::bytes_of(&SimParams {
                    id: 1,
                    tile_index: 0,
                    _pad: Vec2::ZERO,
                }),
            );
            pass.dispatch_workgroups(2048 / 8, 2048 / 8, 1);
        }
        Ok(())
    }

    fn update(&mut self, world: &mut World) {
        let mut s = world.resource_mut::<WaterBindGroupsSwap>();
        s.0 = !s.0;
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct SimParams {
    id: i32,
    tile_index: i32,
    _pad: Vec2,
}

// ============================================
// Tiled Water Texture Support
// ============================================

/// Textures for a single water simulation tile
#[derive(Clone)]
pub struct WaterTileTextures {
    pub texture_a: Handle<Image>,
    pub texture_b: Handle<Image>,
    pub flow_x: Handle<Image>,
    pub flow_y: Handle<Image>,
    pub base_height: Handle<Image>,
}

/// Tiled water textures resource - manages 9 tiles of water simulation
#[derive(Resource, Clone, ExtractResource)]
pub struct TiledWaterTextures {
    /// Textures for each of the 9 tiles
    pub tiles: Vec<WaterTileTextures>,
    pub layout: TileLayout,
    pub tile_size: u32,
}

impl Default for TiledWaterTextures {
    fn default() -> Self {
        Self {
            tiles: Vec::new(),
            layout: TileLayout::default(),
            tile_size: TILE_DIM,
        }
    }
}

impl TiledWaterTextures {
    /// Create a new tiled water textures resource
    pub fn new(tiles: Vec<WaterTileTextures>, layout: TileLayout) -> Self {
        assert_eq!(tiles.len(), 9, "Expected exactly 9 tiles");
        Self {
            tiles,
            layout,
            tile_size: TILE_DIM,
        }
    }

    /// Get the textures for a specific grid position
    pub fn get_tile_textures(&self, pos: TilePosition) -> Option<&WaterTileTextures> {
        self.layout.get_tile_index(pos).and_then(|idx| self.tiles.get(idx))
    }

    /// Get textures for a world position
    pub fn get_textures_at_world(&self, world_pos: Vec2) -> Option<&WaterTileTextures> {
        let pos = TilePosition::from_world(world_pos);
        self.get_tile_textures(pos)
    }

    /// Get world bounds for a grid position
    pub fn get_world_bounds(&self, pos: TilePosition) -> (Vec2, Vec2) {
        self.layout.get_world_bounds(pos)
    }
}

/// Resource holding bind groups for all tile textures (ping-pong buffers)
#[derive(Resource)]
pub struct WaterTileBindGroups {
    pub bindgroups: Vec<[BindGroup; 2]>,
}

/// Old single-texture version (kept for reference)
#[allow(dead_code)]
fn init_internal_textures(
    trigger: On<
        Add,
        (
            // MeshMaterial3d<ExtendedMaterial<StandardMaterial, TerrainMaterial>>,
            WaterMarker,
        ),
    >,
    mut commands: Commands,
    material: Query<&MeshMaterial3d<ExtendedMaterial<StandardMaterial, WaterTerrainMaterial>>>,
    water_terrain_materials: Res<Assets<ExtendedMaterial<StandardMaterial, WaterTerrainMaterial>>>,
    mut images: ResMut<Assets<Image>>,
) {
    info!("adding internal water tex");
    let material = material.get(trigger.entity).unwrap();
    let material = water_terrain_materials.get(material.0.id()).unwrap();
    let water_height = images.get(material.extension.water.id()).unwrap();
    let water_height_2 = water_height.clone();
    let mut flow_x = Image::new(
        Extent3d {
            width: water_height.width() + 1,
            height: water_height.width(),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        (0..((water_height.width() + 1) * water_height.height()))
            .flat_map(|_| 0.0f32.to_le_bytes())
            .collect(),
        bevy::render::render_resource::TextureFormat::R32Float,
        RenderAssetUsages::all(),
    );
    flow_x.texture_descriptor.usage |= TextureUsages::STORAGE_BINDING | TextureUsages::COPY_DST;
    let mut flow_y = Image::new(
        Extent3d {
            width: water_height.width(),
            height: water_height.width() + 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        (0..(water_height.width() * (water_height.height() + 1)))
            .flat_map(|_| 0.0f32.to_le_bytes())
            .collect(),
        bevy::render::render_resource::TextureFormat::R32Float,
        RenderAssetUsages::all(),
    );
    flow_y.texture_descriptor.usage |= TextureUsages::STORAGE_BINDING | TextureUsages::COPY_DST;
    let flow_y = images.add(flow_y);
    let flow_x = images.add(flow_x);
    let water_height_2 = images.add(water_height_2);

    commands.insert_resource(WaterHeightTexture {
        texture_a: material.extension.water.clone(),
        texture_b: water_height_2,
        flow_x,
        flow_y,
        base_height: material.extension.base.clone(),
    });
}

/// New tiled version - creates TiledWaterTextures from tiled materials
fn init_internal_tiled_textures(
    trigger: On<
        Add,
        (
            WaterMarker,
        ),
    >,
    mut commands: Commands,
    material: Query<&MeshMaterial3d<ExtendedMaterial<StandardMaterial, WaterTerrainMaterial>>>,
    water_terrain_materials: Res<Assets<ExtendedMaterial<StandardMaterial, WaterTerrainMaterial>>>,
    mut images: ResMut<Assets<Image>>,
) {
    info!("adding internal tiled water textures");
    let material = material.get(trigger.entity).unwrap();
    let material = water_terrain_materials.get(material.0.id()).unwrap();
    
    // Clone the source textures to avoid borrow checker issues
    let source_water = images.get(material.extension.water.id()).unwrap().clone();
    let source_base = images.get(material.extension.base.id()).unwrap().clone();
    
    let mut tile_textures = Vec::with_capacity(9);
    let layout = TileLayout::default();
    
    // Create 9 tiles, each with their own water and base textures
    // For now, all tiles use the same source texture, but this will be updated
    // when we have proper tiled materials
    for _ in 0..9 {
        // Create water textures (a and b for ping-pong)
        let water_a_handle = images.add(source_water.clone());
        let water_b_handle = images.add(source_water.clone());
        
        // Create flow textures
        let mut flow_x = Image::new(
            Extent3d {
                width: TILE_DIM + 1,
                height: TILE_DIM,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            (0..((TILE_DIM + 1) * TILE_DIM))
                .flat_map(|_| 0.0f32.to_le_bytes())
                .collect(),
            bevy::render::render_resource::TextureFormat::R32Float,
            RenderAssetUsages::all(),
        );
        flow_x.texture_descriptor.usage |= TextureUsages::STORAGE_BINDING | TextureUsages::COPY_DST;
        let flow_x_handle = images.add(flow_x);
        
        let mut flow_y = Image::new(
            Extent3d {
                width: TILE_DIM,
                height: TILE_DIM + 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            (0..(TILE_DIM * (TILE_DIM + 1)))
                .flat_map(|_| 0.0f32.to_le_bytes())
                .collect(),
            bevy::render::render_resource::TextureFormat::R32Float,
            RenderAssetUsages::all(),
        );
        flow_y.texture_descriptor.usage |= TextureUsages::STORAGE_BINDING | TextureUsages::COPY_DST;
        let flow_y_handle = images.add(flow_y);
        
        // Use source base for all tiles for now
        let base_handle = images.add(source_base.clone());
        
        tile_textures.push(WaterTileTextures {
            texture_a: water_a_handle,
            texture_b: water_b_handle,
            flow_x: flow_x_handle,
            flow_y: flow_y_handle,
            base_height: base_handle,
        });
    }
    
    commands.insert_resource(TiledWaterTextures::new(tile_textures, layout));
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
                    bevy::render::render_resource::TextureFormat::R32Float,
                    bevy::render::render_resource::StorageTextureAccess::ReadOnly,
                ),
                texture_storage_2d(
                    bevy::render::render_resource::TextureFormat::R32Float,
                    bevy::render::render_resource::StorageTextureAccess::WriteOnly,
                ),
                texture_storage_2d(
                    bevy::render::render_resource::TextureFormat::R32Float,
                    bevy::render::render_resource::StorageTextureAccess::ReadWrite,
                ),
                texture_storage_2d(
                    bevy::render::render_resource::TextureFormat::R32Float,
                    bevy::render::render_resource::StorageTextureAccess::ReadWrite,
                ),
                storage_buffer_read_only::<Vec<Vec4>>(false),
                texture_storage_2d(
                    bevy::render::render_resource::TextureFormat::R32Float,
                    bevy::render::render_resource::StorageTextureAccess::ReadOnly,
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
        push_constant_ranges: vec![PushConstantRange {
            stages: ShaderStages::COMPUTE,
            range: 0..std::mem::size_of::<SimParams>() as u32,
        }],
        ..Default::default()
    });
    commands.insert_resource(WaterRenderPipeline {
        layout: texture_bind_group_layout,
        pipeline: update_pipeline,
    });
}

/// Old single-texture version (kept for reference)
#[allow(dead_code)]
fn prepare_water_bindgroups(
    mut commands: Commands,
    pipeline: Res<WaterRenderPipeline>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    water_images: If<Res<WaterHeightTexture>>,
    mut displacement: If<ResMut<DisplacementBuffer>>,
    displacement_main: If<Res<DisplacementBufferMain>>,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    render_queue: Res<RenderQueue>,
) {
    *displacement.0.buffer.get_mut() = displacement_main.buffer.clone();
    displacement
        .buffer
        .write_buffer(&render_device, &render_queue);

    let tex_a = gpu_images.get(&water_images.texture_a).unwrap();
    let tex_b = gpu_images.get(&water_images.texture_b).unwrap();
    let flow_x = gpu_images.get(&water_images.flow_x).unwrap();
    let flow_y = gpu_images.get(&water_images.flow_y).unwrap();
    let base = gpu_images.get(&water_images.base_height).unwrap();

    let bind_group_0 = render_device.create_bind_group(
        None,
        &pipeline_cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((
            &tex_a.texture_view,
            &tex_b.texture_view,
            &flow_x.texture_view,
            &flow_y.texture_view,
            displacement.buffer.binding().unwrap(),
            &base.texture_view,
        )),
    );
    let bind_group_1 = render_device.create_bind_group(
        None,
        &pipeline_cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((
            &tex_b.texture_view,
            &tex_a.texture_view,
            &flow_x.texture_view,
            &flow_y.texture_view,
            displacement.buffer.binding().unwrap(),
            &base.texture_view,
        )),
    );
    commands.insert_resource(WaterBindGroups([bind_group_0, bind_group_1]));
}

/// New tiled version - creates bind groups for all 9 tiles
fn prepare_tiled_water_bindgroups(
    mut commands: Commands,
    pipeline: Res<WaterRenderPipeline>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    water_images: Res<TiledWaterTextures>,
    mut displacement: ResMut<DisplacementBuffer>,
    displacement_main: Res<DisplacementBufferMain>,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    render_queue: Res<RenderQueue>,
) {
    *displacement.buffer.get_mut() = displacement_main.buffer.clone();
    displacement
        .buffer
        .write_buffer(&render_device, &render_queue);

    // Create bind groups for each tile
    let mut tile_bindgroups = Vec::with_capacity(9);
    
    for tile_textures in &water_images.tiles {
        let tex_a = gpu_images.get(&tile_textures.texture_a).unwrap();
        let tex_b = gpu_images.get(&tile_textures.texture_b).unwrap();
        let flow_x = gpu_images.get(&tile_textures.flow_x).unwrap();
        let flow_y = gpu_images.get(&tile_textures.flow_y).unwrap();
        let base = gpu_images.get(&tile_textures.base_height).unwrap();

        let bind_group_0 = render_device.create_bind_group(
            None,
            &pipeline_cache.get_bind_group_layout(&pipeline.layout),
            &BindGroupEntries::sequential((
                &tex_a.texture_view,
                &tex_b.texture_view,
                &flow_x.texture_view,
                &flow_y.texture_view,
                displacement.buffer.binding().unwrap(),
                &base.texture_view,
            )),
        );
        let bind_group_1 = render_device.create_bind_group(
            None,
            &pipeline_cache.get_bind_group_layout(&pipeline.layout),
            &BindGroupEntries::sequential((
                &tex_b.texture_view,
                &tex_a.texture_view,
                &flow_x.texture_view,
                &flow_y.texture_view,
                displacement.buffer.binding().unwrap(),
                &base.texture_view,
            )),
        );
        
        tile_bindgroups.push([bind_group_0, bind_group_1]);
    }
    
    commands.insert_resource(WaterTileBindGroups {
        bindgroups: tile_bindgroups,
    });
}

#[derive(Resource, Clone, ExtractResource)]
pub struct WaterHeightTexture {
    pub texture_a: Handle<Image>,
    pub texture_b: Handle<Image>,
    pub flow_x: Handle<Image>,
    pub flow_y: Handle<Image>,
    pub base_height: Handle<Image>,
}

pub fn height_from_texture(t: &Image, world: Vec2) -> f32 {
    let uv = (world * 10.0 + 1024.0).floor();
    if uv.x < 0.0 || uv.x as u32 >= t.width() {
        return 0.0;
    }
    if uv.y < 0.0 || uv.y as u32 >= t.height() {
        return 0.0;
    }
    let x = uv.x as u32;
    let y = uv.y as u32;

    t.get_color_at(x, y).unwrap().to_linear().red
}

/// Height lookup for tiled textures using local UV coordinates
pub fn height_from_tiled_texture(t: &Image, local_uv: Vec2) -> f32 {
    let uv = local_uv.floor();
    if uv.x < 0.0 || uv.x as u32 >= t.width() {
        return 0.0;
    }
    if uv.y < 0.0 || uv.y as u32 >= t.height() {
        return 0.0;
    }
    let x = uv.x as u32;
    let y = uv.y as u32;

    t.get_color_at(x, y).unwrap().to_linear().red
}

#[derive(Resource, Clone, ExtractResource)]
struct DisplacementBufferMain {
    buffer: Vec<Vec4>,
}

#[derive(Resource)]
struct DisplacementBuffer {
    buffer: StorageBuffer<Vec<Vec4>>,
}
