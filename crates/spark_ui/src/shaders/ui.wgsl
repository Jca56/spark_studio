// Flat UI rects + SDF icon glyphs: instanced quads in window pixels.
// icon.x selects the glyph: 0 solid fill, 1 minus, 2 square outline, 3 X.

struct Globals {
    resolution: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

struct VsIn {
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) icon: vec4<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) icon: vec4<f32>,
    @location(2) local: vec2<f32>,
    @location(3) size: vec2<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    let corner = vec2<f32>(f32(in.vi & 1u), f32(in.vi >> 1u));
    let px = in.pos + corner * in.size;
    var ndc = px / globals.resolution * 2.0 - vec2<f32>(1.0);
    ndc.y = -ndc.y;
    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.color = in.color;
    out.icon = in.icon;
    out.local = corner * in.size;
    out.size = in.size;
    return out;
}

fn sd_box(p: vec2<f32>, half: vec2<f32>) -> f32 {
    let d = abs(p) - half;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

fn sd_seg(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / max(dot(ba, ba), 0.0001), 0.0, 1.0);
    return length(pa - ba * h);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let kind = u32(in.icon.x + 0.5);
    let t = in.icon.y;
    let p = in.local - in.size * 0.5;
    let r = min(in.size.x, in.size.y) * 0.20;
    // All glyph distances computed unconditionally: fwidth needs uniform
    // control flow, and the branches here would be per-instance divergent.
    let d_minus = sd_box(p, vec2<f32>(r, t));
    let d_square = abs(sd_box(p, vec2<f32>(r * 0.92, r * 0.92))) - t;
    let d_x = min(
        sd_seg(p, vec2<f32>(-r, -r) * 0.92, vec2<f32>(r, r) * 0.92),
        sd_seg(p, vec2<f32>(-r, r) * 0.92, vec2<f32>(r, -r) * 0.92),
    ) - t;
    var d = 1e5;
    d = select(d, d_minus, kind == 1u);
    d = select(d, d_square, kind == 2u);
    d = select(d, d_x, kind == 3u);
    let aa = max(fwidth(d), 0.0001);
    let glyph = 1.0 - smoothstep(-aa, aa, d);
    let cov = select(glyph, 1.0, kind == 0u);
    return vec4<f32>(in.color.rgb, in.color.a * cov);
}
