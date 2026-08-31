// Meshes: opaque, lit, textured triangles — the imported things in a scene.
//
// One instance per object, its matrices and colour in a storage buffer
// indexed by `instance_index`. Lighting is a single sun plus ambient and a
// Fresnel rim, which is the default light a comp gets until it has lights
// of its own. Colour goes out premultiplied so the resolved picture lands
// on the stage with the same `over` every layer uses.

struct Globals {
    view_proj: mat4x4<f32>,
    // The camera's position, for the rim.
    eye: vec4<f32>,
    // xyz = the direction the light *travels* (unit), w = intensity.
    sun: vec4<f32>,
    // rgb = the sun's colour, w = ambient level.
    sun_color: vec4<f32>,
};

struct Instance {
    model: mat4x4<f32>,
    // Inverse transpose of `model`: what normals turn by.
    normal: mat4x4<f32>,
    // rgb = tint × brightness, a = opacity.
    color: vec4<f32>,
    // x = unlit (1: the colour as is, no lighting).
    material: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> instances: array<Instance>;
@group(1) @binding(0) var base_tex: texture_2d<f32>;
@group(1) @binding(1) var base_samp: sampler;

struct VsIn {
    @builtin(instance_index) ii: u32,
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) ii: u32,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    let inst = instances[in.ii];
    let w = inst.model * vec4<f32>(in.pos, 1.0);
    var out: VsOut;
    out.clip = globals.view_proj * w;
    out.world = w.xyz;
    out.normal = (inst.normal * vec4<f32>(in.normal, 0.0)).xyz;
    out.uv = in.uv;
    out.ii = in.ii;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let inst = instances[in.ii];
    let v = normalize(globals.eye.xyz - in.world);
    var n = normalize(in.normal);
    // Whichever side faces the camera is the side that gets lit — a
    // double-sided material, and a mesh whose winding we didn't check.
    if dot(n, v) < 0.0 {
        n = -n;
    }
    let albedo = textureSample(base_tex, base_samp, in.uv).rgb * inst.color.rgb;
    let nl = max(dot(n, -globals.sun.xyz), 0.0);
    let rim = pow(1.0 - max(dot(n, v), 0.0), 3.0) * 0.35;
    var light = globals.sun_color.rgb * (nl * globals.sun.w) + vec3<f32>(globals.sun_color.w + rim);
    if inst.material.x > 0.5 {
        light = vec3<f32>(1.0);
    }
    let a = inst.color.a;
    return vec4<f32>(albedo * light * a, a);
}
