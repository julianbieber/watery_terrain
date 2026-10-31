// Vertex entry point of the terrain material in the main (forward) pass.

import bevy_pbr::render::forward_io::{Vertex, VertexOutput};
import package::render::terrain;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    let displaced = terrain::displace(vertex.instance_index, vertex.position);
    var out: VertexOutput;
    out.world_position = displaced.world_position;
    out.position = displaced.clip_position;
    out.world_normal = displaced.height.yzw;
    @if(VERTEX_TANGENTS)
    out.world_tangent = terrain::world_tangent(displaced.height);
    out.uv = displaced.uv;
    return out;
}
