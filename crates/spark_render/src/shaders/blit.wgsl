// The stage blit: copy the cached stage texture onto the frame, pixel for
// pixel, composited premultiplied-over whatever the frame already holds.
//
// One triangle covers the whole target; the scissor rect set by the caller
// trims it to the canvas. `textureLoad` by pixel coordinate, no sampler —
// the stage is the frame's own size, so there is nothing to filter.

@group(0) @binding(0) var stage: texture_2d<f32>;

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
    return textureLoad(stage, vec2<i32>(in.pos.xy), 0);
}
