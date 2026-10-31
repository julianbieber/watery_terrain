---
title: Water implementation v1
created: 2026-05-09
tags:
  - water
---

# Water Simulation

## Overview

The water system is a 2D shallow-water simulation on a staggered grid, run
entirely on the GPU in compute shaders. It covers 204.8 m × 204.8 m at 0.1 m
resolution (2048 × 2048 cells), centred on the world origin. The water depth
texture it updates in place is the same texture the water material draws.

## Grid & Coordinate System

`src/grid.rs` defines the grid once: `DIM = 2048` cells per side,
`CELLS_PER_METER = 10`. The shaders get the cell density as the shader def
`constants::CELLS_PER_METER` through `package::grid` (`src/grid.wesl`) and take
the size from `textureDimensions`, so neither side repeats the numbers.

| Texture | Dimensions | Purpose |
|---------|------------|---------|
| `water` | 2048×2048 | Water depth in metres, updated in place |
| `base_height` | 2048×2048 | Terrain height in metres, static |
| `flow_x` | 2049×2048 | Flow across the vertical cell edges |
| `flow_y` | 2048×2049 | Flow across the horizontal cell edges |
| `outflow_scale` | 2048×2048 | Per-cell fraction of its outflow it can afford |

**World ↔ cell mapping:**
```
cell = world_xz * CELLS_PER_METER + DIM / 2
```
Texel `i` holds the height at the corner `(i - DIM/2) / CELLS_PER_METER`. The
terrain collider puts its samples on those same corners.

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
   at the edge's midpoint. Border edges are walls (zero flow).
1. **Outflow scale** — per cell, `min(1, water / total outflow)`.
2. **Limit flows** — every edge's flow is multiplied by the outflow scale of
   the cell it drains, so no cell gives away more than it holds. This keeps
   the total volume constant.
3. **Heights** — each cell adds its net inflow, in place.
4. **Sample** (once per frame) — records water depth and ground height under
   every object for the CPU.

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

- **Height**: bilinear between the four surrounding texels, loads past the
  edge repeat the edge.
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
| `DIM`, `CELLS_PER_METER` | `grid.rs` | Grid size and resolution |
| `STEP_SECONDS`, `MAX_STEPS_PER_FRAME` | `water_sim/mod.rs` | Sim rate, catch-up limit |
| `acceleration` (0.118) | `water.wesl` | Flow gained per step from a height difference |
| `damping` (0.99) | `water.wesl` | Flow kept from the previous step |
| `DRY_DEPTH_MM` | `water_sim/mod.rs` | Depth treated as dry |
| `WATER_DENSITY_RATIO`, `WATER_DRAG_PER_SECOND` | `water_sim/mod.rs` | Buoyancy |
| `radius`, `strength` | `WaterDisplacement` | Object's reach and push |
| `NORMAL_MAP_STRENGTH`, `TEXTURE_REPEAT_METRES` | `heightfield.wesl` | Surface detail |
| `BAND_TOPS` | `terrain.wesl` | Material height bands |
