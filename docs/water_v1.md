---
title: Water implementation v1
created: 2026-05-09
tags:
  - water
---

# Water Simulation

This crate is being built as a library for simulating and drawing water on
terrain, not as a game. The binary is a demo scene: its spheres are plain shapes
that show the simulation working.

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
| `cell_scratch` | Per-cell fraction of its outflow it can afford, then its depth change of the last step |

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
collect_displacements                       steps × (flows, objects, outflow
   └─ DisplacementBufferMain ───────►          scale, limit flows, heights)
                                            then sample_water once
WaterAtObjects  ◄── Readback ──────────     └─ WaterSamples buffer
   ├─ collect_displacements (drag)
   └─ apply_water_forces (FixedUpdate)
```

### Time step

The sim runs at a fixed 60 Hz. `count_sim_steps` accumulates frame time and
hands the render world the number of whole steps due, at most 4 per frame;
time beyond that is dropped.

### Compute passes (`src/water_sim/water.wesl`)

One pipeline, the pass selected by an immediate (`SimParams.id`):

Flow is stored as depth moved per step, `f = q·dt/dx` for the discharge `q` in
m²/s, so the outflow limit and height passes work in plain depth.

0. **Flows** — each interior edge, as an inertial shallow-water update with
   semi-implicit Manning friction:
   ```
   h = max(surface_a, surface_b) - max(ground_a, ground_b)
   f = (f + g·dt²/dx² · min(h, h_max) · drop + ν · rate_difference)
       / (1 + g·dt·n²·|q| / h^(7/3))
   ```
   clamped to critical flow (`|q| ≤ h·√(g·min(h, h_max))`). Wave speed is
   `√(g·h)` up to `h_max`, 0.92 m (3 m/s), where the stability limit caps it. `ν` is a bulk
   viscosity on the difference between the two cells' depth change of the last
   step: without it steep fronts ring at the cell scale and the ringing grows.
   Within `EDGE_FADE_CELLS` (100) cells of the window's border the flow is scaled
   down linearly, to zero on the border.
1. **Objects** — each submerged object, in a dispatch over its own footprint:
   see below.
2. **Outflow scale** — per cell, `min(1, water / total outflow)`, with a margin
   so rounding cannot drive a cell negative.
3. **Limit flows** — every edge's flow is multiplied by the outflow scale of
   the cell it drains, so no cell gives away more than it holds. This keeps
   the total volume inside the window constant.
4. **Heights** — each cell adds its net inflow, in place, and records it for the
   next step's viscosity.
5. **Sample** (once per frame) — records the water around every object for the
   CPU; zero outside the window.

## Object Interaction

```rust
pub struct WaterDisplacement {
    pub radius: f32,    // Radius of the sphere, in metres
}
```

Up to 64 objects are uploaded each frame with their entity id, height and
velocity. Pass 5 samples the water around each, just past its rim because the
object presses down the water under itself; the result is read back with Bevy's
`Readback` into `WaterAtObjects`, keyed by entity, a few frames behind the
object's position. Without wet water at the rim it records the column under the
centre. Water at or below `DRY_DEPTH_MM` (10 mm) counts as dry.

The submerged fraction is the sphere's immersed volume over its whole volume.

- **Displacement**: the column of water the sphere occupies adds to the surface
  gradient on the edges under it, like a hydrostatic pressure patch, so water
  leaves from under the object and piles up around it. Water only moves through
  flows, so volume is kept.
- **Drag on the water**: flow under the object is pulled towards the object's
  velocity by quadratic drag, over the object's footprint.
- **Buoyancy** (`apply_water_forces`, FixedUpdate): an upward acceleration of
  `|g| × 2 × submerged fraction` (water twice the bodies' density) plus a
  vertical drag of `2 /s × submerged fraction`, through avian's `Forces`.
- **Drag on the object**: the same quadratic drag on the difference to the
  water's velocity, so currents and waves carry the object.

---

# Water Rendering

## Clipmap Mesh

Terrain and water each use a clipmap mesh (`TerrainHeightMapMesh`): a central
region of 160 × 160 quads of 0.05 m and 5 rings, each doubling the quad size,
with triple-split quads at the LOD seams. Vertices are shared and carry their
level's quad size in their first UV coordinate.

The mesh transform sits on the camera. The vertex shaders snap the mesh to its
coarsest quad size, 1.6 m, so every ring's vertices keep their world positions.
Over the outer quarter of each level, heights and normals blend towards the
next-coarser level, reaching it at the level's edge, so a ring changes level
without a jump. Vertex heights come from the textures in the vertex shader.

## Shared heightfield code (`src/render/heightfield.wesl`)

- **Height**: bilinear between the four surrounding cells, each clamped to the
  window, so loads past the window repeat its edge.
- **Normal**: central differences `s` cells either side,
  `normalize((h(x-s) - h(x+s)) · 5/s, 1, (h(z-s) - h(z+s)) · 5/s)`, the 5 being
  `CELLS_PER_METER / 2` and `s` the spacing of the surface the level draws, at
  least one cell.
- **Tangent**: the surface direction along +x (where u grows), `w = 1`, length
  0.11 to damp the normal maps evenly.
- **UV**: `world_xz / 51.2 m`.

## Terrain

The world is read at startup from the terrain directory `assets/terrain`, relative
to the working directory: the height field and the solved water depth of each
chunk. Edit it with `mise exec -- watershed_editor assets/terrain`; the sim reads
what was last saved. Startup panics if the directory is missing or unreadable, or
its grid is not the one in `grid.rs`.

`terrain.wesl` displaces the vertex to the terrain height and picks the atlas
column by height: sand below 12 m, grass below 21 m, rock below 27 m, snow
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
| `STEPS_PER_SECOND`, `MAX_STEPS_PER_FRAME` | `water_sim/mod.rs` | Sim rate, catch-up limit |
| `MANNING_ROUGHNESS` (0.03) | `water.wesl` | Bed friction, strongest in shallow water |
| `BULK_VISCOSITY` (0.1) | `water.wesl` | Damping of cell-scale ringing; with `MAX_WAVE_SPEED` it must stay inside the stability limit |
| `MAX_WAVE_SPEED` (half a cell per step) | `water.wesl` | Wave speed cap, and so `h_max` |
| `DRY_DEPTH_MM` | `water_sim/mod.rs` | Depth treated as dry |
| `WATER_DENSITY_RATIO`, `WATER_DRAG_PER_SECOND`, `SPHERE_DRAG_COEFFICIENT` | `water_sim/mod.rs` | Buoyancy and drag |
| `radius` | `WaterDisplacement` | Size of the sphere |
| `NORMAL_MAP_STRENGTH`, `TEXTURE_REPEAT_METRES` | `heightfield.wesl` | Surface detail |
| `BAND_TOPS` | `terrain.wesl` | Material height bands |
