#import bevy_pbr::mesh_functions
#import bevy_pbr::pbr_fragment::pbr_input_from_standard_material
#import bevy_pbr::view_transformations::position_world_to_clip

#ifdef MESHLET_MESH_MATERIAL_PASS
#import bevy_pbr::meshlet_visibility_buffer_resolve::VertexOutput
#else ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::{Vertex, VertexOutput, FragmentOutput}
#import bevy_pbr::pbr_deferred_functions::deferred_output;
#else
#import bevy_pbr::forward_io::{Vertex, VertexOutput, FragmentOutput}
#import bevy_pbr::pbr_functions::main_pass_post_lighting_processing
#endif

// 9 heightmap textures for each tile in 3x3 grid
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var height_00: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var height_00_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var height_01: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var height_01_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var height_02: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var height_02_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var height_10: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var height_10_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var height_11: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(109) var height_11_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var height_12: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(111) var height_12_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(112) var height_20: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(113) var height_20_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(114) var height_21: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(115) var height_21_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(116) var height_22: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(117) var height_22_sampler: sampler;

// Tile configuration
const TILE_SIZE_WORLD: f32 = 102.4;
const TILE_SIZE_TEX: i32 = 1024;

// Get the texture for a grid position
// grid_z: -1 (top), 0 (middle), 1 (bottom)
// grid_x: -1 (left), 0 (center), 1 (right)
fn get_height_texture(grid_x: i32, grid_z: i32) -> texture_2d<f32> {
    let row = (grid_z + 1);
    let col = (grid_x + 1);
    let index = row * 3 + col;
    
    if index == 0 { return height_00; }
    if index == 1 { return height_01; }
    if index == 2 { return height_02; }
    if index == 3 { return height_10; }
    if index == 4 { return height_11; }
    if index == 5 { return height_12; }
    if index == 6 { return height_20; }
    if index == 7 { return height_21; }
    return height_22;
}

fn get_height_sampler(grid_x: i32, grid_z: i32) -> sampler {
    let row = (grid_z + 1);
    let col = (grid_x + 1);
    let index = row * 3 + col;
    
    if index == 0 { return height_00_sampler; }
    if index == 1 { return height_01_sampler; }
    if index == 2 { return height_02_sampler; }
    if index == 3 { return height_10_sampler; }
    if index == 4 { return height_11_sampler; }
    if index == 5 { return height_12_sampler; }
    if index == 6 { return height_20_sampler; }
    if index == 7 { return height_21_sampler; }
    return height_22_sampler;
}

// Calculate grid position from world position
fn get_grid_position(world_pos: vec2f) -> vec2i {
    let gx = i32(floor((world_pos.x + 51.2) / 102.4));
    let gz = i32(floor((world_pos.y + 51.2) / 102.4));
    // Clamp to valid grid range
    let gx = clamp(gx, -1, 1);
    let gz = clamp(gz, -1, 1);
    return vec2i(gx, gz);
}

// Get local texture coordinates within a tile
fn get_tile_uv(world_pos: vec2f) -> vec2i {
    let grid_pos = get_grid_position(world_pos);
    
    // Calculate world position of tile corner
    let tile_x_min = f32(grid_pos.x) * 102.4 - 51.2;
    let tile_z_min = f32(grid_pos.y) * 102.4 - 51.2;
    
    // Convert to texture coordinates (10.0 pixels per world unit)
    let local_x = (world_pos.x - tile_x_min) * 10.0;
    let local_y = (world_pos.y - tile_z_min) * 10.0;
    
    return vec2i(i32(local_x), i32(local_y));
}

fn get_height(vertex_position_world: vec2f) -> vec4f {
    let grid_pos = get_grid_position(vertex_position_world);
    let uv = get_tile_uv(vertex_position_world);
    
    // Check bounds
    if uv.x < 0 || uv.x >= TILE_SIZE_TEX || uv.y < 0 || uv.y >= TILE_SIZE_TEX {
        // Fallback to center tile height
        let h = textureSample(height_11, height_11_sampler, vec2f(fract(float(uv.x) / float(TILE_SIZE_TEX)), fract(float(uv.y) / float(TILE_SIZE_TEX)))).r;
        return vec4f(h, 0.0, 1.0, 0.0);
    }
    
    let tex = get_height_texture(grid_pos.x, grid_pos.y);
    let sampler = get_height_sampler(grid_pos.x, grid_pos.y);
    let h: f32 = textureLoad(tex, uv, 0).r;

    let L = textureLoad(tex, uv + vec2i(-1, 0), 0).r;
    let R = textureLoad(tex, uv + vec2i(1, 0), 0).r;
    let D = textureLoad(tex, uv + vec2i(0, 1), 0).r;
    let U = textureLoad(tex, uv + vec2i(0, -1), 0).r;
    let n = vec3f(
        (L - R) * 1.0,
        1.0,
        (D - U) * 1.0
    );

    return vec4f(h, normalize(n));
}

fn wrap(x: vec2f) -> vec2f {
    return fract(x + ceil(abs(x)));
}

@vertex
fn vertex(vertex: Vertex, @builtin(vertex_index) idx: u32) -> VertexOutput {
    var out: VertexOutput;
    let model = mesh_functions::get_world_from_local(vertex.instance_index);
    out.world_position = model * vec4<f32>(vertex.position, 1.0);
    
    let height = get_height(out.world_position.xz);
    out.world_position.y = height.x;

    #ifdef MESHLET_MESH_MATERIAL_PASS
    #else ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
        out.world_normal = height.yzw;
    #else ifdef PREPASS_PIPELINE
    #else
        out.world_normal = height.yzw;
        if abs(dot(height.xzw, vec3(0,1,0))) < 0.899 {
            out.world_tangent = vec4(normalize(cross(vec3(0,1,0), height.yzw)), 1.0) * 0.11;
        } else {
            out.world_tangent = vec4(normalize(cross(vec3(1,0,0), height.yzw)), 1.0) * 0.11;
        }
    #endif

    out.position = position_world_to_clip(out.world_position.xyz);

    // Calculate UV for albedo/texturing
    // Map world position to [0,1] range across all tiles
    let total_size = 3.0 * TILE_SIZE_WORLD;
    let offset = total_size / 2.0;
    out.uv = (out.world_position.xz + offset) / total_size;

    return out;
}
