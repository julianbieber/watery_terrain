pub mod clipmap;

use std::borrow::Cow;

use bevy::{
    asset::{AssetPath, embedded_asset, embedded_path},
    pbr::ExtendedMaterial,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_graph::{self, RenderGraph, RenderLabel},
        render_resource::{
            BindGroup, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            CachedComputePipelineId, ComputePassDescriptor, ComputePipelineDescriptor,
            PipelineCache, ShaderStages, binding_types::texture_storage_2d,
        },
        renderer::RenderDevice,
        texture::GpuImage,
    },
};

use crate::{
    render::clipmap::{TerrainMaterial, follow},
    screens::Screen,
};

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
                    bevy::render::render_resource::TextureFormat::R32Float,
                    bevy::render::render_resource::StorageTextureAccess::ReadOnly,
                ),
                texture_storage_2d(
                    bevy::render::render_resource::TextureFormat::R32Float,
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

#[derive(Resource, Clone, ExtractResource)]
pub struct WaterHeightTexture {
    pub texture_a: Handle<Image>,
    pub texture_b: Handle<Image>,
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
