// Vertex entry point of the terrain material in the depth, normal and deferred
// prepasses.

import bevy_pbr::prepass::io::{Vertex, VertexOutput};
import package::render::terrain;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    let displaced = terrain::displace(vertex.instance_index, vertex.position);
    var out: VertexOutput;
    out.world_position = displaced.world_position;
    out.position = displaced.clip_position;
    @if(NORMAL_PREPASS_OR_DEFERRED_PREPASS)
    out.world_normal = displaced.height.yzw;
    out.uv = displaced.uv;
    return out;
}
