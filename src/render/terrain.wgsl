#import bevy_pbr::mesh_functions
#import bevy_pbr::pbr_fragment::pbr_input_from_standard_material
#import bevy_pbr::view_transformations::position_world_to_clip

#ifdef MESHLET_MESH_MATERIAL_PASS
#import bevy_pbr::meshlet_visibility_buffer_resolve::VertexOutput
#else ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::{Vertex, VertexOutput, FragmentOutput}
#import bevy_pbr::pbr_deferred_functions::deferred_output;
#else   // PREPASS_PIPELINE
#import bevy_pbr::forward_io::{Vertex, VertexOutput, FragmentOutput}
#import bevy_pbr::pbr_functions::main_pass_post_lighting_processing
#endif  // PREPASS_PIPELINE


@group(#{MATERIAL_BIND_GROUP}) @binding(100) var height_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var height_sampler: sampler;

fn get_height(vertex_position_world: vec2f) -> vec4f {
    // Convert world position to texture coordinates (integer pixel coordinates)
    // The caller already scales by 10, so we just add the offset
    let texture_size = 1024.0;
    let pos = vertex_position_world + vec2f(texture_size);
    let uv = vec2i(floor(pos));
    
    // Fractional part for bilinear interpolation
    let frac = fract(pos);
    
    // Get the 4 corner heights using textureLoad
    let h00 = textureLoad(height_texture, uv, 0).r;
    let h10 = textureLoad(height_texture, uv + vec2i(1, 0), 0).r;
    let h01 = textureLoad(height_texture, uv + vec2i(0, 1), 0).r;
    let h11 = textureLoad(height_texture, uv + vec2i(1, 1), 0).r;
    
    // Bilinear interpolation
    let h = mix(mix(h00, h10, frac.x), mix(h01, h11, frac.x), frac.y);
    
    // For normal calculation, sample neighbors with bilinear interpolation
    // Sample at offsets of 1 pixel in each direction
    let offset_x = vec2i(1, 0);
    let offset_y = vec2i(0, 1);
    
    // Left neighbor (uv - 1 in x)
    let l_uv = uv - vec2i(1, 0);
    let l00 = textureLoad(height_texture, l_uv, 0).r;
    let l10 = textureLoad(height_texture, l_uv + vec2i(1, 0), 0).r;
    let l01 = textureLoad(height_texture, l_uv + vec2i(0, 1), 0).r;
    let l11 = textureLoad(height_texture, l_uv + vec2i(1, 1), 0).r;
    let L = mix(mix(l00, l10, frac.x), mix(l01, l11, frac.x), frac.y);
    
    // Right neighbor (uv + 1 in x)
    let r_uv = uv + vec2i(1, 0);
    let r00 = textureLoad(height_texture, r_uv, 0).r;
    let r10 = textureLoad(height_texture, r_uv + vec2i(1, 0), 0).r;
    let r01 = textureLoad(height_texture, r_uv + vec2i(0, 1), 0).r;
    let r11 = textureLoad(height_texture, r_uv + vec2i(1, 1), 0).r;
    let R = mix(mix(r00, r10, frac.x), mix(r01, r11, frac.x), frac.y);
    
    // Down neighbor (uv + 1 in y)
    let d_uv = uv + vec2i(0, 1);
    let d00 = textureLoad(height_texture, d_uv, 0).r;
    let d10 = textureLoad(height_texture, d_uv + vec2i(1, 0), 0).r;
    let d01 = textureLoad(height_texture, d_uv + vec2i(0, 1), 0).r;
    let d11 = textureLoad(height_texture, d_uv + vec2i(1, 1), 0).r;
    let D = mix(mix(d00, d10, frac.x), mix(d01, d11, frac.x), frac.y);
    
    // Up neighbor (uv - 1 in y)
    let u_uv = uv - vec2i(0, 1);
    let u00 = textureLoad(height_texture, u_uv, 0).r;
    let u10 = textureLoad(height_texture, u_uv + vec2i(1, 0), 0).r;
    let u01 = textureLoad(height_texture, u_uv + vec2i(0, 1), 0).r;
    let u11 = textureLoad(height_texture, u_uv + vec2i(1, 1), 0).r;
    let U = mix(mix(u00, u10, frac.x), mix(u01, u11, frac.x), frac.y);

    let n = vec3f(
        (L - R) * 5.0,
        1.0,
        (D - U) * 5.0
    );

    return vec4f(h * 10.0, normalize(n));
}

fn wrap(x: vec2f) -> vec2f {
    return fract(x + ceil(abs(x)));
}

@vertex
fn vertex(vertex: Vertex, @builtin(vertex_index) idx: u32) -> VertexOutput {
    var out: VertexOutput;
    let model = mesh_functions::get_world_from_local(vertex.instance_index);
    out.world_position = model * vec4<f32>(vertex.position, 1.0);
    // let height = get_height(out.world_position.xz/1024.0);
    let height = get_height(out.world_position.xz*10.0);
    out.world_position.y = height.x;

    #ifdef MESHLET_MESH_MATERIAL_PASS
    #else ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
        out.world_normal = height.yzw;
    #else ifdef PREPASS_PIPELINE
    #else
        out.world_normal = height.yzw;
        if abs(dot(height.xzw, vec3(0,1,0))) < 0.899 {
            out.world_tangent = vec4(normalize(cross(vec3(0,1,0), height.yzw)), 1.0)*0.11;
        } else {
            out.world_tangent = vec4(normalize(cross(vec3(1,0,0), height.yzw)), 1.0)*0.11; // fallback for near-vertical normals
        }
    #endif

    out.position = position_world_to_clip(out.world_position.xyz);

    out.uv = wrap(((out.world_position.xz+1024.0)/2048.0));

    return out;
}
