// Vertex entry point of the water material in the depth, normal and deferred
// prepasses.

import bevy_pbr::prepass::io::{Vertex, VertexOutput};
import package::render::water;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    let displaced = water::displace(vertex.instance_index, vertex.position);
    var out: VertexOutput;
    out.world_position = displaced.world_position;
    out.position = displaced.clip_position;
    @if(NORMAL_PREPASS_OR_DEFERRED_PREPASS)
    out.world_normal = displaced.height.yzw;
    out.uv = displaced.uv;
    return out;
}
