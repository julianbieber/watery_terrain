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
    shader::ShaderDefVal,
};
use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;

use crate::{
    grid::{DIM, cell_at},
    render::clipmap::WaterTerrainMaterial,
    screens::Screen,
};

const WORKGROUP_SIZE: u32 = 8;

const STEP_SECONDS: f32 = 1.0 / 60.0;

const MAX_STEPS_PER_FRAME: u32 = 4;

pub struct WaterSimPlugin;

#[derive(Component)]
pub struct WaterMarker;

impl Plugin for WaterSimPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "water.wesl");
        app.add_plugins(ExtractResourcePlugin::<WaterHeightTexture>::default());
        app.add_plugins(ExtractResourcePlugin::<DisplacementBufferMain>::default());
        app.add_plugins(ExtractResourcePlugin::<WaterSimSteps>::default());
        app.init_resource::<WaterSimSteps>();
        app.add_systems(Update, count_sim_steps);
        app.insert_resource(DisplacementBufferMain { buffer: Vec::new() });
        app.add_systems(
            Update,
            collect_displacements.run_if(in_state(Screen::Gameplay)),
        );
        let render_app = app.sub_app_mut(RenderApp);
        render_app.add_systems(RenderStartup, init_water_render);
        render_app.add_systems(Render, prepare_water_bindgroups);
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
    let water = textures.get(water_height.water.id()).unwrap();
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
struct WaterBindGroup(BindGroup);

#[derive(Resource, Clone, Default, ExtractResource)]
#[extract_app(RenderApp)]
struct WaterSimSteps {
    due: u32,
    unsimulated_seconds: f32,
}

fn count_sim_steps(time: Res<Time>, mut steps: ResMut<WaterSimSteps>) {
    let elapsed = steps.unsimulated_seconds + time.delta_secs();
    let due = ((elapsed / STEP_SECONDS) as u32).min(MAX_STEPS_PER_FRAME);
    steps.due = due;
    steps.unsimulated_seconds = (elapsed - due as f32 * STEP_SECONDS).min(STEP_SECONDS);
}

fn run_water_sim(
    mut render_context: RenderContext,
    pipeline: If<Res<WaterRenderPipeline>>,
    pipeline_cache: If<Res<PipelineCache>>,
    bind_group: If<Res<WaterBindGroup>>,
    steps: If<Res<WaterSimSteps>>,
) {
    let Some(update_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.pipeline) else {
        return;
    };
    let mut pass = render_context
        .command_encoder()
        .begin_compute_pass(&ComputePassDescriptor::default());

    pass.set_bind_group(0, &bind_group.0.0, &[]);
    pass.set_pipeline(update_pipeline);

    let workgroups = (DIM + 1).div_ceil(WORKGROUP_SIZE);
    for _ in 0..steps.0.due {
        for id in [0, 1] {
            pass.set_immediates(
                0,
                bytemuck::bytes_of(&SimParams {
                    id,
                    _pad: Vec3::ZERO,
                }),
            );
            pass.dispatch_workgroups(workgroups, workgroups, 1);
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct SimParams {
    id: i32,
    _pad: Vec3,
}

fn init_internal_textures(
    trigger: On<
        Add<(
            // MeshMaterial3d<ExtendedMaterial<StandardMaterial, TerrainMaterial>>,
            WaterMarker,
        )>,
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

    commands.insert_resource(WaterHeightTexture {
        water: material.extension.water.clone(),
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
                    bevy::render::render_resource::StorageTextureAccess::ReadWrite,
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
        .load(AssetPath::from_path_buf(embedded_path!("water.wesl")).with_source("embedded"));
    let update_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        layout: vec![texture_bind_group_layout.clone()],
        shader,
        entry_point: Some(Cow::from("main")),
        shader_defs: vec![ShaderDefVal::UInt("WORKGROUP_SIZE".into(), WORKGROUP_SIZE)],
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

    let water = gpu_images.get(&water_images.water).unwrap();
    let flow_x = gpu_images.get(&water_images.flow_x).unwrap();
    let flow_y = gpu_images.get(&water_images.flow_y).unwrap();
    let base = gpu_images.get(&water_images.base_height).unwrap();

    let bind_group = render_device.create_bind_group(
        None,
        &pipeline_cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((
            &water.texture_view,
            &flow_x.texture_view,
            &flow_y.texture_view,
            displacement.buffer.binding().unwrap(),
            &base.texture_view,
        )),
    );
    commands.insert_resource(WaterBindGroup(bind_group));
}

#[derive(Resource, Clone, ExtractResource)]
#[extract_app(RenderApp)]
pub struct WaterHeightTexture {
    pub water: Handle<Image>,
    pub flow_x: Handle<Image>,
    pub flow_y: Handle<Image>,
    pub base_height: Handle<Image>,
}

pub fn height_from_texture(t: &Image, world: Vec2) -> f32 {
    let Some(cell) = cell_at(world) else {
        return 0.0;
    };
    t.get_color_at(cell.x, cell.y).unwrap().to_linear().red
}

#[derive(Resource, Clone, ExtractResource)]
#[extract_app(RenderApp)]
struct DisplacementBufferMain {
    buffer: Vec<Vec4>,
}

#[derive(Resource)]
struct DisplacementBuffer {
    buffer: StorageBuffer<Vec<Vec4>>,
}
