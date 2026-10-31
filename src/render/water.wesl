import bevy_pbr::render::{
    mesh_functions,
    view_transformations::position_world_to_clip,
};

@if(PREPASS_PIPELINE)
import bevy_pbr::prepass::io::{Vertex, VertexOutput};
@else
import bevy_pbr::render::forward_io::{Vertex, VertexOutput};


@group(constants::MATERIAL_BIND_GROUP) @binding(100) var water_height_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var water_height_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var base_height_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var base_height_sampler: sampler;

fn get_height(vertex_position_world: vec2f) -> vec4f {
    let uv = vec2i(floor((vertex_position_world + vec2f(1024.0))));
    let h: f32 = textureLoad(water_height_texture, uv, 0).r;

    let L = textureLoad(water_height_texture, uv + vec2i(-1,  0), 0).r;
    let R = textureLoad(water_height_texture, uv + vec2i(1,  0), 0).r;
    let D = textureLoad(water_height_texture, uv + vec2i( 0, 1), 0).r;
    let U = textureLoad(water_height_texture, uv + vec2i( 0,  -1), 0).r;

    let base_h: f32 = textureLoad(base_height_texture, uv, 0).r;

    let base_L = textureLoad(base_height_texture, uv + vec2i(-1,  0), 0).r;
    let base_R = textureLoad(base_height_texture, uv + vec2i(1,  0), 0).r;
    let base_D = textureLoad(base_height_texture, uv + vec2i( 0, 1), 0).r;
    let base_U = textureLoad(base_height_texture, uv + vec2i( 0,  -1), 0).r;
    let n = vec3f(
        ((L+base_L) - (R+base_R)) * 1.0,
        1.0,
        ((D+base_D) - (U+base_U)) * 1.0
    );
    if h == 0.0{
        return vec4f(base_h - 0.1, normalize(n));
    } 

    return vec4f(h+base_h, normalize(n));
}

@vertex
fn vertex(vertex: Vertex, @builtin(vertex_index) idx: u32) -> VertexOutput {
    var out: VertexOutput;
    let model = mesh_functions::get_world_from_local(vertex.instance_index);
    out.world_position = model * vec4<f32>(vertex.position, 1.0);
    // let height = get_height(out.world_position.xz/1024.0);
    let height = get_height(out.world_position.xz * 10.0);
    out.world_position.y = height.x;

    @if(NORMAL_PREPASS_OR_DEFERRED_PREPASS) {
        out.world_normal = height.yzw;
    } @elif(!PREPASS_PIPELINE) {
        out.world_normal = height.yzw;
        @if(VERTEX_TANGENTS) {
            if abs(dot(height.yzw, vec3(0,1,0))) < 0.899 {
                out.world_tangent = vec4(normalize(cross(vec3(0,1,0), height.yzw)), 1.0)*0.11;
            } else {
                out.world_tangent = vec4(normalize(cross(vec3(1,0,0), height.yzw)), 1.0)*0.11; // fallback for near-vertical normals
            }
        }
    }

    out.position = position_world_to_clip(out.world_position.xyz);

    out.uv = ((out.world_position.xz+1024.0)/2048.0)*40.0;

    return out;
}
