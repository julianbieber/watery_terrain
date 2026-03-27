
@group(0) @binding(0) var input: texture_storage_2d<r32float, read>;
@group(0) @binding(1) var output: texture_storage_2d<r32float, write>;

@group(0) @binding(2) var flow_x: texture_storage_2d<r32float, read_write>;
@group(0) @binding(3) var flow_y: texture_storage_2d<r32float, read_write>;

struct SimParams {
    id: i32,
    _pad: vec3f,
};

var<push_constant> sim: SimParams;


fn get_height(location: vec2i) -> f32{
    return clamp(textureLoad(input, location), vec4f(0.0), vec4f(1.0)).x;
}

fn set_height(location: vec2i, v: f32) {
    textureStore(output, location, vec4f(v, 0.0, 0.0, 0.0));
} 

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) invocation_id: vec3<u32>) {
    let l = vec2i(invocation_id.xy);
    let change_mult = 0.4;


    let own = get_height(l);
    var change = 0.0;
    let left = get_height(l - vec2i(1, 0));
    {
        let c =(left - own) * change_mult; 
        change += c;
    }
    let right = get_height(l + vec2i(1, 0));
    {
        let c =(right - own) * change_mult; 
        change += c;
    }
    let up = get_height(l + vec2i(0, 1));
    {
        let c =(up - own) * change_mult; 
        change += c;
    }
    let down = get_height(l - vec2i(0, 1));
    {
        let c =(down - own) * change_mult; 
        change += c;
    }
    set_height(l+vec2i(1,1), own + change);
}
