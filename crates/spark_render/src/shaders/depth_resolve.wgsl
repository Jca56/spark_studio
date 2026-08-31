// Depth resolve: the opaque pass draws into a multisampled depth buffer,
// and the shape pass tests against a single-sample one — the stage's own,
// and the halo layer's at half size. Each destination pixel takes the
// *nearest* depth of every sample it covers, so a translucent shape is
// hidden by a mesh wherever any part of the pixel is, which errs toward
// the mesh at its edges rather than toward light leaking through it.

struct Params {
    // Source texels per destination pixel, per axis: 1 for the stage, 2
    // for the halo layer.
    ratio: vec2<f32>,
    pad: vec2<f32>,
};

@group(0) @binding(0) var src: texture_depth_multisampled_2d;
@group(0) @binding(1) var<uniform> params: Params;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @builtin(frag_depth) f32 {
    let r = u32(params.ratio.x + 0.5);
    let dims = textureDimensions(src);
    let samples = textureNumSamples(src);
    let base = vec2<u32>(pos.xy) * r;
    var d = 1.0;
    for (var y = 0u; y < r; y++) {
        for (var x = 0u; x < r; x++) {
            let c = min(base + vec2<u32>(x, y), dims - vec2<u32>(1u));
            for (var s = 0u; s < samples; s++) {
                d = min(d, textureLoad(src, c, i32(s)));
            }
        }
    }
    return d;
}
