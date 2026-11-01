---
title: Water implementation v1
created: 2026-05-09
tags:
  - water
---

# Water Simulation

## Overview

The water system is a 2D shallow-water simulation on a staggered grid, run
entirely on the GPU in compute shaders. The world is 6 × 6 chunks of
1024 × 1024 cells at 0.1 m (614.4 m a side), centred on the world origin. Only
the 3 × 3 chunk window around the camera (307.2 m a side) is drawn, collides
and is simulated. The water depth texture the sim updates in place is the same
texture the water material draws.

## Grid & Coordinate System

`src/grid.rs` defines the grid once: `CHUNK_DIM = 1024` cells per chunk side,
`WORLD_CHUNKS = 6`, `WINDOW_CHUNKS = 3`, `CELLS_PER_METER = 10`. The shaders
get them as shader defs through `package::grid` (`src/grid.wesl`), so neither
side repeats the numbers.

Every window texture is 3072 × 3072 (`WINDOW_DIM`) and holds the nine window
chunks:

| Texture | Purpose |
|---------|---------|
| `water` | Water depth in metres, updated in place |
| `base_height` | Terrain height in metres |
| `flow_x` | Flow across each cell's −x edge |
| `flow_y` | Flow across each cell's −z edge |
| `outflow_scale` | Per-cell fraction of its outflow it can afford |

**World ↔ cell ↔ texel mapping:**
```
world_cell = world_xz * CELLS_PER_METER + WORLD_CHUNKS * CHUNK_DIM / 2
chunk      = world_cell / CHUNK_DIM
texel      = world_cell mod WINDOW_DIM
```
A chunk's slot in the textures is its coordinate mod 3 on each axis, so the
window origin alone says where every window chunk is, and a neighbour across a
chunk seam is the next texel. Shaders clamp each cell to the window before
mapping it, so reads past the window repeat its edge. World cell `c` holds the
height at the corner `(c - WORLD_CHUNKS * CHUNK_DIM / 2) / CELLS_PER_METER`;
each chunk's terrain collider puts its samples on those same corners and shares
its seam row and column with its neighbours.

## Chunk window

`ChunkLayout` holds the window's lower chunk. Each frame `shift_window` checks,
per axis, whether the camera is more than `SHIFT_OVERLAP_METRES` (10 m) outside
the centre chunk; if so the window re-centres on the camera's chunk, clamped to
the world.

Each chunk that leaves hands its slot to the chunk entering into the same slot;
the chunks that stay are never touched. For each such swap, in one command
encoder before the sim and before the frame is drawn:

1. the leaving chunk's water is copied into a staging buffer,
2. the entering chunk's terrain and water are copied into the slot,
3. the slot's flows are zeroed.

The staging buffer is read back to the CPU and stored in `WorldChunks`, which
keeps every chunk's terrain and its last water. The window does not move again
until every readback is in, so a chunk never re-enters before its water is
back. A chunk outside the window is not simulated, and re-enters with zero
flow.

Physics objects outside the window are `Parked`: their rigid body and collider
are disabled and they are hidden, until the window covers them again. Only the
window chunks' colliders are enabled.

## Architecture

```
Main world (Update / FixedUpdate)        Render world
─────────────────────────────────        ────────────────────────────

count_sim_steps  ── WaterSimSteps ──►    run_water_sim
collect_displacements                       steps × (flows, outflow scale,
   └─ DisplacementBufferMain ───────►          limit flows, heights)
                                            then sample_water once
WaterAtObjects  ◄── Readback ──────────     └─ WaterSamples buffer
   ├─ collect_displacements (push)
   └─ apply_buoyancy (FixedUpdate)
```

### Time step

The sim runs at a fixed 60 Hz. `count_sim_steps` accumulates frame time and
hands the render world the number of whole steps due, at most 4 per frame;
time beyond that is dropped.

### Compute passes (`src/water_sim/water.wesl`)

One pipeline, the pass selected by an immediate (`SimParams.id`):

0. **Flows** — each interior edge: `f = f * damping + dh * acceleration`,
   with `dh` the difference in water surface (ground + water) clamped to the
   water in the source cell, plus the push of the nearest submerged object
   at the edge's midpoint. Within `EDGE_FADE_CELLS` (100) cells of the
   window's border the flow is scaled down linearly, to zero on the border.
1. **Outflow scale** — per cell, `min(1, water / total outflow)`.
2. **Limit flows** — every edge's flow is multiplied by the outflow scale of
   the cell it drains, so no cell gives away more than it holds. This keeps
   the total volume inside the window constant.
3. **Heights** — each cell adds its net inflow, in place.
4. **Sample** (once per frame) — records water depth and ground height under
   every object for the CPU; zero outside the window.

## Object Interaction

```rust
pub struct WaterDisplacement {
    pub radius: f32,    // Area of influence
    pub strength: f32,  // Push multiplier
}
```

Up to 64 objects are uploaded each frame with their entity id. Pass 4 samples
the water under each; the result is read back with Bevy's `Readback` into
`WaterAtObjects`, keyed by entity, a few frames behind the object's position.

From that sample, the submerged fraction is `(depth - bottom above ground) / (2·radius)`,
clamped to [0, 1]. Water at or below `DRY_DEPTH_MM` (10 mm) counts as dry.

- **Push on the water**: `strength × submerged fraction`, radially outward
  inside the object's radius.
- **Buoyancy** (`apply_buoyancy`, FixedUpdate): an upward acceleration of
  `|g| × 2 × submerged fraction` (water twice the bodies' density) plus a
  vertical drag of `2 /s × submerged fraction`, through avian's `Forces`.

---

# Water Rendering

## Clipmap Mesh

Terrain and water each use a clipmap mesh (`TerrainHeightMapMesh`): a central
region of 160 × 160 quads of 0.05 m and 5 rings, each doubling the quad size,
with triple-split quads at the LOD seams. The mesh follows the camera in 10 m
steps; vertex heights come from the textures in the vertex shader.

## Shared heightfield code (`src/render/heightfield.wesl`)

- **Height**: bilinear between the four surrounding cells, each clamped to the
  window, so loads past the window repeat its edge.
- **Normal**: central differences one cell either side,
  `normalize((h(x-1) - h(x+1)) · 5, 1, (h(z-1) - h(z+1)) · 5)`, the 5 being
  `CELLS_PER_METER / 2`.
- **Tangent**: the surface direction along +x (where u grows), `w = 1`, length
  0.11 to damp the normal maps evenly.
- **UV**: `world_xz / 51.2 m`.

## Terrain

`terrain.wesl` displaces the vertex to the terrain height and picks the atlas
column by height: sand below 4 m, grass below 7.5 m, rock below 12 m, snow
above.

## Water

`water.wesl` draws the water surface (ground + water). Where the water is at
or below `DRY_DEPTH_MM` it draws 0.1 m below the ground instead, so the
terrain hides it.

---

## Parameters & Tuning

| Parameter | Location | Effect |
|-----------|----------|--------|
| `CHUNK_DIM`, `WORLD_CHUNKS`, `WINDOW_CHUNKS`, `CELLS_PER_METER` | `grid.rs` | Chunk and world size, window size, resolution |
| `SHIFT_OVERLAP_METRES` | `chunks.rs` | How far past the centre chunk the camera goes before the window follows |
| `EDGE_FADE_CELLS` | `water_sim/mod.rs` | Width of the flow fade at the window border |
| `STEP_SECONDS`, `MAX_STEPS_PER_FRAME` | `water_sim/mod.rs` | Sim rate, catch-up limit |
| `acceleration` (0.118) | `water.wesl` | Flow gained per step from a height difference |
| `damping` (0.99) | `water.wesl` | Flow kept from the previous step |
| `DRY_DEPTH_MM` | `water_sim/mod.rs` | Depth treated as dry |
| `WATER_DENSITY_RATIO`, `WATER_DRAG_PER_SECOND` | `water_sim/mod.rs` | Buoyancy |
| `radius`, `strength` | `WaterDisplacement` | Object's reach and push |
| `NORMAL_MAP_STRENGTH`, `TEXTURE_REPEAT_METRES` | `heightfield.wesl` | Surface detail |
| `BAND_TOPS` | `terrain.wesl` | Material height bands |
