// SparkUI's material shader. One instance = one complete piece of chrome:
// drop shadow, fill, gradient, inner shadow, bevel, grain and a real inset
// stroke, all composited here in a single quad.
//
// Instance data comes from a storage buffer indexed by instance_index, not
// from vertex attributes — attributes cap out at 16 slots / 60 inter-stage
// components, and the material set is meant to keep growing.
//
// Every parameter is zero by default and zero means "off", so a plain fill
// costs the same few ALU ops it always did.

struct Globals {
    resolution: vec2<f32>,
    _pad: vec2<f32>,
};

struct Rect {
    pos: vec2<f32>,
    size: vec2<f32>,
    color: vec4<f32>,
    // [kind, glyph thickness, ngon sides, glyph radius factor]
    icon: vec4<f32>,
    color2: vec4<f32>,
    // Corner radii tl/tr/br/bl — or a capsule's endpoints, center-relative.
    radii: vec4<f32>,
    // [on, turns, 0 linear / 1 radial, unused]
    grad: vec4<f32>,
    // [stroke width, unused x3]
    edge: vec4<f32>,
    edge_color: vec4<f32>,
    // [offset x, offset y, blur, spread]
    outer: vec4<f32>,
    outer_color: vec4<f32>,
    inner: vec4<f32>,
    inner_color: vec4<f32>,
    // [top highlight, bottom shade, thickness, unused]
    bevel: vec4<f32>,
    // [amount, pixel size, unused, unused]
    grain: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> rects: array<Rect>;
@group(1) @binding(0) var image_tex: texture_2d<f32>;
@group(1) @binding(1) var image_samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    // Position inside the rect, in px from its top-left. Runs negative /
    // past `size` in the padding a drop shadow adds.
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) idx: u32,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VsOut {
    let r = rects[ii];
    // Grow the quad by whatever spills past the shape: a drop shadow's reach,
    // or an outward-aligned stroke. With neither, the quad is exactly the
    // rect, so nothing already on screen shifts by even a subpixel.
    let shadow_reach = select(
        0.0,
        r.outer.z + r.outer.w + max(abs(r.outer.x), abs(r.outer.y)),
        r.outer_color.a > 0.0,
    );
    let edge_reach = select(0.0, r.edge.x * r.edge.y, r.edge_color.a > 0.0);
    let reach = max(shadow_reach, edge_reach);
    let pad = select(0.0, reach + 2.0, reach > 0.0);
    let corner = vec2<f32>(f32(vi & 1u), f32(vi >> 1u));
    let full = r.size + vec2<f32>(pad * 2.0);
    let px = r.pos - vec2<f32>(pad) + corner * full;
    var ndc = px / globals.resolution * 2.0 - vec2<f32>(1.0);
    ndc.y = -ndc.y;

    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.local = corner * full - vec2<f32>(pad);
    out.idx = ii;
    return out;
}

// ---------------------------------------------------------------- distance

fn sd_box(p: vec2<f32>, half: vec2<f32>) -> f32 {
    let d = abs(p) - half;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

// Rounded box with an independent radius per corner. y grows downward, so
// p.y < 0 is the top half.
fn sd_round_box(p: vec2<f32>, half: vec2<f32>, radii: vec4<f32>) -> f32 {
    let left = select(radii.w, radii.x, p.y < 0.0);  // bl : tl
    let right = select(radii.z, radii.y, p.y < 0.0); // br : tr
    var k = select(left, right, p.x > 0.0);
    k = clamp(k, 0.0, min(half.x, half.y));
    let q = abs(p) - half + vec2<f32>(k);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - k;
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

// The instance's silhouette as one signed distance field: the rounded box
// for fills, the glyph's own field for icons. Everything downstream —
// shadow, stroke, bevel, inner shadow — is computed from this, which is why
// icons now get borders and glows for free.
//
// Called more than once per pixel (the shadow samples it at an offset), so
// it stays branchless: every candidate distance is evaluated and selected.
fn shape_sd(r: Rect, p: vec2<f32>) -> f32 {
    let kind = u32(r.icon.x + 0.5);
    let half = r.size * 0.5;
    let t = r.icon.y;
    // Glyph radius: icon.w overrides the default fraction when > 0.
    let rf = select(0.20, r.icon.w, r.icon.w > 0.0);
    let g = min(r.size.x, r.size.y) * rf;

    let d_minus = sd_box(p, vec2<f32>(g, t));
    let d_square = abs(sd_box(p, vec2<f32>(g * 0.92, g * 0.92))) - t;
    let d_x = min(
        sd_seg(p, vec2<f32>(-g, -g) * 0.92, vec2<f32>(g, g) * 0.92),
        sd_seg(p, vec2<f32>(-g, g) * 0.92, vec2<f32>(g, -g) * 0.92),
    ) - t;
    let d_arrow = sd_triangle(
        p,
        vec2<f32>(-0.42 * g, -0.95 * g),
        vec2<f32>(-0.42 * g, 0.62 * g),
        vec2<f32>(0.52 * g, 0.02 * g),
    );
    let d_circle = abs(length(p) - 0.78 * g) - t;
    let pent_sides = select(5.0, r.icon.z, r.icon.z >= 3.0);
    let d_pent = abs(sd_ngon(-p, 0.85 * g, pent_sides)) - t;
    let d_line = sd_seg(p, vec2<f32>(-0.7 * g, 0.65 * g), vec2<f32>(0.7 * g, -0.65 * g)) - t;
    let d_play = sd_triangle(
        p,
        vec2<f32>(-0.5 * g, -0.8 * g),
        vec2<f32>(-0.5 * g, 0.8 * g),
        vec2<f32>(0.8 * g, 0.0),
    );
    let d_pause = min(
        sd_box(p - vec2<f32>(-0.42 * g, 0.0), vec2<f32>(0.18 * g, 0.75 * g)),
        sd_box(p - vec2<f32>(0.42 * g, 0.0), vec2<f32>(0.18 * g, 0.75 * g)),
    );
    let d_zig = min(
        sd_seg(p, vec2<f32>(-0.8 * g, 0.5 * g), vec2<f32>(-0.27 * g, -0.5 * g)),
        min(
            sd_seg(p, vec2<f32>(-0.27 * g, -0.5 * g), vec2<f32>(0.27 * g, 0.5 * g)),
            sd_seg(p, vec2<f32>(0.27 * g, 0.5 * g), vec2<f32>(0.8 * g, -0.5 * g)),
        ),
    ) - t;
    // Filled diamond (keyframe marker): L1-norm distance.
    let d_key = (abs(p.x) + abs(p.y)) - 0.82 * g;
    // Cogwheel: a disc whose rim ripples with 8 square-ish teeth, minus a
    // hub hole.
    let ga = atan2(p.y, p.x);
    let teeth = clamp(sin(ga * 8.0) * 2.0, -1.0, 1.0) * 0.11 * g;
    let d_gear = max(length(p) - (0.70 * g + teeth), -(length(p) - 0.30 * g));
    // Eye: almond outline (two-circle intersection) + pupil; the hidden
    // variant swaps the pupil for a diagonal slash.
    let er = 0.85 * g;
    let ec = 0.55 * er;
    let rr = 1.05 * er;
    let d_alm = abs(max(
        length(p - vec2<f32>(0.0, ec)) - rr,
        length(p + vec2<f32>(0.0, ec)) - rr,
    )) - t;
    let d_eye = min(d_alm, length(p) - 0.25 * er);
    let d_eye_off = min(d_alm, sd_seg(p, vec2<f32>(-er, er), vec2<f32>(er, -er)) - t);
    // Capsule: endpoints ride in `radii`, half-thickness in icon.y.
    let d_cap = sd_seg(p, r.radii.xy, r.radii.zw) - t;

    // Fills (0), the picker's two ramps (12, 13) and the image blit (8) all
    // take the rounded box.
    var d = sd_round_box(p, half, r.radii);
    d = select(d, d_minus, kind == 1u);
    d = select(d, d_square, kind == 2u);
    d = select(d, d_x, kind == 3u);
    d = select(d, d_arrow, kind == 4u);
    d = select(d, d_circle, kind == 5u);
    d = select(d, d_pent, kind == 6u);
    d = select(d, d_line, kind == 7u);
    d = select(d, d_play, kind == 9u);
    d = select(d, d_pause, kind == 10u);
    d = select(d, d_zig, kind == 11u);
    d = select(d, d_key, kind == 14u);
    d = select(d, d_gear, kind == 15u);
    d = select(d, d_eye, kind == 16u);
    d = select(d, d_eye_off, kind == 17u);
    d = select(d, d_cap, kind == 18u);
    return d;
}

// ------------------------------------------------------------------- color

// Rainbow hue ramp (sRGB), h in 0..1.
fn hue_ramp(h: f32) -> vec3<f32> {
    let k = h * 6.0;
    return vec3<f32>(
        clamp(abs(k - 3.0) - 1.0, 0.0, 1.0),
        clamp(2.0 - abs(k - 2.0), 0.0, 1.0),
        clamp(2.0 - abs(k - 4.0), 0.0, 1.0),
    );
}

fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(vec3<f32>(p.xyx) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

// Straight-alpha "src over dst".
fn over(dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    let a = src.a + dst.a * (1.0 - src.a);
    let rgb = (src.rgb * src.a + dst.rgb * dst.a * (1.0 - src.a)) / max(a, 0.0001);
    return vec4<f32>(rgb, a);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let r = rects[in.idx];
    let kind = u32(r.icon.x + 0.5);
    let half = r.size * 0.5;
    let p = in.local - half;

    let d = shape_sd(r, p);
    let aa = max(fwidth(d), 0.0001);
    var cov = 1.0 - smoothstep(-aa, aa, d);
    // A sharp-cornered fill with nothing spilling outside covers its quad
    // exactly. Skipping the AA ramp keeps panels that tile edge to edge from
    // showing a half-covered hairline along every shared seam.
    let sharp = max(max(r.radii.x, r.radii.y), max(r.radii.z, r.radii.w)) <= 0.0;
    let plain = kind == 0u || kind == 12u || kind == 13u;
    // ...but only when the quad still ends at the shape. Anything that
    // spills outside (shadow, outward stroke) padded the quad, and the fill
    // has to stop at the real edge.
    let spills = r.outer_color.a > 0.0 || (r.edge_color.a > 0.0 && r.edge.y > 0.0);
    cov = select(cov, 1.0, plain && sharp && !spills);

    // -- fill ------------------------------------------------------------
    let ang = r.grad.y * 6.28318531;
    let dir = vec2<f32>(cos(ang), sin(ang));
    let extent = max(abs(dir.x) * half.x + abs(dir.y) * half.y, 0.0001);
    let t_lin = clamp(dot(p, dir) / extent * 0.5 + 0.5, 0.0, 1.0);
    let t_rad = clamp(length(p / max(half, vec2<f32>(0.0001))), 0.0, 1.0);
    let gt = select(t_lin, t_rad, r.grad.z > 0.5);
    var fill = select(r.color, mix(r.color, r.color2, gt), r.grad.x > 0.5);

    // Color-picker fills, computed in sRGB then linearized for the surface:
    // 12 = HSV square (color carries the hue), 13 = vertical hue bar.
    let uu = clamp(in.local.x / max(r.size.x, 0.0001), 0.0, 1.0);
    let vv = clamp(in.local.y / max(r.size.y, 0.0001), 0.0, 1.0);
    let sv_srgb = mix(vec3<f32>(1.0), r.color.rgb, uu) * (1.0 - vv);
    fill = select(fill, vec4<f32>(pow(sv_srgb, vec3<f32>(2.2)), 1.0), kind == 12u);
    fill = select(fill, vec4<f32>(pow(hue_ramp(vv), vec3<f32>(2.2)), 1.0), kind == 13u);

    // Grain modulates the fill proportionally, so a dark surface gets a
    // dark tooth and a bright one gets a bright tooth.
    let cell = max(r.grain.y, 1.0);
    let n = (hash21(floor(in.pos.xy / cell)) - 0.5) * 2.0 * r.grain.x;
    fill = vec4<f32>(fill.rgb * (1.0 + n), fill.a);

    // -- drop shadow ------------------------------------------------------
    // Spread rides the distance field itself, so it works identically for a
    // rounded plate and for a gear glyph.
    let d_out = shape_sd(r, p - r.outer.xy) - r.outer.w;
    let out_a = (1.0 - smoothstep(0.0, max(r.outer.z, 0.0001), d_out))
        * r.outer_color.a
        * (1.0 - cov);

    // -- inner shadow -----------------------------------------------------
    let d_in = shape_sd(r, p - r.inner.xy) + r.inner.w;
    let in_a = (1.0 - smoothstep(0.0, max(r.inner.z, 0.0001), -d_in))
        * r.inner_color.a
        * cov;

    // -- bevel ------------------------------------------------------------
    // The distance field's own gradient is the outward surface normal, so
    // the rim light bends around every corner instead of stopping at the
    // straight edges the way a stack of thin rects would.
    let grad_d = vec2<f32>(dpdx(d), dpdy(d));
    let nrm = grad_d / max(length(grad_d), 0.0001);
    let rim = 1.0 - smoothstep(0.0, max(r.bevel.z, 0.0001), -d);
    let lit = clamp(-nrm.y, 0.0, 1.0) * r.bevel.x * rim * cov;
    let shade = clamp(nrm.y, 0.0, 1.0) * r.bevel.y * rim * cov;

    // -- stroke -----------------------------------------------------------
    // A ring riding the silhouette itself: exactly `width` px thick all the
    // way around, corners included. No more oversized rect behind.
    // edge.y aligns it — 0 inside the edge, 1 outside, 0.5 straddling.
    let w = r.edge.x;
    let ring = abs(d + w * (0.5 - r.edge.y)) - w * 0.5;
    let edge_a = select(
        0.0,
        (1.0 - smoothstep(-aa, aa, ring)) * r.edge_color.a,
        w > 0.0,
    );

    var acc = vec4<f32>(0.0);
    acc = over(acc, vec4<f32>(r.outer_color.rgb, out_a));
    acc = over(acc, vec4<f32>(fill.rgb, fill.a * cov));
    acc = over(acc, vec4<f32>(r.inner_color.rgb, in_a));
    acc = over(acc, vec4<f32>(vec3<f32>(1.0), lit));
    acc = over(acc, vec4<f32>(vec3<f32>(0.0), shade));
    acc = over(acc, vec4<f32>(r.edge_color.rgb, edge_a));

    // Image blits stay a straight tinted sample. Sampling must sit in
    // uniform control flow, so it happens unconditionally.
    let img = textureSample(image_tex, image_samp, in.local / max(r.size, vec2<f32>(0.0001)));
    let image = vec4<f32>(img.rgb * r.color.rgb, img.a * r.color.a);
    return select(acc, image, kind == 8u);
}
