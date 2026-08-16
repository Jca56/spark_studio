// Flat UI rects + SDF icon glyphs: instanced quads in window pixels.
// icon.x selects the glyph: 0 solid fill, 1 minus, 2 square outline, 3 X.

struct Globals {
    resolution: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var image_tex: texture_2d<f32>;
@group(1) @binding(1) var image_samp: sampler;

struct VsIn {
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) icon: vec4<f32>,
    @location(4) color2: vec4<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) icon: vec4<f32>,
    @location(2) local: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) color2: vec4<f32>,
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
    out.color2 = in.color2;
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

fn sd_ngon(p: vec2<f32>, radius: f32, sides: f32) -> f32 {
    let an = 3.14159265 / sides;
    let acs = vec2<f32>(cos(an), sin(an));
    var ang = atan2(p.x, p.y);
    let m = 2.0 * an;
    ang = ang - m * floor(ang / m);
    let bn = ang - an;
    var q = length(p) * vec2<f32>(cos(bn), abs(sin(bn)));
    q = q - radius * acs;
    q.y = q.y + clamp(-q.y, 0.0, radius * acs.y);
    return length(q) * sign(q.x);
}

fn sd_triangle(p: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> f32 {
    let e0 = p1 - p0;
    let e1 = p2 - p1;
    let e2 = p0 - p2;
    let v0 = p - p0;
    let v1 = p - p1;
    let v2 = p - p2;
    let pq0 = v0 - e0 * clamp(dot(v0, e0) / dot(e0, e0), 0.0, 1.0);
    let pq1 = v1 - e1 * clamp(dot(v1, e1) / dot(e1, e1), 0.0, 1.0);
    let pq2 = v2 - e2 * clamp(dot(v2, e2) / dot(e2, e2), 0.0, 1.0);
    let s = sign(e0.x * e2.y - e0.y * e2.x);
    let d = min(
        min(
            vec2<f32>(dot(pq0, pq0), s * (v0.x * e0.y - v0.y * e0.x)),
            vec2<f32>(dot(pq1, pq1), s * (v1.x * e1.y - v1.y * e1.x)),
        ),
        vec2<f32>(dot(pq2, pq2), s * (v2.x * e2.y - v2.y * e2.x)),
    );
    return -sqrt(d.x) * sign(d.y);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let kind = u32(in.icon.x + 0.5);
    let t = in.icon.y;
    let p = in.local - in.size * 0.5;
    // Glyph radius factor: icon.w overrides the default when > 0.
    let rf = select(0.20, in.icon.w, in.icon.w > 0.0);
    let r = min(in.size.x, in.size.y) * rf;
    // All glyph distances computed unconditionally: fwidth needs uniform
    // control flow, and the branches here would be per-instance divergent.
    let d_minus = sd_box(p, vec2<f32>(r, t));
    let d_square = abs(sd_box(p, vec2<f32>(r * 0.92, r * 0.92))) - t;
    let d_x = min(
        sd_seg(p, vec2<f32>(-r, -r) * 0.92, vec2<f32>(r, r) * 0.92),
        sd_seg(p, vec2<f32>(-r, r) * 0.92, vec2<f32>(r, -r) * 0.92),
    ) - t;
    let d_arrow = sd_triangle(
        p,
        vec2<f32>(-0.42 * r, -0.95 * r),
        vec2<f32>(-0.42 * r, 0.62 * r),
        vec2<f32>(0.52 * r, 0.02 * r),
    );
    let d_circle = abs(length(p) - 0.78 * r) - t;
    // icon.z carries the ngon's side count when >= 3 (fills use it as corner
    // radius instead; glyph and fill paths never mix).
    let pent_sides = select(5.0, in.icon.z, in.icon.z >= 3.0);
    let d_pent = abs(sd_ngon(p, 0.85 * r, pent_sides)) - t;
    let d_line = sd_seg(p, vec2<f32>(-0.7 * r, 0.65 * r), vec2<f32>(0.7 * r, -0.65 * r)) - t;
    let d_play = sd_triangle(
        p,
        vec2<f32>(-0.5 * r, -0.8 * r),
        vec2<f32>(-0.5 * r, 0.8 * r),
        vec2<f32>(0.8 * r, 0.0),
    );
    let d_pause = min(
        sd_box(p - vec2<f32>(-0.42 * r, 0.0), vec2<f32>(0.18 * r, 0.75 * r)),
        sd_box(p - vec2<f32>(0.42 * r, 0.0), vec2<f32>(0.18 * r, 0.75 * r)),
    );
    var d = 1e5;
    d = select(d, d_minus, kind == 1u);
    d = select(d, d_square, kind == 2u);
    d = select(d, d_x, kind == 3u);
    d = select(d, d_arrow, kind == 4u);
    d = select(d, d_circle, kind == 5u);
    d = select(d, d_pent, kind == 6u);
    d = select(d, d_line, kind == 7u);
    d = select(d, d_play, kind == 9u);
    d = select(d, d_pause, kind == 10u);
    let aa = max(fwidth(d), 0.0001);
    let glyph = 1.0 - smoothstep(-aa, aa, d);
    // Rounded-corner coverage for fills (icon.z = corner radius, 0 = sharp).
    let radius = in.icon.z;
    let d_round = sd_box(p, in.size * 0.5 - vec2<f32>(radius)) - radius;
    let aa_r = max(fwidth(d_round), 0.0001);
    let round_cov = 1.0 - smoothstep(-aa_r, aa_r, d_round);
    let fill_cov = select(1.0, round_cov, radius > 0.0);
    let cov = select(glyph, fill_cov, kind == 0u);
    // Image sampling must also stay in uniform control flow.
    let img = textureSample(image_tex, image_samp, in.local / max(in.size, vec2<f32>(0.0001)));
    // Left→right gradient for fills when color2 carries alpha.
    let gt = clamp(in.local.x / max(in.size.x, 0.0001), 0.0, 1.0);
    let grad = mix(in.color, in.color2, gt);
    let base = select(in.color, grad, in.color2.a > 0.0 && kind == 0u);
    let flat = vec4<f32>(base.rgb, base.a * cov);
    let image = vec4<f32>(img.rgb * in.color.rgb, img.a * in.color.a);
    return select(flat, image, kind == 8u);
}
