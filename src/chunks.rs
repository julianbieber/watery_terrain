//! The chunked world: every chunk kept in memory, the square window of chunks
//! that is drawn and collides and whose centre chunk is simulated, and moving
//! that window under the camera without touching the chunks that stay in it.

use std::{collections::HashMap, path::Path, sync::Arc};

use avian3d::prelude::{ColliderDisabled, RigidBody, RigidBodyDisabled};
use bevy::{
    pbr::ExtendedMaterial,
    prelude::*,
    render::{
        RenderApp,
        extract_resource::ExtractResource,
        gpu_readback::{Readback, ReadbackComplete},
        storage::ShaderBuffer,
    },
    tasks::ComputeTaskPool,
};
use watershed::{FieldRole, TerrainDir};

use crate::{
    grid::{
        CELL_SIZE, CHUNK_DIM, ChunkCoord, WINDOW_CHUNKS, WORLD_CHUNKS, WORLD_DIM, chunk_of,
        chunk_origin, texel_of,
    },
    heightmap::Heightmap,
    render::clipmap::{FollowTerrainMarker, TerrainMaterial, WaterTerrainMaterial},
    screens::Screen,
    water_sim::ChunkGpuReady,
};

/// How far, in metres, the camera has to leave the centre chunk on an axis
/// before the window follows it on that axis.
pub const SHIFT_OVERLAP_METRES: f32 = 10.0;

/// Staging buffers for the water of leaving chunks: one per window chunk, so
/// a single shift can never run out.
pub const STAGING_BUFFERS: u32 = WINDOW_CHUNKS * WINDOW_CHUNKS;

const _: () = assert!(
    STAGING_BUFFERS <= u32::BITS,
    "StagingBuffers::in_use_mask has one bit per staging buffer"
);

/// Bytes of one chunk's water.
pub const CHUNK_BYTES: u64 = (CHUNK_DIM as u64) * (CHUNK_DIM as u64) * 4;

/// Moves the window of active chunks with the camera and keeps the physics
/// objects outside it inactive.
pub struct ChunkPlugin;

impl Plugin for ChunkPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SwapQueue>();
        app.add_systems(
            Update,
            (
                shift_window
                    .run_if(resource_exists::<ChunkLayout>)
                    .run_if(resource_exists::<StagingBuffers>)
                    .run_if(gpu_ready),
                park_objects.run_if(resource_exists::<ChunkLayout>),
            )
                .chain()
                .run_if(in_state(Screen::Gameplay)),
        );
    }
}

fn gpu_ready(ready: Option<Res<ChunkGpuReady>>) -> bool {
    ready.is_some_and(|r| r.is_ready())
}

/// One chunk's terrain and water heights, row after row with rows along world
/// z, shared without copying with the swaps that upload them.
#[derive(Clone)]
pub struct Chunk {
    pub terrain: Arc<[f32]>,
    pub water: Arc<[f32]>,
}

impl Chunk {
    fn terrain_at(&self, local: UVec2) -> f32 {
        self.terrain[(local.y * CHUNK_DIM + local.x) as usize]
    }

    fn water_at(&self, local: UVec2) -> f32 {
        self.water[(local.y * CHUNK_DIM + local.x) as usize]
    }
}

const TERRAIN_DIR: &str = "assets/terrain";

/// Takes the height field as terrain and the solved water depth as water.
///
/// Panics if the chunk cannot be read, or the directory carries no height field
/// or no solved water.
pub fn load_chunk(dir: &TerrainDir, coord: ChunkCoord) -> Chunk {
    let chunk = UVec2::new(coord.x as u32, coord.z as u32);
    let terrain = dir
        .load_chunk(chunk)
        .unwrap_or_else(|error| panic!("reading terrain chunk {coord:?}: {error}"));
    let height = terrain
        .field_with_role(FieldRole::Height)
        .expect("the terrain directory has no height field");
    let water = terrain
        .water()
        .expect("the terrain directory has no solved water");
    let cells = || (0..CHUNK_DIM).flat_map(|z| (0..CHUNK_DIM).map(move |x| (x, z)));
    Chunk {
        terrain: cells()
            .map(|(x, z)| height.value_at(x, z).expect("a cell inside the chunk"))
            .collect(),
        water: cells()
            .map(|(x, z)| water.depth_at(x, z).expect("a cell inside the chunk"))
            .collect(),
    }
}

fn open_terrain(path: &Path) -> TerrainDir {
    let dir = TerrainDir::open(path)
        .unwrap_or_else(|error| panic!("opening terrain {}: {error}", path.display()));
    let grid = dir.grid();
    assert_eq!(
        (grid.chunk_size(), grid.chunks()),
        (UVec2::splat(CHUNK_DIM), UVec2::splat(WORLD_CHUNKS)),
        "terrain {} does not match the world grid",
        path.display()
    );
    dir
}

/// Every chunk of the world, active or not. A chunk outside the window holds
/// the water it had when it left.
#[derive(Resource)]
pub struct WorldChunks {
    chunks: HashMap<ChunkCoord, Chunk>,
}

impl WorldChunks {
    /// Reads `assets/terrain` under the working directory.
    ///
    /// Panics if the directory is missing or unreadable, or its grid is not the
    /// world's.
    pub fn load() -> Self {
        let dir = open_terrain(Path::new(TERRAIN_DIR));
        let dir = &dir;
        let coords: Vec<ChunkCoord> = world_chunks().collect();
        let chunks = ComputeTaskPool::get().scope(|scope| {
            for coord in &coords {
                let coord = *coord;
                scope.spawn(async move { (coord, load_chunk(dir, coord)) });
            }
        });
        Self {
            chunks: chunks.into_iter().collect(),
        }
    }

    /// The chunk at `coord`. Panics if it lies outside the world.
    pub fn get(&self, coord: ChunkCoord) -> &Chunk {
        self.chunks
            .get(&coord)
            .unwrap_or_else(|| panic!("chunk {coord:?} lies outside the world"))
    }

    /// Replaces the water of the chunk at `coord`.
    ///
    /// Panics if `coord` lies outside the world or `water` is not one chunk's
    /// worth of values.
    pub fn store_water(&mut self, coord: ChunkCoord, water: Vec<f32>) {
        assert_eq!(water.len(), (CHUNK_DIM * CHUNK_DIM) as usize);
        let chunk = self
            .chunks
            .get_mut(&coord)
            .unwrap_or_else(|| panic!("chunk {coord:?} lies outside the world"));
        chunk.water = water.into();
    }

    /// Every chunk of the world.
    pub fn coords(&self) -> impl Iterator<Item = ChunkCoord> + '_ {
        self.chunks.keys().copied()
    }

    fn terrain_at_world_cell(&self, cell: UVec2) -> f32 {
        let cell = cell.min(UVec2::splat(WORLD_DIM - 1));
        let chunk = ChunkCoord::new((cell.x / CHUNK_DIM) as i32, (cell.y / CHUNK_DIM) as i32);
        self.get(chunk).terrain_at(cell % CHUNK_DIM)
    }

    /// The terrain heights a collider of the chunk at `coord` is built from:
    /// its own cells plus the first row and column of its +x, +z and diagonal
    /// neighbours, repeating its own last row and column where the world
    /// ends, so neighbouring colliders share their seam.
    pub fn collider_heights(&self, coord: ChunkCoord) -> Heightmap {
        let origin = chunk_origin(coord);
        let mut heights = Heightmap::zero(CHUNK_DIM + 1);
        for z in 0..=CHUNK_DIM {
            for x in 0..=CHUNK_DIM {
                let value = self.terrain_at_world_cell(origin + UVec2::new(x, z));
                heights.set(z, x, value);
            }
        }
        heights
    }

    /// The window textures' contents for `layout`: each window chunk's terrain
    /// (or water, when `water` is set) at the texels of its world cells.
    pub fn window_heightmap(&self, layout: &ChunkLayout, water: bool) -> Heightmap {
        let mut heights = Heightmap::zero(WINDOW_CHUNKS * CHUNK_DIM);
        for coord in layout.chunks() {
            let chunk = self.get(coord);
            let origin = chunk_origin(coord);
            for z in 0..CHUNK_DIM {
                for x in 0..CHUNK_DIM {
                    let local = UVec2::new(x, z);
                    let texel = texel_of(origin + local);
                    let value = if water {
                        chunk.water_at(local)
                    } else {
                        chunk.terrain_at(local)
                    };
                    heights.set(texel.y, texel.x, value);
                }
            }
        }
        heights
    }
}

fn world_chunks() -> impl Iterator<Item = ChunkCoord> {
    (0..WORLD_CHUNKS as i32)
        .flat_map(|z| (0..WORLD_CHUNKS as i32).map(move |x| ChunkCoord::new(x, z)))
}

/// Where the window lies: `origin` is its lower chunk. Every window chunk
/// lives at the texels of its world cells modulo the window size, so the
/// origin alone says where each one is. `centre` is the chunk the window
/// follows the camera with, the only one whose water is simulated; it is the
/// window's middle chunk except where the window meets the world's edge.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, ExtractResource)]
#[extract_app(RenderApp)]
pub struct ChunkLayout {
    pub origin: ChunkCoord,
    pub centre: ChunkCoord,
}

impl ChunkLayout {
    /// The window whose centre chunk is `chunk`, moved as little as needed to
    /// lie inside the world; a `chunk` outside the world is moved onto its
    /// nearest chunk.
    pub fn centred_on(chunk: ChunkCoord) -> Self {
        let last = (WORLD_CHUNKS - WINDOW_CHUNKS) as i32;
        let edge = WORLD_CHUNKS as i32 - 1;
        let half = WINDOW_CHUNKS as i32 / 2;
        Self {
            origin: ChunkCoord::new(
                (chunk.x - half).clamp(0, last),
                (chunk.z - half).clamp(0, last),
            ),
            centre: ChunkCoord::new(chunk.x.clamp(0, edge), chunk.z.clamp(0, edge)),
        }
    }

    /// The centre chunk as the unsigned chunk the shaders take.
    pub fn centre_uvec(&self) -> UVec2 {
        UVec2::new(self.centre.x as u32, self.centre.z as u32)
    }

    /// The window origin as the unsigned chunk the shaders take.
    pub fn origin_uvec(&self) -> UVec2 {
        UVec2::new(self.origin.x as u32, self.origin.z as u32)
    }

    /// Whether `coord` is one of the window's chunks.
    pub fn contains(&self, coord: ChunkCoord) -> bool {
        let span = WINDOW_CHUNKS as i32;
        (self.origin.x..self.origin.x + span).contains(&coord.x)
            && (self.origin.z..self.origin.z + span).contains(&coord.z)
    }

    /// The window's chunks.
    pub fn chunks(&self) -> impl Iterator<Item = ChunkCoord> + '_ {
        let span = WINDOW_CHUNKS as i32;
        (0..span).flat_map(move |z| {
            (0..span).map(move |x| ChunkCoord::new(self.origin.x + x, self.origin.z + z))
        })
    }

    /// The window to move to with the camera at `camera_xz`, or `None` when
    /// it stays. On each axis the centre follows the camera's chunk only once
    /// the camera is more than `SHIFT_OVERLAP_METRES` outside the current
    /// centre chunk on that axis.
    pub fn recentre(&self, camera_xz: Vec2) -> Option<Self> {
        let chunk_metres = CHUNK_DIM as f32 * CELL_SIZE;
        let world_low = -(WORLD_DIM as f32) / 2.0 * CELL_SIZE;
        let camera_chunk = chunk_of(camera_xz);
        let follow = |centre: i32, camera: f32, camera_chunk: i32| {
            let low = world_low + centre as f32 * chunk_metres;
            let high = low + chunk_metres;
            if camera < low - SHIFT_OVERLAP_METRES || camera > high + SHIFT_OVERLAP_METRES {
                camera_chunk
            } else {
                centre
            }
        };
        let centre = ChunkCoord::new(
            follow(self.centre.x, camera_xz.x, camera_chunk.x),
            follow(self.centre.z, camera_xz.y, camera_chunk.z),
        );
        let next = Self::centred_on(centre);
        (next != *self).then_some(next)
    }

    /// Every chunk that leaves when the window moves to `next`, paired with the
    /// chunk that enters into its slot.
    pub fn swaps_to(&self, next: &ChunkLayout) -> Vec<(ChunkCoord, ChunkCoord)> {
        next.chunks()
            .filter(|entering| !self.contains(*entering))
            .map(|entering| {
                let leaving = self
                    .chunks()
                    .find(|old| slot_of(*old) == slot_of(entering))
                    .expect("every slot holds one window chunk");
                (leaving, entering)
            })
            .collect()
    }
}

fn slot_of(coord: ChunkCoord) -> (i32, i32) {
    let span = WINDOW_CHUNKS as i32;
    (coord.x.rem_euclid(span), coord.z.rem_euclid(span))
}

/// One chunk replacing another in its slot of the window textures: the
/// leaving chunk's water is copied into `staging` before the entering
/// chunk's heights overwrite the slot.
#[derive(Clone)]
pub struct ChunkSwap {
    pub leaving: ChunkCoord,
    pub entering: ChunkCoord,
    pub terrain: Arc<[f32]>,
    pub water: Arc<[f32]>,
    pub staging: Handle<ShaderBuffer>,
}

/// Swaps queued by the main world for the render world to apply, each once.
#[derive(Resource, Default)]
pub struct SwapQueue {
    pub swaps: Vec<ChunkSwap>,
}

/// The buffers the leaving chunks' water is read back through. Bit `i` of
/// `in_use_mask` is set while buffer `i` awaits its readback.
#[derive(Resource, Clone, ExtractResource)]
#[extract_app(RenderApp)]
pub struct StagingBuffers {
    pub buffers: Vec<Handle<ShaderBuffer>>,
    pub in_use_mask: u32,
}

/// Static terrain collider of one chunk; disabled while the chunk is outside
/// the window.
#[derive(Component)]
pub struct ChunkCollider {
    pub coord: ChunkCoord,
}

/// The readback entity of one leaving chunk's water.
#[derive(Component)]
pub struct LeavingChunk {
    pub coord: ChunkCoord,
    pub staging_bit: u32,
}

/// A physics object outside the window: it neither moves, collides,
/// displaces water nor is drawn until the window covers it again.
#[derive(Component)]
pub struct Parked;

fn shift_window(
    mut commands: Commands,
    camera: Single<&Transform, With<FollowTerrainMarker>>,
    mut layout: ResMut<ChunkLayout>,
    world: Res<WorldChunks>,
    mut queue: ResMut<SwapQueue>,
    mut staging: ResMut<StagingBuffers>,
    colliders: Query<(Entity, &ChunkCollider)>,
    water_surfaces: Query<
        &MeshMaterial3d<ExtendedMaterial<StandardMaterial, WaterTerrainMaterial>>,
    >,
    terrain_surfaces: Query<&MeshMaterial3d<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
    mut water_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, WaterTerrainMaterial>>>,
    mut terrain_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, TerrainMaterial>>>,
) {
    if staging.in_use_mask != 0 {
        return;
    }
    let Some(next) = layout.recentre(camera.translation.xz()) else {
        return;
    };

    for (i, (leaving, entering)) in layout.swaps_to(&next).into_iter().enumerate() {
        let bit = 1 << i;
        let buffer = staging.buffers[i].clone();
        let chunk = world.get(entering);
        queue.swaps.push(ChunkSwap {
            leaving,
            entering,
            terrain: chunk.terrain.clone(),
            water: chunk.water.clone(),
            staging: buffer.clone(),
        });
        staging.in_use_mask |= bit;
        commands
            .spawn((
                DespawnOnExit(Screen::Gameplay),
                Readback::buffer(buffer),
                LeavingChunk {
                    coord: leaving,
                    staging_bit: bit,
                },
            ))
            .observe(store_leaving_water);
    }

    for (entity, collider) in &colliders {
        match (
            layout.contains(collider.coord),
            next.contains(collider.coord),
        ) {
            (true, false) => {
                commands.entity(entity).insert(ColliderDisabled);
            }
            (false, true) => {
                commands.entity(entity).remove::<ColliderDisabled>();
            }
            _ => {}
        }
    }

    *layout = next;
    let origin = next.origin_uvec();
    for surface in &water_surfaces {
        if let Some(mut material) = water_materials.get_mut(surface.id()) {
            material.extension.window_origin = origin;
        }
    }
    for surface in &terrain_surfaces {
        if let Some(mut material) = terrain_materials.get_mut(surface.id()) {
            material.extension.window_origin = origin;
        }
    }
}

/// Deactivates every physics object outside the window and reactivates the
/// parked ones the window covers again. Systems that must not see a parked
/// object for even one frame run after it.
pub fn park_objects(
    mut commands: Commands,
    layout: Res<ChunkLayout>,
    bodies: Query<(Entity, &Transform, Has<Parked>), (With<RigidBody>, Without<ChunkCollider>)>,
) {
    for (entity, transform, parked) in &bodies {
        let inside = layout.contains(chunk_of(transform.translation.xz()));
        if !inside && !parked {
            commands.entity(entity).insert((
                Parked,
                RigidBodyDisabled,
                ColliderDisabled,
                Visibility::Hidden,
            ));
        } else if inside && parked {
            commands
                .entity(entity)
                .remove::<(Parked, RigidBodyDisabled, ColliderDisabled)>()
                .insert(Visibility::Inherited);
        }
    }
}

fn store_leaving_water(
    readback: On<ReadbackComplete>,
    leaving: Query<&LeavingChunk>,
    mut world: ResMut<WorldChunks>,
    mut staging: ResMut<StagingBuffers>,
    mut commands: Commands,
) {
    let Ok(chunk) = leaving.get(readback.entity) else {
        return;
    };
    world.store_water(chunk.coord, bytemuck::pod_collect_to_vec(&readback.data));
    staging.in_use_mask &= !chunk.staging_bit;
    commands.entity(readback.entity).try_despawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::corner_of;

    const EDGE: i32 = WORLD_CHUNKS as i32 - 1;

    fn centre_of(chunk: ChunkCoord) -> Vec2 {
        corner_of(chunk_origin(chunk)) + Vec2::splat(CHUNK_DIM as f32 * CELL_SIZE / 2.0)
    }

    fn assert_slots_are_kept(before: &ChunkLayout, after: &ChunkLayout) {
        let swaps = before.swaps_to(after);
        let leaving: Vec<_> = before.chunks().filter(|c| !after.contains(*c)).collect();
        assert_eq!(swaps.len(), leaving.len());
        assert!(swaps.len() <= STAGING_BUFFERS as usize);
        for (out, into) in &swaps {
            assert!(before.contains(*out) && !after.contains(*out));
            assert!(after.contains(*into) && !before.contains(*into));
            assert_eq!(slot_of(*out), slot_of(*into));
        }
    }

    #[test]
    fn centred_on_clamps_to_the_world() {
        assert_eq!(
            ChunkLayout::centred_on(ChunkCoord::new(0, EDGE)).origin,
            ChunkCoord::new(0, EDGE - 4)
        );
        assert_eq!(
            ChunkLayout::centred_on(ChunkCoord::new(3, 3)).origin,
            ChunkCoord::new(1, 1)
        );
    }

    #[test]
    fn recentre_waits_for_the_overlap_on_each_axis() {
        let layout = ChunkLayout::centred_on(ChunkCoord::new(3, 3));
        let seam_x = corner_of(chunk_origin(ChunkCoord::new(4, 3))).x;
        let z = centre_of(ChunkCoord::new(3, 3)).y;
        assert_eq!(
            layout.recentre(Vec2::new(seam_x + SHIFT_OVERLAP_METRES - 1.0, z)),
            None
        );
        let moved = layout
            .recentre(Vec2::new(seam_x + SHIFT_OVERLAP_METRES + 1.0, z))
            .unwrap();
        assert_eq!(moved.origin, ChunkCoord::new(2, 1));
    }

    #[test]
    fn recentre_ignores_a_camera_near_another_seam() {
        let layout = ChunkLayout::centred_on(ChunkCoord::new(3, 3));
        let corner = corner_of(chunk_origin(ChunkCoord::new(3, 3)));
        assert_eq!(layout.recentre(corner + Vec2::new(0.0, 1.0)), None);
    }

    #[test]
    fn diagonal_shift_keeps_the_staying_chunks_in_their_slots() {
        let before = ChunkLayout::centred_on(ChunkCoord::new(2, 2));
        let after = ChunkLayout::centred_on(ChunkCoord::new(3, 3));
        assert_eq!(before.swaps_to(&after).len(), 9);
        assert_slots_are_kept(&before, &after);
    }

    #[test]
    fn far_jump_replaces_every_chunk() {
        let before = ChunkLayout::centred_on(ChunkCoord::new(1, 1));
        let after = before.recentre(centre_of(ChunkCoord::new(8, 8))).unwrap();
        assert_eq!(before.swaps_to(&after).len(), 25);
        assert_slots_are_kept(&before, &after);
    }

    #[test]
    fn clamped_window_moves_only_its_centre_to_the_camera() {
        let layout = ChunkLayout::centred_on(ChunkCoord::new(EDGE, EDGE));
        let moved = layout
            .recentre(centre_of(ChunkCoord::new(EDGE - 1, EDGE)))
            .unwrap();
        assert_eq!(moved.origin, layout.origin);
        assert_eq!(moved.centre, ChunkCoord::new(EDGE - 1, EDGE));
        assert!(layout.swaps_to(&moved).is_empty());
    }

    #[test]
    fn centre_outside_the_world_is_moved_onto_it() {
        assert_eq!(
            ChunkLayout::centred_on(ChunkCoord::new(-1, EDGE + 2)).centre,
            ChunkCoord::new(0, EDGE)
        );
    }

    #[test]
    fn clamped_window_stays_while_the_camera_is_inside_it() {
        let layout = ChunkLayout::centred_on(ChunkCoord::new(EDGE, EDGE));
        assert_eq!(
            layout.recentre(centre_of(ChunkCoord::new(EDGE, EDGE))),
            None
        );
    }

    fn committed_terrain() -> TerrainDir {
        open_terrain(Path::new(TERRAIN_DIR))
    }

    #[test]
    #[should_panic(expected = "opening terrain")]
    fn a_missing_terrain_directory_panics() {
        open_terrain(Path::new("no/such/terrain"));
    }

    #[test]
    fn a_chunk_holds_one_height_and_one_depth_per_cell() {
        let chunk = load_chunk(&committed_terrain(), ChunkCoord::new(2, 2));
        let cells = (CHUNK_DIM * CHUNK_DIM) as usize;
        assert_eq!(chunk.terrain.len(), cells);
        assert_eq!(chunk.water.len(), cells);
    }

    #[test]
    fn the_committed_terrain_holds_water_and_dry_land() {
        let chunk = load_chunk(&committed_terrain(), ChunkCoord::new(2, 2));
        assert!(chunk.water.iter().any(|depth| *depth > 1.0));
        assert!(chunk.water.contains(&0.0));
    }

    fn tiny_world() -> WorldChunks {
        let chunks = world_chunks()
            .map(|coord| {
                let terrain: Vec<f32> = (0..CHUNK_DIM * CHUNK_DIM)
                    .map(|i| (coord.x * 100 + coord.z) as f32 + i as f32 * 1e-7)
                    .collect();
                let water = vec![coord.x as f32; (CHUNK_DIM * CHUNK_DIM) as usize];
                (
                    coord,
                    Chunk {
                        terrain: terrain.into(),
                        water: water.into(),
                    },
                )
            })
            .collect();
        WorldChunks { chunks }
    }

    #[test]
    fn collider_heights_share_the_seam_with_the_neighbour() {
        let world = tiny_world();
        let left = world.collider_heights(ChunkCoord::new(1, 2));
        let right = world.collider_heights(ChunkCoord::new(2, 2));
        for z in 0..CHUNK_DIM {
            assert_eq!(left.get(z, CHUNK_DIM), right.get(z, 0));
        }
        let edge = world.collider_heights(ChunkCoord::new(EDGE, 0));
        assert_eq!(edge.get(3, CHUNK_DIM), edge.get(3, CHUNK_DIM - 1));
    }

    #[test]
    fn window_heightmap_puts_each_chunk_at_its_texels() {
        let world = tiny_world();
        let layout = ChunkLayout::centred_on(ChunkCoord::new(3, 2));
        let window = world.window_heightmap(&layout, false);
        for coord in layout.chunks() {
            let local = UVec2::new(5, 9);
            let texel = texel_of(chunk_origin(coord) + local);
            assert_eq!(
                window.get(texel.y, texel.x),
                world.get(coord).terrain_at(local)
            );
        }
    }
}
