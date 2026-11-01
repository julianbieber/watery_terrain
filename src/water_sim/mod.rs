//! The shallow-water simulation over the chunk window on the GPU, and how
//! objects push the water and are lifted by it.

use avian3d::prelude::{Forces, Gravity, ReadRigidBodyForces, WriteRigidBodyForces};
use bevy::{
    asset::{AssetPath, RenderAssetUsages, embedded_asset, embedded_path},
    ecs::entity::EntityHashMap,
    pbr::ExtendedMaterial,
    prelude::*,
    render::{
        MainWorld, Render, RenderApp, RenderStartup, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        gpu_readback::{Readback, ReadbackComplete},
        render_asset::RenderAssets,
        render_resource::{
            BindGroup, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, Buffer,
            BufferDescriptor, BufferInitDescriptor, BufferUsages, CachedComputePipelineId,
            ComputePassDescriptor, ComputePipelineDescriptor, Extent3d, Origin3d, PipelineCache,
            ShaderStages, ShaderType, StorageBuffer, TexelCopyBufferInfo, TexelCopyBufferLayout,
            TexelCopyTextureInfo, Texture, TextureAspect, TextureDimension, TextureFormat,
            TextureUsages,
            binding_types::{storage_buffer, storage_buffer_read_only, texture_storage_2d},
        },
        renderer::{RenderContext, RenderDevice, RenderGraphSystems, RenderQueue},
        storage::{GpuShaderBuffer, ShaderBuffer},
        texture::GpuImage,
    },
    shader::ShaderDefVal,
};
use bytemuck::{Pod, Zeroable};
use std::{
    borrow::Cow,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::{
    chunks::{ChunkLayout, ChunkSwap, Parked, StagingBuffers, SwapQueue, park_objects},
    grid::{CHUNK_DIM, WINDOW_CHUNKS, WINDOW_DIM},
    render::clipmap::WaterTerrainMaterial,
    screens::Screen,
};

const WORKGROUP_SIZE: u32 = 8;

const STEP_SECONDS: f32 = 1.0 / 60.0;

const MAX_STEPS_PER_FRAME: u32 = 4;

/// Water depth in millimetres at or below which a cell counts as dry: it is
/// not drawn and lifts nothing. The water shader reads it as
/// `constants::DRY_DEPTH_MM`.
pub const DRY_DEPTH_MM: u32 = 10;

const DRY_DEPTH: f32 = DRY_DEPTH_MM as f32 / 1000.0;

const WATER_DENSITY_RATIO: f32 = 2.0;

const WATER_DRAG_PER_SECOND: f32 = 2.0;

const MAX_DISPLACEMENTS: usize = (WORKGROUP_SIZE * WORKGROUP_SIZE) as usize;

/// Cells from the window's border over which the flow fades to zero. The sim
/// shader reads it as `constants::EDGE_FADE_CELLS`.
pub const EDGE_FADE_CELLS: u32 = 100;

/// Simulates the water drawn by the [`WaterMarker`] clipmap and its
/// interaction with [`WaterDisplacement`] objects.
pub struct WaterSimPlugin;

/// The water clipmap. Adding it starts the simulation on the textures of the
/// entity's [`WaterTerrainMaterial`], which the entity must already carry.
#[derive(Component)]
pub struct WaterMarker;

impl Plugin for WaterSimPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "water.wesl");
        app.add_plugins(ExtractResourcePlugin::<WaterHeightTexture>::default());
        app.add_plugins(ExtractResourcePlugin::<DisplacementBufferMain>::default());
        app.add_plugins(ExtractResourcePlugin::<WaterSimSteps>::default());
        app.add_plugins(ExtractResourcePlugin::<ChunkLayout>::default());
        app.add_plugins(ExtractResourcePlugin::<StagingBuffers>::default());
        let ready = ChunkGpuReady::default();
        app.insert_resource(ready.clone());
        app.init_resource::<WaterSimSteps>();
        app.add_systems(Update, count_sim_steps);
        app.insert_resource(DisplacementBufferMain { buffer: Vec::new() });
        app.init_resource::<WaterAtObjects>();
        app.add_systems(
            Update,
            collect_displacements
                .after(park_objects)
                .run_if(in_state(Screen::Gameplay)),
        );
        app.add_systems(
            FixedUpdate,
            apply_buoyancy.run_if(in_state(Screen::Gameplay)),
        );
        let render_app = app.sub_app_mut(RenderApp);
        render_app.add_systems(RenderStartup, init_water_render);
        render_app.insert_resource(ready);
        render_app.init_resource::<ExtractedSwaps>();
        render_app.add_systems(ExtractSchedule, extract_swaps);
        render_app.add_systems(
            Render,
            (prepare_water_bindgroups, mark_chunks_ready)
                .chain()
                .in_set(RenderSystems::PrepareBindGroups),
        );
        render_app.insert_resource(DisplacementBuffer {
            buffer: StorageBuffer::from(Displacements {
                count: 0,
                items: [Displacement::default(); MAX_DISPLACEMENTS],
            }),
        });
        render_app.add_systems(
            RenderGraph,
            (swap_chunks, run_water_sim)
                .chain()
                .in_set(RenderGraphSystems::Begin),
        );

        app.add_observer(init_internal_textures);
    }
}

/// An object that pushes water away and is lifted by it: `radius` is its reach
/// in metres, `strength` scales its push. Only the first 64 such objects take
/// part.
#[derive(Component)]
pub struct WaterDisplacement {
    pub radius: f32,
    pub strength: f32,
}

/// The water column under each [`WaterDisplacement`] object, as the GPU last
/// sampled it. An object appears here a few frames after it first gets a
/// [`WaterDisplacement`], and the sample lags its position by as long.
#[derive(Resource, Default)]
pub struct WaterAtObjects(pub EntityHashMap<WaterColumn>);

/// Water depth and ground height in metres at one point of the grid.
#[derive(Clone, Copy, Debug)]
pub struct WaterColumn {
    pub depth: f32,
    pub ground: f32,
}

fn submerged_fraction(column: &WaterColumn, transform: &Transform, radius: f32) -> f32 {
    let bottom_above_ground = (transform.translation.y - radius) - column.ground;
    if column.depth <= DRY_DEPTH || bottom_above_ground >= column.depth {
        return 0.0;
    }
    (column.depth - bottom_above_ground).min(radius * 2.0) / (radius * 2.0)
}

fn collect_displacements(
    d: Query<(Entity, &Transform, &WaterDisplacement), Without<Parked>>,
    mut buffer: ResMut<DisplacementBufferMain>,
    water: Res<WaterAtObjects>,
) {
    buffer.buffer.clear();
    for (entity, transform, w) in d.iter().take(MAX_DISPLACEMENTS) {
        let in_water = water.0.get(&entity).map_or(0.0, |column| {
            submerged_fraction(column, transform, w.radius)
        });
        let bits = entity.to_bits();
        buffer.buffer.push(Displacement {
            center: transform.translation.xz(),
            radius: w.radius,
            push: w.strength * in_water,
            entity: UVec2::new(bits as u32, (bits >> 32) as u32),
        });
    }
}

fn apply_buoyancy(
    mut bodies: Query<(Entity, Forces, &Transform, &WaterDisplacement), Without<Parked>>,
    water: Res<WaterAtObjects>,
    gravity: Res<Gravity>,
) {
    for (entity, mut forces, transform, w) in &mut bodies {
        let Some(column) = water.0.get(&entity) else {
            continue;
        };
        let in_water = submerged_fraction(column, transform, w.radius);
        if in_water == 0.0 {
            continue;
        }
        let lift = gravity.0.length() * WATER_DENSITY_RATIO * in_water;
        let drag = -forces.linear_velocity().y * WATER_DRAG_PER_SECOND * in_water;
        forces.apply_linear_acceleration(Vec3::Y * (lift + drag));
    }
}

fn store_water_samples(readback: On<ReadbackComplete>, mut water: ResMut<WaterAtObjects>) {
    let samples: WaterSamples = readback.to_shader_type();
    water.0.clear();
    for sample in &samples.items[..samples.count as usize] {
        let bits = u64::from(sample.entity.x) | (u64::from(sample.entity.y) << 32);
        if let Some(entity) = Entity::try_from_bits(bits) {
            water.0.insert(
                entity,
                WaterColumn {
                    depth: sample.depth,
                    ground: sample.ground,
                },
            );
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
    layout: If<Res<ChunkLayout>>,
) {
    let window_origin = layout.origin_uvec();
    let Some(update_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.pipeline) else {
        return;
    };
    let mut pass = render_context
        .command_encoder()
        .begin_compute_pass(&ComputePassDescriptor::default());

    pass.set_bind_group(0, &bind_group.0.0, &[]);
    pass.set_pipeline(update_pipeline);

    let workgroups = WINDOW_DIM.div_ceil(WORKGROUP_SIZE);
    for _ in 0..steps.0.due {
        for id in [0, 1, 2, 3] {
            pass.set_immediates(
                0,
                bytemuck::bytes_of(&SimParams {
                    id,
                    _pad: 0,
                    window_origin,
                }),
            );
            pass.dispatch_workgroups(workgroups, workgroups, 1);
        }
    }

    pass.set_immediates(
        0,
        bytemuck::bytes_of(&SimParams {
            id: 4,
            _pad: 0,
            window_origin,
        }),
    );
    pass.dispatch_workgroups(1, 1, 1);
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct SimParams {
    id: i32,
    _pad: u32,
    window_origin: UVec2,
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
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    info!("adding internal water tex");
    let material = material.get(trigger.entity).unwrap();
    let material = water_terrain_materials.get(material.0.id()).unwrap();
    let flow_x = images.add(zeroed_storage_texture(WINDOW_DIM, WINDOW_DIM));
    let flow_y = images.add(zeroed_storage_texture(WINDOW_DIM, WINDOW_DIM));
    let outflow_scale = images.add(zeroed_storage_texture(WINDOW_DIM, WINDOW_DIM));
    let samples = buffers.add(ShaderBuffer::with_size(
        WaterSamples::min_size().get(),
        RenderAssetUsages::RENDER_WORLD,
    ));
    commands
        .spawn((
            DespawnOnExit(Screen::Gameplay),
            Readback::buffer(samples.clone()),
        ))
        .observe(store_water_samples);

    commands.insert_resource(WaterHeightTexture {
        water: material.extension.water.clone(),
        flow_x,
        flow_y,
        base_height: material.extension.base.clone(),
        outflow_scale,
        samples,
    });
}

fn zeroed_storage_texture(width: u32, height: u32) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &0.0f32.to_le_bytes(),
        TextureFormat::R32Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage |= TextureUsages::STORAGE_BINDING | TextureUsages::COPY_DST;
    image
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
                    TextureFormat::R32Float,
                    bevy::render::render_resource::StorageTextureAccess::ReadWrite,
                ),
                texture_storage_2d(
                    TextureFormat::R32Float,
                    bevy::render::render_resource::StorageTextureAccess::ReadWrite,
                ),
                texture_storage_2d(
                    TextureFormat::R32Float,
                    bevy::render::render_resource::StorageTextureAccess::ReadWrite,
                ),
                storage_buffer_read_only::<Displacements>(false),
                texture_storage_2d(
                    TextureFormat::R32Float,
                    bevy::render::render_resource::StorageTextureAccess::ReadOnly,
                ),
                texture_storage_2d(
                    TextureFormat::R32Float,
                    bevy::render::render_resource::StorageTextureAccess::ReadWrite,
                ),
                storage_buffer::<WaterSamples>(false),
            ),
        ),
    );
    let shader: Handle<Shader> = asset_server
        .load(AssetPath::from_path_buf(embedded_path!("water.wesl")).with_source("embedded"));
    let update_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        layout: vec![texture_bind_group_layout.clone()],
        shader,
        entry_point: Some(Cow::from("main")),
        shader_defs: vec![
            ShaderDefVal::UInt("WORKGROUP_SIZE".into(), WORKGROUP_SIZE),
            ShaderDefVal::UInt("MAX_DISPLACEMENTS".into(), MAX_DISPLACEMENTS as u32),
            ShaderDefVal::UInt("EDGE_FADE_CELLS".into(), EDGE_FADE_CELLS),
        ],
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
    bind_group: Option<Res<WaterBindGroup>>,
    pipeline: Res<WaterRenderPipeline>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    gpu_buffers: Res<RenderAssets<GpuShaderBuffer>>,
    water_images: If<Res<WaterHeightTexture>>,
    mut displacement: If<ResMut<DisplacementBuffer>>,
    displacement_main: If<Res<DisplacementBufferMain>>,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    render_queue: Res<RenderQueue>,
) {
    let uploaded = displacement.buffer.get_mut();
    uploaded.count = displacement_main.buffer.len() as u32;
    uploaded.items[..displacement_main.buffer.len()].copy_from_slice(&displacement_main.buffer);
    displacement
        .buffer
        .write_buffer(&render_device, &render_queue);

    if bind_group.is_some() {
        return;
    }
    let (Some(water), Some(flow_x), Some(flow_y), Some(base), Some(outflow_scale), Some(samples)) = (
        gpu_images.get(&water_images.water),
        gpu_images.get(&water_images.flow_x),
        gpu_images.get(&water_images.flow_y),
        gpu_images.get(&water_images.base_height),
        gpu_images.get(&water_images.outflow_scale),
        gpu_buffers.get(&water_images.samples),
    ) else {
        return;
    };

    let bind_group = render_device.create_bind_group(
        None,
        &pipeline_cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((
            &water.texture_view,
            &flow_x.texture_view,
            &flow_y.texture_view,
            displacement.buffer.binding().unwrap(),
            &base.texture_view,
            &outflow_scale.texture_view,
            samples.buffer.as_entire_buffer_binding(),
        )),
    );
    commands.insert_resource(WaterBindGroup(bind_group));
}

/// The textures the simulation reads and writes, all window textures, and
/// the buffer the per-object samples are read back through. Present once the
/// water clipmap exists.
#[derive(Resource, Clone, ExtractResource)]
#[extract_app(RenderApp)]
pub struct WaterHeightTexture {
    pub water: Handle<Image>,
    pub flow_x: Handle<Image>,
    pub flow_y: Handle<Image>,
    pub base_height: Handle<Image>,
    pub outflow_scale: Handle<Image>,
    pub samples: Handle<ShaderBuffer>,
}

#[derive(Resource, Clone, ExtractResource)]
#[extract_app(RenderApp)]
struct DisplacementBufferMain {
    buffer: Vec<Displacement>,
}

#[derive(ShaderType, Clone, Copy, Default)]
struct Displacement {
    center: Vec2,
    radius: f32,
    push: f32,
    entity: UVec2,
}

#[derive(ShaderType)]
struct Displacements {
    count: u32,
    items: [Displacement; MAX_DISPLACEMENTS],
}

#[derive(ShaderType, Clone, Copy, Default)]
struct WaterSample {
    entity: UVec2,
    depth: f32,
    ground: f32,
}

#[derive(ShaderType)]
struct WaterSamples {
    count: u32,
    items: [WaterSample; MAX_DISPLACEMENTS],
}

impl Default for WaterSamples {
    fn default() -> Self {
        Self {
            count: 0,
            items: [WaterSample::default(); MAX_DISPLACEMENTS],
        }
    }
}

#[derive(Resource)]
struct DisplacementBuffer {
    buffer: StorageBuffer<Displacements>,
}

/// Whether the window textures, the sim bind group and every staging buffer
/// are on the GPU, so a window shift can be applied the frame it is queued.
/// Shared by the main and the render world.
#[derive(Resource, Clone, Default)]
pub struct ChunkGpuReady(Arc<AtomicBool>);

impl ChunkGpuReady {
    pub fn is_ready(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

fn mark_chunks_ready(
    ready: Res<ChunkGpuReady>,
    bind_group: Option<Res<WaterBindGroup>>,
    staging: Option<Res<StagingBuffers>>,
    gpu_buffers: Res<RenderAssets<GpuShaderBuffer>>,
) {
    let all_there = bind_group.is_some()
        && staging.is_some_and(|s| s.buffers.iter().all(|b| gpu_buffers.get(b).is_some()));
    ready.0.store(all_there, Ordering::Relaxed);
}

#[derive(Resource, Default)]
struct ExtractedSwaps(Vec<ChunkSwap>);

fn extract_swaps(mut main_world: ResMut<MainWorld>, mut extracted: ResMut<ExtractedSwaps>) {
    if let Some(mut queue) = main_world.get_resource_mut::<SwapQueue>()
        && !queue.swaps.is_empty()
    {
        extracted.0.append(&mut queue.swaps);
    }
}

const CHUNK_BYTES_PER_ROW: u32 = CHUNK_DIM * 4;

fn chunk_extent() -> Extent3d {
    Extent3d {
        width: CHUNK_DIM,
        height: CHUNK_DIM,
        depth_or_array_layers: 1,
    }
}

fn chunk_layout_in_buffer() -> TexelCopyBufferLayout {
    TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(CHUNK_BYTES_PER_ROW),
        rows_per_image: Some(CHUNK_DIM),
    }
}

fn slot_region(texture: &Texture, slot: Origin3d) -> TexelCopyTextureInfo<'_> {
    TexelCopyTextureInfo {
        texture,
        mip_level: 0,
        origin: slot,
        aspect: TextureAspect::All,
    }
}

fn swap_chunks(
    mut render_context: RenderContext,
    mut swaps: ResMut<ExtractedSwaps>,
    water_images: Option<Res<WaterHeightTexture>>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    gpu_buffers: Res<RenderAssets<GpuShaderBuffer>>,
    mut zeros: Local<Option<Buffer>>,
) {
    if swaps.0.is_empty() {
        return;
    }
    let water_images = water_images.expect("chunk swap queued before the water textures exist");
    let texture = |handle: &Handle<Image>| {
        gpu_images
            .get(handle)
            .expect("chunk swap queued before the window textures are on the GPU")
            .texture
            .clone()
    };
    let water = texture(&water_images.water);
    let base = texture(&water_images.base_height);
    let flows = [
        texture(&water_images.flow_x),
        texture(&water_images.flow_y),
        texture(&water_images.outflow_scale),
    ];
    let device = render_context.render_device().clone();
    let zeros = zeros
        .get_or_insert_with(|| {
            device.create_buffer(&BufferDescriptor {
                label: Some("zero chunk"),
                size: u64::from(CHUNK_BYTES_PER_ROW) * u64::from(CHUNK_DIM),
                usage: BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        })
        .clone();

    for swap in swaps.0.drain(..) {
        let staging = gpu_buffers
            .get(&swap.staging)
            .expect("chunk swap queued before its staging buffer is on the GPU");
        debug!(leaving = ?swap.leaving, entering = ?swap.entering, "swapping chunk");
        let span = WINDOW_CHUNKS as i32;
        let slot = Origin3d {
            x: swap.entering.x.rem_euclid(span) as u32 * CHUNK_DIM,
            y: swap.entering.z.rem_euclid(span) as u32 * CHUNK_DIM,
            z: 0,
        };
        let upload = |contents: &[f32], label: &str| {
            device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(contents),
                usage: BufferUsages::COPY_SRC,
            })
        };
        let terrain = upload(&swap.terrain, "entering chunk terrain");
        let entering_water = upload(&swap.water, "entering chunk water");
        let encoder = render_context.command_encoder();
        encoder.copy_texture_to_buffer(
            slot_region(&water, slot),
            TexelCopyBufferInfo {
                buffer: &staging.buffer,
                layout: chunk_layout_in_buffer(),
            },
            chunk_extent(),
        );
        for (source, target) in [(&terrain, &base), (&entering_water, &water)]
            .into_iter()
            .chain(flows.iter().map(|flow| (&zeros, flow)))
        {
            encoder.copy_buffer_to_texture(
                TexelCopyBufferInfo {
                    buffer: source,
                    layout: chunk_layout_in_buffer(),
                },
                slot_region(target, slot),
                chunk_extent(),
            );
        }
    }
}
