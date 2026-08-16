use avian3d::prelude::LinearVelocity;
use bevy::{
    asset::{AssetPath, RenderAssetUsages, embedded_asset, embedded_path},
    pbr::ExtendedMaterial,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_resource::{
            BindGroup, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            CachedComputePipelineId, ComputePassDescriptor, ComputePipelineDescriptor, Extent3d,
            PipelineCache, ShaderStages, StorageBuffer, TextureDimension, TextureUsages,
            binding_types::{storage_buffer_read_only, texture_storage_2d},
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
        texture::GpuImage,
    },
};
use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;

use crate::{render::clipmap::WaterTerrainMaterial, screens::Screen};

pub struct WaterSimPlugin;

#[derive(Component)]
pub struct WaterMarker;

impl Plugin for WaterSimPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "water.wgsl");
        app.add_plugins(ExtractResourcePlugin::<WaterHeightTexture>::default());
        app.add_plugins(ExtractResourcePlugin::<DisplacementBufferMain>::default());
        app.insert_resource(DisplacementBufferMain { buffer: Vec::new() });
        app.add_systems(
            Update,
            collect_displacements.run_if(in_state(Screen::Gameplay)),
        );
        let render_app = app.sub_app_mut(RenderApp);
        render_app.add_systems(RenderStartup, init_water_render);
        render_app.add_systems(Render, prepare_water_bindgroups);
        render_app.insert_resource(WaterBindGroupsSwap(true));
        let displacements = StorageBuffer::<Vec<Vec4>>::from(Vec::new());
        render_app.insert_resource(DisplacementBuffer {
            buffer: displacements,
        });
        render_app.add_systems(RenderGraph, run_water_sim);

        app.add_observer(init_internal_textures);
    }
}

#[derive(Component)]
pub struct WaterDisplacement {
    pub radius: f32,
    pub strength: f32,
}

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

#[derive(Resource)]
struct WaterRenderPipeline {
    layout: BindGroupLayoutDescriptor,
    pipeline: CachedComputePipelineId,
}

#[derive(Resource)]
struct WaterBindGroups([BindGroup; 2]);

#[derive(Resource)]
struct WaterBindGroupsSwap(bool);

fn run_water_sim(
    mut render_context: RenderContext,
    pipeline: If<Res<WaterRenderPipeline>>,
    pipeline_cache: If<Res<PipelineCache>>,
    bind_groups: If<Res<WaterBindGroups>>,
    swap: If<Res<WaterBindGroupsSwap>>,
) {
    let mut pass = render_context
        .command_encoder()
        .begin_compute_pass(&ComputePassDescriptor::default());

    let update_pipeline = pipeline_cache
        .get_compute_pipeline(pipeline.pipeline)
        .unwrap();
    pass.set_bind_group(0, &bind_groups.0.0[swap.0.0 as usize], &[]);
    pass.set_pipeline(update_pipeline);

    pass.set_immediates(
        0,
        bytemuck::bytes_of(&SimParams {
            id: 0,
            _pad: Vec3::ZERO,
        }),
    );
    pass.dispatch_workgroups(2048 / 8, 2048 / 8, 1);

    pass.set_immediates(
        0,
        bytemuck::bytes_of(&SimParams {
            id: 1,
            _pad: Vec3::ZERO,
        }),
    );
    pass.dispatch_workgroups(2048 / 8, 2048 / 8, 1);
}

fn update_water_sim(world: &mut World) {
    let mut s = world.resource_mut::<WaterBindGroupsSwap>();
    s.0 = !s.0;
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct SimParams {
    id: i32,
    _pad: Vec3,
}

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
        immediate_size: std::mem::size_of::<SimParams>() as u32,
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

#[derive(Resource, Clone, ExtractResource)]
struct DisplacementBufferMain {
    buffer: Vec<Vec4>,
}

#[derive(Resource)]
struct DisplacementBuffer {
    buffer: StorageBuffer<Vec<Vec4>>,
}
