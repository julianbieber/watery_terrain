import bevy_pbr::render::{
    mesh_functions,
    view_transformations::position_world_to_clip,
};

@if(PREPASS_PIPELINE)
import bevy_pbr::prepass::io::{Vertex, VertexOutput};
@else
import bevy_pbr::render::forward_io::{Vertex, VertexOutput};


@group(constants::MATERIAL_BIND_GROUP) @binding(100) var height_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var height_sampler: sampler;

const TERRAIN_UV_REPEAT: f32 = 40.0;

fn get_height(vertex_position_world: vec2f) -> vec4f {
    let uv = vec2i(floor((vertex_position_world + vec2f(1024.0))));
    let h: f32 = textureLoad(height_texture, uv, 0).r;

    let L = textureLoad(height_texture, uv + vec2i(-1,  0), 0).r;
    let R = textureLoad(height_texture, uv + vec2i(1,  0), 0).r;
    let D = textureLoad(height_texture, uv + vec2i( 0, 1), 0).r;
    let U = textureLoad(height_texture, uv + vec2i( 0,  -1), 0).r;
    let n = vec3f(
        (L - R) * 1.0,
        1.0,
        (D - U) * 1.0
    );

    return vec4f(h, normalize(n));
}

fn wrap(x: f32) -> f32 {
    return fract(x + ceil(abs(x)));
}


fn height_uv_range(h: f32) -> vec2f {
    if h < 0.0 {
        return vec2f(0.0, 0.25);
    }
    if h < 2.25 {
        return vec2f(0.25, 0.5);
    }
    if h < 4.35 {
        return vec2f(0.5, 0.75);
    }
    return vec2f(0.75, 1.0);
    
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
            if abs(dot(height.xzw, vec3(0,1,0))) < 0.899 {
                out.world_tangent = vec4(normalize(cross(vec3(0,1,0), height.yzw)), 1.0)*0.11;
            } else {
                out.world_tangent = vec4(normalize(cross(vec3(1,0,0), height.yzw)), 1.0)*0.11; // fallback for near-vertical normals
            }
        }
    }

    out.position = position_world_to_clip(out.world_position.xyz);

    let tiled_uv = ((out.world_position.xz+1024.0)/2048.0) * TERRAIN_UV_REPEAT;
    let u_range = height_uv_range(height.x);
    out.uv = vec2f(mix(u_range.x, u_range.y, wrap(tiled_uv.x)), tiled_uv.y);

    return out;
}
