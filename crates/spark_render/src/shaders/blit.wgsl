// The stage blit: lay a texture onto a target, composited premultiplied-over
// whatever the target already holds.
//
// One triangle covers the whole target; the scissor rect set by the caller
// trims it to the canvas. The fragment samples the source at the target
// pixel's position over the *target's* size, so a source the same size as
// the target copies texel for texel (the sample lands dead on a texel
// centre and bilinear weights are 0 and 1), and a smaller source — the
// halo layer, or the whole stage in half-resolution playback — comes up
// bilinearly.

struct Blit {
    // The size of the target this blit paints onto, in px.
    onto: vec2<f32>,
    pad: vec2<f32>,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> blit: Blit;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    // (-1,-1), (3,-1), (-1,3): a triangle whose inside is the whole clip box.
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    var out: VsOut;
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(src, samp, in.pos.xy / blit.onto);
}
