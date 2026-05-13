---
title: Water implementation v1
created: 2026-05-09
tags:
  - water
---

# Water Simulation

## Overview

The water system implements a 2D shallow water simulation using a staggered MAC grid, running entirely on the GPU via compute shaders. The simulation covers a 204.8m x 204.8m area at 0.1m resolution (2048x2048 cells). Water height and flow are stored in textures that are also used directly by the terrain rendering shader, creating a tight coupling between simulation and visualization.

## Grid & Coordinate System

| Texture | Dimensions | Purpose |
|---------|------------|---------|
| `texture_a` / `texture_b` | 2048x2048 | Water height (ping-pong buffers) |
| `flow_x` | 2049x2048 | Horizontal flow on vertical edges |
| `flow_y` | 2048x2049 | Vertical flow on horizontal edges |

**World <-> Texture coordinate mapping:**
```
texture_coord = (world_coord * 10.0 + 1024.0).floor()
world_height = texture_value * 10.0
```

The offset of 1024 centers the grid at world origin. A texture value of 1.0 represents 10m of water height in world space.

## Architecture

```
CPU (Update)                           GPU (Render)
─────────────                         ────────────────────

collect_displacements                 WaterRenderNode
    |                                      |
    v                                      v
DisplacementBufferMain                 texture_a <---> texture_b
    (Vec<Vec4> on CPU)                      (ping-pong)
    |                                      |
    v                                      v
DisplacementBuffer                    flow_x, flow_y
    (StorageBuffer on GPU)                  (updated each frame)
    |                                      |
    +──────────+──────────────────────────+
               |
               v
    terrain.wgsl vertex shader
       samples latest height texture
       displaces mesh vertices
```

### Compute Pipeline

```
WaterSimPlugin
├── init_internal_textures (observer on TerrainMaterial add)
│   └── Creates: texture_a, texture_b, flow_x, flow_y
├── init_water_render (RenderStartup)
│   └── Creates: bind group layout, compute pipeline
├── prepare_water_bindgroups (Render)
│   └── Uploads displacement buffer, creates bind groups
└── WaterRenderNode (in render graph)
    ├── Pass 0: update_flows (sim.id = 0)
    │   └── Dispatch: 256x256 workgroups
    └── Pass 1: update_water_height (sim.id = 1)
        └── Dispatch: 256x256 workgroups
```

Render graph: `WaterRenderLabel -> CameraDriverLabel`

## Compute Shader (`water.wgsl`)

The shader has two entry points selected via push constants (`SimParams.id`):

### Pass 0: Update Flows
- Computes flow on edges based on height differences between adjacent cells
- Applies damping (x0.99) and gravity-like acceleration (dh x 0.1125)
- Adds displacement forces from objects in water:
  ```wgsl
  f += dot(dir, vec2f(1.0, 0.0)) * displacement_circle.w * 0.1 * 0.003
  ```
- Solid walls (zero flow) at domain borders

### Pass 1: Update Water Height
- Computes net inflow for each cell from surrounding edge flows
- Updates height: `new_height = old_height + net_flow`
- Net inflow = (fx_left - fx_right) + (fy_down - fy_up)

## Object Interaction

Objects with the `WaterDisplacement` component:
```rust
pub struct WaterDisplacement {
    pub radius: f32,    // Area of influence
    pub strength: f32,  // Force multiplier
}
```

The `collect_displacements` system:
1. For each object, samples water height at its position using `height_from_texture`
2. If object is below water surface (`transform.translation.y < h`):
   - Computes depth = `h - transform.translation.y`
   - Pushes `Vec4(x, z, radius, strength * depth)` to displacement buffer
   - Applies upward velocity: `velocity.0.y = depth * 1.0` (buoyancy)

The displacement buffer is uploaded to GPU each frame and bound as a read-only storage buffer at binding 4.

---

# Water Rendering

## Terrain Mesh

The terrain uses a **geomipmap/clipmap** approach (`TerrainHeightMapMesh`) for efficient LOD rendering:

- Central high-detail region (16x10 = 160 quads, each 0.1m)
- `rings` concentric lower-detail rings, each doubling the quad size
- Rings use subdivided quads with "triple" divided quads at LOD transition boundaries to avoid cracks

The mesh is static; vertex positions are displaced in the vertex shader based on the water height texture.

## Terrain Material (`TerrainMaterial`)

Extended material that adds a height texture to `StandardMaterial`:
```rust
#[derive(Asset, AsBindGroup, Debug, Clone, Reflect)]
pub struct TerrainMaterial {
    #[texture(100)]
    #[sampler(101)]
    pub height: Handle<Image>,
}
```

## Vertex Shader (`terrain.wgsl`)

For each vertex:
1. Transforms to world space
2. Samples height texture at `floor(world_position.xz * 5.0)` (equivalent to the simulation coordinate mapping)
3. Computes normal from 4-tap central difference:
   ```wgsl
   let n = vec3f(
       (L - R) * 100.0,    // Left/Right height difference
       1.0,
       (D - U) * 100.0     // Down/Up height difference
   );
   normal = normalize(n).yzw
   ```
4. Sets vertex position y-coordinate to `height * 10.0`
5. Computes tangent for normal mapping
6. Sets UV for tiling texture: `abs((world_position.xz/200.0) % 1.0)`

The scale factor of 100.0 for normal computation matches the 10.0 scale for height (1.0 texture value = 10m height, so gradient is ~10:1).

## Texture Swapping

The `swap_textures` system ensures terrain always renders the newest water height:
```rust
// In render/mod.rs
fn swap_textures(...) {
    if a.extension.height == textures.texture_a {
        a.extension.height = textures.texture_b.clone();
    } else {
        a.extension.height = textures.texture_a.clone();
    }
}
```

This toggles the material's height texture reference each frame to match whichever texture the simulation just wrote to.

---

## Initialization Flow

1. **Heightmap creation** (`heightmap.rs`): Generates a 2048x2048 terrain heightmap using layered noise (`mountain_noise`)
2. **Terrain spawn** (`gameplay/mod.rs`): Creates the mesh and material with the heightmap texture
3. **Water textures** (`init_internal_textures` observer): Triggered when `TerrainMaterial` is added:
   - Clones the terrain heightmap as initial water height (flat water at height 0)
   - Creates `flow_x` (2049x2048) and `flow_y` (2048x2049) textures initialized to 0.0
   - All textures use `R32Float` format with `STORAGE_BINDING | COPY_DST` usage
4. **Compute pipeline** (`init_water_render`): Creates bind group layout and compute pipeline for `water.wgsl`
5. **Bind groups** (`prepare_water_bindgroups`): Creates two bind groups (for ping-pong) with all 5 bindings

---

## Parameters & Tuning

| Parameter | Location | Effect |
|-----------|----------|--------|
| `radius` | `WaterDisplacement` component | Object's area of influence on water |
| `strength` | `WaterDisplacement` component | Force multiplier for water displacement |
| `0.1125` | `water.wgsl` | Flow acceleration from height gradient |
| `0.99` | `water.wgsl` | Flow damping coefficient |
| `0.1 * 0.003` | `water.wgsl` | Displacement force scaling |
| `1.0` | `collect_displacements` | Buoyancy velocity multiplier |
