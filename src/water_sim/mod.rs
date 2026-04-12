use bevy::{
    asset::{AssetPath, RenderAssetUsages, embedded_asset, embedded_path},
    pbr::ExtendedMaterial,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup,
        extract_resource::ExtractResource,
        render_asset::RenderAssets,
        render_graph::{self, RenderGraph, RenderLabel},
        render_resource::{
            BindGroup, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            CachedComputePipelineId, ComputePassDescriptor, ComputePipelineDescriptor, Extent3d,
            PipelineCache, PushConstantRange, ShaderStages, TextureDimension, TextureUsages,
            binding_types::texture_storage_2d,
        },
        renderer::RenderDevice,
        texture::GpuImage,
    },
};
use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;

use crate::render::clipmap::TerrainMaterial;
pub struct WaterSimPlugin;

impl Plugin for WaterSimPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "water.wgsl");
        let render_app = app.sub_app_mut(RenderApp);
        render_app.add_systems(RenderStartup, init_water_render);
        render_app.add_systems(Render, prepare_water_bindgroups);
        render_app.insert_resource(WaterBindGroupsSwap(true));

        let mut render_graph = render_app.world_mut().resource_mut::<RenderGraph>();
        render_graph.add_node(WaterRenderLabel, WaterRenderNode);
        render_graph.add_node_edge(WaterRenderLabel, bevy::render::graph::CameraDriverLabel);
        app.add_observer(init_internal_textures);
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
            .into_iter()
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
            .into_iter()
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
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
) {
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
