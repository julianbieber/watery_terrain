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
    let uv = vec2i(floor((vertex_position_world + vec2f(1024.0))));
    let h: f32 = textureLoad(height_texture, uv, 0).r;

    let L = textureLoad(height_texture, uv + vec2i(-1,  0), 0).r;
    let R = textureLoad(height_texture, uv + vec2i(1,  0), 0).r;
    let D = textureLoad(height_texture, uv + vec2i( 0, 1), 0).r;
    let U = textureLoad(height_texture, uv + vec2i( 0,  -1), 0).r;

    let n = vec3f(
        (L - R)*5.0 ,
        1.0,    
        (D - U)*5.0 
    );

    return vec4f(h* 10.0, normalize(n));
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
