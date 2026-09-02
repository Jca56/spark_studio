// Meshes: lit, textured triangles — the imported things in a scene.
//
// One instance per object, its matrices and colour in a storage buffer
// indexed by `instance_index`. Lighting is the scene's lights — suns,
// points, spots — plus ambient and a Fresnel rim; a comp with no lights
// of its own is handed the default sun. Colour goes out premultiplied so
// the resolved picture lands on the stage with the same `over` every
// layer uses — and a see-through mesh (alpha under one) lands over what
// is behind it at exactly its opacity.

struct Globals {
    view_proj: mat4x4<f32>,
    // xyz = the camera's position, for the rim; w = the ambient level a
    // scene has until an ambient light sets its own.
    eye: vec4<f32>,
    // x = the rim strength until an ambient light sets its own.
    params: vec4<f32>,
};

struct Light {
    // xyz = position, w = kind: 0 sun, 1 point, 2 spot, 3 ambient.
    pos_kind: vec4<f32>,
    // xyz = the direction the light *travels* (unit), w = range: the
    // distance at which a point or spot shines at its nominal intensity.
    dir_range: vec4<f32>,
    // rgb = colour × intensity, w = cos of the spot cone's outer edge.
    color_cos: vec4<f32>,
    // x = cos of the spot cone's inner edge (where the fade begins);
    // y = an ambient's rim strength; z = the light's shadow map, or -1.
    params: vec4<f32>,
};

struct Shadows {
    // One matrix per map: the world → the casting light's clip space.
    view_proj: array<mat4x4<f32>, 4>,
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
@group(0) @binding(3) var<uniform> shadows: Shadows;
@group(0) @binding(4) var shadow_maps: texture_depth_2d_array;
@group(0) @binding(5) var shadow_samp: sampler_comparison;
@group(1) @binding(0) var base_tex: texture_2d<f32>;
@group(1) @binding(1) var base_samp: sampler;

struct VsIn {
    @builtin(instance_index) ii: u32,
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VsOut {
    // Invariant: a see-through mesh's prepass and colour pass have to
    // land on the same depth to the bit, or LessEqual loses its surface.
    @builtin(position) @invariant clip: vec4<f32>,
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

// How lit a point is by the light that owns shadow map `index`: 1 in
// the open, 0 in shadow, soft between — a 3×3 tap of the comparison
// sampler, from a little off the surface along its normal (more at
// grazing angles) so a surface never shadows itself. Off the map, or
// with no map, a point is lit.
fn shadowed(index: i32, world: vec3<f32>, n: vec3<f32>, to_light: vec3<f32>) -> f32 {
    if index < 0 {
        return 1.0;
    }
    let ndl = clamp(dot(n, to_light), 0.0, 1.0);
    let origin = world + n * (3.0 * (1.0 - ndl) + 0.5);
    let p = shadows.view_proj[index] * vec4<f32>(origin, 1.0);
    let ndc = p.xyz / p.w;
    if ndc.z <= 0.0 || ndc.z >= 1.0 {
        return 1.0;
    }
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return 1.0;
    }
    let texel = 1.0 / f32(textureDimensions(shadow_maps).x);
    var lit = 0.0;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let o = vec2<f32>(f32(dx), f32(dy)) * texel;
            lit += textureSampleCompareLevel(shadow_maps, shadow_samp, uv + o, index, ndc.z - 0.0005);
        }
    }
    return lit / 9.0;
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
    // The scene's level and rim: the defaults, or what its ambient
    // lights say — the first replaces the default, the rest add.
    var ambient = vec3<f32>(globals.eye.w);
    var rim_k = globals.params.x;
    var own_ambient = false;
    var direct = vec3<f32>(0.0);
    let n_lights = u32(lights.count.x + 0.5);
    for (var i = 0u; i < n_lights; i++) {
        let l = lights.items[i];
        let kind = u32(l.pos_kind.w + 0.5);
        if kind == 3u {
            if !own_ambient {
                ambient = vec3<f32>(0.0);
                own_ambient = true;
            }
            ambient += l.color_cos.rgb;
            rim_k = l.params.y;
            continue;
        }
        // `to_light` points from the surface toward the light.
        var to_light = -l.dir_range.xyz;
        var atten = 1.0;
        if kind != 0u {
            let to = l.pos_kind.xyz - in.world;
            let dist = length(to);
            to_light = to / max(dist, 1e-4);
            // Inverse square, in the light's own units: full intensity at
            // its range, a quarter at twice that, four times right at it
            // — softened at the light itself so nothing divides by zero.
            // It never cuts off: a light past its range is dim, not gone.
            let r = max(l.dir_range.w, 1e-4);
            atten = (r * r) / (dist * dist + 0.25 * r * r);
            if kind == 2u {
                let along = dot(-to_light, l.dir_range.xyz);
                atten *= smoothstep(l.color_cos.w, l.params.x, along);
            }
        }
        let lit = shadowed(i32(l.params.z), in.world, n, to_light);
        direct += l.color_cos.rgb * (max(dot(n, to_light), 0.0) * atten * lit);
    }
    let rim = pow(1.0 - max(dot(n, v), 0.0), 3.0) * rim_k;
    var light = ambient + rim + direct;
    if inst.material.x > 0.5 {
        light = vec3<f32>(1.0);
    }
    let a = inst.color.a;
    return vec4<f32>(albedo * light * a, a);
}

// The depth prepass: a see-through mesh's nearest surface into the depth
// buffer and nothing into the colour target (its write mask is off).
@fragment
fn fs_depth() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}
