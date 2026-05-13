use avian3d::prelude::LinearVelocity;
use bevy::{
    asset::{AssetPath, RenderAssetUsages, embedded_asset, embedded_path},
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

use crate::{render::clipmap::TerrainMaterial, screens::Screen};

pub struct WaterSimPlugin;

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

        let mut render_graph = render_app.world_mut().resource_mut::<RenderGraph>();
        render_graph.add_node(WaterRenderLabel, WaterRenderNode);
        render_graph.add_node_edge(WaterRenderLabel, bevy::render::graph::CameraDriverLabel);
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
    buffer.buffer.clear();
    for (mut velocity, transform, w) in &mut d {
        let h = height_from_texture(water, transform.translation.xz());
        if transform.translation.y < h {
            let t_h = transform.translation.y;
            let depth = h - (t_h);
            buffer.buffer.push(Vec4::new(
                transform.translation.x,
                transform.translation.z,
                w.radius,
                w.strength * (depth + 0.2),
            ));
            // velocity.0 *= 0.8;
            velocity.0.y = depth * 0.8;
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
            pass.set_pipeline(update_pipeline);

            pass.set_push_constants(
                0,
                bytemuck::bytes_of(&SimParams {
                    id: 0,
                    _pad: Vec3::ZERO,
                }),
            );
            pass.dispatch_workgroups(2048 / 8, 2048 / 8, 1);

            pass.set_push_constants(
                0,
                bytemuck::bytes_of(&SimParams {
                    id: 1,
                    _pad: Vec3::ZERO,
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
    _pad: Vec3,
}

fn init_internal_textures(
    _: On<Add, MeshMaterial3d<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
    mut commands: Commands,
    material: Single<&MeshMaterial3d<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
    materials: Res<Assets<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
    mut images: ResMut<Assets<Image>>,
) {
    let material = materials.get(material.0.id()).unwrap();
    let water_height = images.get(material.extension.height.id()).unwrap();
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
        texture_a: material.extension.height.clone(),
        texture_b: water_height_2,
        flow_x,
        flow_y,
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

    let bind_group_0 = render_device.create_bind_group(
        None,
        &pipeline_cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((
            &tex_a.texture_view,
            &tex_b.texture_view,
            &flow_x.texture_view,
            &flow_y.texture_view,
            displacement.buffer.binding().unwrap(),
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

    t.get_color_at(x, y).unwrap().to_linear().red * 10.0
}

#[derive(Resource, Clone, ExtractResource)]
struct DisplacementBufferMain {
    buffer: Vec<Vec4>,
}

#[derive(Resource)]
struct DisplacementBuffer {
    buffer: StorageBuffer<Vec<Vec4>>,
}
