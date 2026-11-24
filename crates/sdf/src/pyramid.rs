//! The max-mip pyramids the SDF surfaces are ray-marched through, and their
//! upkeep on the GPU: the terrain's whenever the window's ground changes, the
//! water's after every simulation step.

use std::borrow::Cow;

use bevy::{
    asset::{AssetPath, RenderAssetUsages, embedded_asset, embedded_path},
    prelude::*,
    render::{
        RenderApp, RenderStartup,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_resource::{
            BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            CachedComputePipelineId, ComputePass, ComputePassDescriptor, ComputePipeline,
            ComputePipelineDescriptor, Extent3d, PipelineCache, ShaderStages, StorageTextureAccess,
            TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureView,
            TextureViewDescriptor,
            binding_types::{texture_2d, texture_storage_2d},
        },
        renderer::{RenderContext, RenderDevice, RenderGraphSystems},
        texture::GpuImage,
    },
    shader::ShaderDefVal,
};
use bytemuck::{Pod, Zeroable};

use watery_terrain::{
    grid::{CHUNK_DIM, COARSE_FACTOR, WINDOW_DIM},
    render::{ChunkLayout, WaterStep, WindowHeights},
};

const WORKGROUP_SIZE: u32 = 8;

const TERRAIN_LEVELS: u32 = 10;

const WATER_LEVELS: u32 = 9;

#[derive(Resource, Clone, ExtractResource)]
#[extract_app(RenderApp)]
pub(crate) struct Pyramids {
    pub terrain: Handle<Image>,
    pub water: Handle<Image>,
    pub centre_water: Handle<Image>,
}

impl Pyramids {
    pub fn new(images: &mut Assets<Image>) -> Self {
        let mut pyramid = |dim: u32, levels: u32| {
            let mut image = Image::new_uninit(
                Extent3d {
                    width: dim,
                    height: dim,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                TextureFormat::R32Float,
                RenderAssetUsages::RENDER_WORLD,
            );
            image.texture_descriptor.mip_level_count = levels;
            image.texture_descriptor.usage =
                TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING;
            images.add(image)
        };
        Self {
            terrain: pyramid(WINDOW_DIM / 2, TERRAIN_LEVELS),
            water: pyramid(WINDOW_DIM / COARSE_FACTOR, WATER_LEVELS),
            centre_water: pyramid(CHUNK_DIM / 2, 1),
        }
    }
}

pub(crate) struct PyramidPlugin;

impl Plugin for PyramidPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "pyramid.wesl");
        app.add_plugins(ExtractResourcePlugin::<Pyramids>::default());
        let render_app = app.sub_app_mut(RenderApp);
        render_app.add_systems(RenderStartup, init_pipelines);
        render_app.add_systems(
            RenderGraph,
            build_pyramids
                .after(WaterStep)
                .in_set(RenderGraphSystems::Begin),
        );
    }
}

#[derive(Resource)]
struct PyramidPipelines {
    one_input: BindGroupLayoutDescriptor,
    four_inputs: BindGroupLayoutDescriptor,
    reduce: CachedComputePipelineId,
    terrain_base: CachedComputePipelineId,
    centre_base: CachedComputePipelineId,
    water_base: CachedComputePipelineId,
}

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct Params {
    window_origin: UVec2,
    centre: UVec2,
}

fn init_pipelines(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    pipeline_cache: Res<PipelineCache>,
) {
    let input = || texture_2d(TextureSampleType::Float { filterable: false });
    let output = || texture_storage_2d(TextureFormat::R32Float, StorageTextureAccess::WriteOnly);
    let one_input = BindGroupLayoutDescriptor::new(
        "sdf pyramid, one input",
        &BindGroupLayoutEntries::sequential(ShaderStages::COMPUTE, (output(), input())),
    );
    let four_inputs = BindGroupLayoutDescriptor::new(
        "sdf pyramid, four inputs",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (output(), input(), input(), input(), input()),
        ),
    );
    let shader: Handle<Shader> = asset_server
        .load(AssetPath::from_path_buf(embedded_path!("pyramid.wesl")).with_source("embedded"));
    let queue = |layout: &BindGroupLayoutDescriptor, entry: &'static str| {
        pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some(Cow::from(entry)),
            layout: vec![layout.clone()],
            shader: shader.clone(),
            entry_point: Some(Cow::from(entry)),
            shader_defs: vec![ShaderDefVal::UInt("WORKGROUP_SIZE".into(), WORKGROUP_SIZE)],
            immediate_size: size_of::<Params>() as u32,
            ..default()
        })
    };
    commands.insert_resource(PyramidPipelines {
        reduce: queue(&one_input, "reduce"),
        terrain_base: queue(&one_input, "terrain_base"),
        centre_base: queue(&four_inputs, "centre_base"),
        water_base: queue(&four_inputs, "water_base"),
        one_input,
        four_inputs,
    });
}

struct Pass<'a> {
    device: RenderDevice,
    cache: &'a PipelineCache,
    pipelines: &'a PyramidPipelines,
    params: Params,
}

impl Pass<'_> {
    fn dispatch(
        &self,
        pass: &mut ComputePass,
        pipeline: &ComputePipeline,
        output: &TextureView,
        inputs: &[&TextureView],
        dim: u32,
    ) {
        let bind_group = match inputs {
            [a] => self.device.create_bind_group(
                None,
                &self.cache.get_bind_group_layout(&self.pipelines.one_input),
                &BindGroupEntries::sequential((output, *a)),
            ),
            [a, b, c, d] => self.device.create_bind_group(
                None,
                &self
                    .cache
                    .get_bind_group_layout(&self.pipelines.four_inputs),
                &BindGroupEntries::sequential((output, *a, *b, *c, *d)),
            ),
            _ => unreachable!("pyramid passes take one or four inputs"),
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_immediates(0, bytemuck::bytes_of(&self.params));
        let groups = dim.div_ceil(WORKGROUP_SIZE);
        pass.dispatch_workgroups(groups, groups, 1);
    }

    fn reduce_levels(
        &self,
        pass: &mut ComputePass,
        reduce: &ComputePipeline,
        levels: &[TextureView],
        base_dim: u32,
    ) {
        for (k, pair) in levels.windows(2).enumerate() {
            self.dispatch(pass, reduce, &pair[1], &[&pair[0]], base_dim >> (k + 1));
        }
    }
}

fn mip_views(image: &GpuImage, levels: u32) -> Vec<TextureView> {
    (0..levels)
        .map(|level| {
            image.texture.create_view(&TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..default()
            })
        })
        .collect()
}

fn build_pyramids(
    mut context: RenderContext,
    pyramids: Option<Res<Pyramids>>,
    heights: Option<Res<WindowHeights>>,
    layout: Option<Res<ChunkLayout>>,
    pipelines: Option<Res<PyramidPipelines>>,
    cache: Res<PipelineCache>,
    images: Res<RenderAssets<GpuImage>>,
    mut terrain_built: Local<Option<(AssetId<Image>, ChunkLayout)>>,
) {
    let (Some(pyramids), Some(heights), Some(layout), Some(pipelines)) =
        (pyramids, heights, layout, pipelines)
    else {
        return;
    };
    let (Some(reduce), Some(terrain_base), Some(centre_base), Some(water_base)) = (
        cache.get_compute_pipeline(pipelines.reduce),
        cache.get_compute_pipeline(pipelines.terrain_base),
        cache.get_compute_pipeline(pipelines.centre_base),
        cache.get_compute_pipeline(pipelines.water_base),
    ) else {
        return;
    };
    let image = |handle: &Handle<Image>| images.get(handle);
    let (
        Some(terrain),
        Some(water_pyramid),
        Some(centre_water),
        Some(ground),
        Some(water),
        Some(coarse_water),
        Some(coarse_ground),
    ) = (
        image(&pyramids.terrain),
        image(&pyramids.water),
        image(&pyramids.centre_water),
        image(&heights.ground),
        image(&heights.water),
        image(&heights.coarse_water),
        image(&heights.coarse_ground),
    )
    else {
        return;
    };

    let terrain_levels = mip_views(terrain, TERRAIN_LEVELS);
    let water_levels = mip_views(water_pyramid, WATER_LEVELS);
    let rebuild_terrain = *terrain_built != Some((pyramids.terrain.id(), *layout));
    let pass_info = Pass {
        device: context.render_device().clone(),
        cache: &cache,
        pipelines: &pipelines,
        params: Params {
            window_origin: layout.origin_uvec(),
            centre: layout.centre_uvec(),
        },
    };
    let mut pass = context
        .command_encoder()
        .begin_compute_pass(&ComputePassDescriptor {
            label: Some("sdf pyramids"),
            ..default()
        });
    if rebuild_terrain {
        pass_info.dispatch(
            &mut pass,
            terrain_base,
            &terrain_levels[0],
            &[&ground.texture_view],
            WINDOW_DIM / 2,
        );
        pass_info.reduce_levels(&mut pass, reduce, &terrain_levels, WINDOW_DIM / 2);
    }
    pass_info.dispatch(
        &mut pass,
        centre_base,
        &centre_water.texture_view,
        &[
            &water.texture_view,
            &ground.texture_view,
            &coarse_water.texture_view,
            &coarse_ground.texture_view,
        ],
        CHUNK_DIM / 2,
    );
    pass_info.dispatch(
        &mut pass,
        water_base,
        &water_levels[0],
        &[
            &centre_water.texture_view,
            &terrain_levels[1],
            &coarse_water.texture_view,
            &coarse_ground.texture_view,
        ],
        WINDOW_DIM / COARSE_FACTOR,
    );
    pass_info.reduce_levels(&mut pass, reduce, &water_levels, WINDOW_DIM / COARSE_FACTOR);
    if rebuild_terrain {
        *terrain_built = Some((pyramids.terrain.id(), *layout));
    }
}
