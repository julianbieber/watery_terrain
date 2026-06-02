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

// 9 water textures for each tile in 3x3 grid
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var water_00: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var water_00_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var water_01: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var water_01_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var water_02: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var water_02_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var water_10: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var water_10_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var water_11: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(109) var water_11_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var water_12: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(111) var water_12_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(112) var water_20: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(113) var water_20_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(114) var water_21: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(115) var water_21_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(116) var water_22: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(117) var water_22_sampler: sampler;

// 9 base height textures for each tile in 3x3 grid
@group(#{MATERIAL_BIND_GROUP}) @binding(118) var base_00: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(119) var base_00_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(120) var base_01: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(121) var base_01_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(122) var base_02: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(123) var base_02_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(124) var base_10: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(125) var base_10_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(126) var base_11: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(127) var base_11_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(128) var base_12: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(129) var base_12_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(130) var base_20: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(131) var base_20_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(132) var base_21: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(133) var base_21_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(134) var base_22: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(135) var base_22_sampler: sampler;

// Tile configuration
const TILE_SIZE_WORLD: f32 = 102.4;
const TILE_SIZE_TEX: i32 = 1024;

// Get the water texture for a grid position
fn get_water_texture(grid_x: i32, grid_z: i32) -> texture_2d<f32> {
    let row = (grid_z + 1);
    let col = (grid_x + 1);
    let index = row * 3 + col;
    
    if index == 0 { return water_00; }
    if index == 1 { return water_01; }
    if index == 2 { return water_02; }
    if index == 3 { return water_10; }
    if index == 4 { return water_11; }
    if index == 5 { return water_12; }
    if index == 6 { return water_20; }
    if index == 7 { return water_21; }
    return water_22;
}

fn get_water_sampler(grid_x: i32, grid_z: i32) -> sampler {
    let row = (grid_z + 1);
    let col = (grid_x + 1);
    let index = row * 3 + col;
    
    if index == 0 { return water_00_sampler; }
    if index == 1 { return water_01_sampler; }
    if index == 2 { return water_02_sampler; }
    if index == 3 { return water_10_sampler; }
    if index == 4 { return water_11_sampler; }
    if index == 5 { return water_12_sampler; }
    if index == 6 { return water_20_sampler; }
    if index == 7 { return water_21_sampler; }
    return water_22_sampler;
}

// Get the base height texture for a grid position
fn get_base_texture(grid_x: i32, grid_z: i32) -> texture_2d<f32> {
    let row = (grid_z + 1);
    let col = (grid_x + 1);
    let index = row * 3 + col;
    
    if index == 0 { return base_00; }
    if index == 1 { return base_01; }
    if index == 2 { return base_02; }
    if index == 3 { return base_10; }
    if index == 4 { return base_11; }
    if index == 5 { return base_12; }
    if index == 6 { return base_20; }
    if index == 7 { return base_21; }
    return base_22;
}

fn get_base_sampler(grid_x: i32, grid_z: i32) -> sampler {
    let row = (grid_z + 1);
    let col = (grid_x + 1);
    let index = row * 3 + col;
    
    if index == 0 { return base_00_sampler; }
    if index == 1 { return base_01_sampler; }
    if index == 2 { return base_02_sampler; }
    if index == 3 { return base_10_sampler; }
    if index == 4 { return base_11_sampler; }
    if index == 5 { return base_12_sampler; }
    if index == 6 { return base_20_sampler; }
    if index == 7 { return base_21_sampler; }
    return base_22_sampler;
}

// Calculate grid position from world position
fn get_grid_position(world_pos: vec2f) -> vec2i {
    let gx = i32(floor((world_pos.x + 51.2) / 102.4));
    let gz = i32(floor((world_pos.y + 51.2) / 102.4));
    let gx = clamp(gx, -1, 1);
    let gz = clamp(gz, -1, 1);
    return vec2i(gx, gz);
}

// Get local texture coordinates within a tile
fn get_tile_uv(world_pos: vec2f) -> vec2i {
    let grid_pos = get_grid_position(world_pos);
    
    let tile_x_min = f32(grid_pos.x) * 102.4 - 51.2;
    let tile_z_min = f32(grid_pos.y) * 102.4 - 51.2;
    
    let local_x = (world_pos.x - tile_x_min) * 10.0;
    let local_y = (world_pos.y - tile_z_min) * 10.0;
    
    return vec2i(i32(local_x), i32(local_y));
}

fn get_height(vertex_position_world: vec2f) -> vec4f {
    let grid_pos = get_grid_position(vertex_position_world);
    let uv = get_tile_uv(vertex_position_world);
    
    if uv.x < 0 || uv.x >= TILE_SIZE_TEX || uv.y < 0 || uv.y >= TILE_SIZE_TEX {
        // Fallback to center tile
        let base_h = textureLoad(base_11, vec2i(abs(uv.x) % TILE_SIZE_TEX, abs(uv.y) % TILE_SIZE_TEX), 0).r;
        return vec4f(base_h - 0.1, 0.0, 1.0, 0.0);
    }
    
    let water_tex = get_water_texture(grid_pos.x, grid_pos.y);
    let base_tex = get_base_texture(grid_pos.x, grid_pos.y);
    
    let h: f32 = textureLoad(water_tex, uv, 0).r;

    let L = textureLoad(water_tex, uv + vec2i(-1, 0), 0).r;
    let R = textureLoad(water_tex, uv + vec2i(1, 0), 0).r;
    let D = textureLoad(water_tex, uv + vec2i(0, 1), 0).r;
    let U = textureLoad(water_tex, uv + vec2i(0, -1), 0).r;

    let base_h: f32 = textureLoad(base_tex, uv, 0).r;

    let base_L = textureLoad(base_tex, uv + vec2i(-1, 0), 0).r;
    let base_R = textureLoad(base_tex, uv + vec2i(1, 0), 0).r;
    let base_D = textureLoad(base_tex, uv + vec2i(0, 1), 0).r;
    let base_U = textureLoad(base_tex, uv + vec2i(0, -1), 0).r;
    
    let n = vec3f(
        ((L+base_L) - (R+base_R)) * 1.0,
        1.0,
        ((D+base_D) - (U+base_U)) * 1.0
    );
    
    if h == 0.0 {
        return vec4f(base_h - 0.1, normalize(n));
    }
    
    return vec4f(h + base_h, normalize(n));
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

    let total_size = 3.0 * TILE_SIZE_WORLD;
    let offset = total_size / 2.0;
    out.uv = (out.world_position.xz + offset) / total_size;

    return out;
}
