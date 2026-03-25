
@group(0) @binding(0) var input: texture_storage_2d<rgba32float, read>;
@group(0) @binding(1) var output: texture_storage_2d<rgba32float, write>;


fn get_height(location: vec2i) -> vec4f {
    return textureLoad(input, location);
}

fn set_height(location: vec2i, v: vec4f) {
    textureStore(output, location, v);
} 

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) invocation_id: vec3<u32>) {
    let l = vec2i(invocation_id.xy);

    let height = get_height(l);
    set_height(l, height * 0.99);
}
