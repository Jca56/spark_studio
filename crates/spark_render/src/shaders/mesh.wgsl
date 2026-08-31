// Meshes: opaque, lit, textured triangles — the imported things in a scene.
//
// One instance per object, its matrices and colour in a storage buffer
// indexed by `instance_index`. Lighting is the scene's lights — suns,
// points, spots — plus ambient and a Fresnel rim; a comp with no lights
// of its own is handed the default sun. Colour goes out premultiplied so
// the resolved picture lands on the stage with the same `over` every
// layer uses.

struct Globals {
    view_proj: mat4x4<f32>,
    // xyz = the camera's position, for the rim; w = ambient level.
    eye: vec4<f32>,
};

struct Light {
    // xyz = position, w = kind: 0 sun, 1 point, 2 spot.
    pos_kind: vec4<f32>,
    // xyz = the direction the light *travels* (unit), w = range.
    dir_range: vec4<f32>,
    // rgb = colour × intensity, w = cos of the spot cone's outer edge.
    color_cos: vec4<f32>,
    // x = cos of the spot cone's inner edge (where the fade begins).
    params: vec4<f32>,
};

struct Lights {
    count: vec4<f32>,
    items: array<Light, 8>,
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
@group(0) @binding(2) var<uniform> lights: Lights;
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
    let rim = pow(1.0 - max(dot(n, v), 0.0), 3.0) * 0.35;
    var light = vec3<f32>(globals.eye.w + rim);
    let n_lights = u32(lights.count.x + 0.5);
    for (var i = 0u; i < n_lights; i++) {
        let l = lights.items[i];
        let kind = u32(l.pos_kind.w + 0.5);
        // `to_light` points from the surface toward the light.
        var to_light = -l.dir_range.xyz;
        var atten = 1.0;
        if kind != 0u {
            let to = l.pos_kind.xyz - in.world;
            let dist = length(to);
            to_light = to / max(dist, 1e-4);
            // Fades to exactly nothing at the range, smoothly: a light you
            // can keyframe the reach of without a hard edge appearing.
            let r = max(l.dir_range.w, 1e-4);
            let x = clamp(1.0 - (dist * dist) / (r * r), 0.0, 1.0);
            atten = x * x;
            if kind == 2u {
                let along = dot(-to_light, l.dir_range.xyz);
                atten *= smoothstep(l.color_cos.w, l.params.x, along);
            }
        }
        light += l.color_cos.rgb * (max(dot(n, to_light), 0.0) * atten);
    }
    if inst.material.x > 0.5 {
        light = vec3<f32>(1.0);
    }
    let a = inst.color.a;
    return vec4<f32>(albedo * light * a, a);
}
