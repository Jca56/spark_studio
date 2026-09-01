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
    // Where along the surface the blend happens: [start, end, on, unused].
    // Off (0) runs it corner to corner, which is all it could ever do.
    grad_span: vec4<f32>,
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
    // [rotation in turns, unused x3]
    xform: vec4<f32>,
    // [dash px, gap px, phase px, unused]
    dash: vec4<f32>,
};

// sRGB transfer, both ways. The palette is stored linear for the pipeline;
// these get back to the space a colour code was written in.
fn srgb_of_lin(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn lin_of_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((max(c, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

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

// The instance's silhouette as one signed distance field: the rounded box
// for fills, the glyph's own field for icons. Everything downstream —
// shadow, stroke, bevel, inner shadow — is computed from this, which is why
// icons now get borders and glows for free.
//
// It branches on the kind rather than evaluating every candidate. All the
// fragments of one instance take the same path, so the branch is uniform
// across a wave, and the function holds no derivatives, so branching is
// legal. That matters: it is called up to three times per pixel (the drop
// shadow and the inner shadow each sample it at an offset), and panel fills
// cover most of the screen — they must not pay for eighteen glyph fields
// they will never use.
fn shape_sd(r: Rect, raw: vec2<f32>) -> f32 {
    let kind = u32(r.icon.x + 0.5);
    let half = r.size * 0.5;

    // Rotation is applied to the sample point, which spins the silhouette
    // the other way — and everything derived from it comes along.
    let rot = r.xform.x * TAU;
    let ca = cos(rot);
    let sa = sin(rot);
    let p = vec2<f32>(raw.x * ca + raw.y * sa, raw.y * ca - raw.x * sa);

    var d = sd_round_box(p, half, r.radii);
    // Fills (0), the image blit (8) and the picker's two ramps (12, 13) are
    // the rounded box, and they are the common case — leave early.
    if kind == 0u || kind == 8u || kind == 12u || kind == 13u {
        return d;
    }
    // The wedge (25): a knob's chicken-head pointer. Base at `icon.w` of
    // the half-width, apex on the right edge, half-height `icon.y` px —
    // rotation (above) aims it like everything else.
    if kind == 25u {
        let inner = r.icon.w * half.x;
        let hw = r.icon.y;
        return sd_tri(
            p,
            vec2<f32>(inner, -hw),
            vec2<f32>(inner, hw),
            vec2<f32>(half.x, 0.0),
        );
    }

    let t = r.icon.y;
    // Glyph radius: icon.w overrides the default fraction when > 0.
    let rf = select(0.20, r.icon.w, r.icon.w > 0.0);
    let g = min(r.size.x, r.size.y) * rf;

    switch kind {
        case 1u: {
            d = sd_box(p, vec2<f32>(g, t));
        }
        case 2u: {
            d = abs(sd_box(p, vec2<f32>(g * 0.92, g * 0.92))) - t;
        }
        case 3u: {
            d = min(
                sd_seg(p, vec2<f32>(-g, -g) * 0.92, vec2<f32>(g, g) * 0.92),
                sd_seg(p, vec2<f32>(-g, g) * 0.92, vec2<f32>(g, -g) * 0.92),
            ) - t;
        }
        case 4u: {
            d = sd_triangle(
                p,
                vec2<f32>(-0.42 * g, -0.95 * g),
                vec2<f32>(-0.42 * g, 0.62 * g),
                vec2<f32>(0.52 * g, 0.02 * g),
            );
        }
        case 5u: {
            d = abs(length(p) - 0.78 * g) - t;
        }
        case 6u: {
            let sides = select(5.0, r.icon.z, r.icon.z >= 3.0);
            d = abs(sd_ngon(-p, 0.85 * g, sides)) - t;
        }
        case 7u: {
            d = sd_seg(p, vec2<f32>(-0.7 * g, 0.65 * g), vec2<f32>(0.7 * g, -0.65 * g)) - t;
        }
        case 9u: {
            d = sd_triangle(
                p,
                vec2<f32>(-0.5 * g, -0.8 * g),
                vec2<f32>(-0.5 * g, 0.8 * g),
                vec2<f32>(0.8 * g, 0.0),
            );
        }
        case 10u: {
            d = min(
                sd_box(p - vec2<f32>(-0.42 * g, 0.0), vec2<f32>(0.18 * g, 0.75 * g)),
                sd_box(p - vec2<f32>(0.42 * g, 0.0), vec2<f32>(0.18 * g, 0.75 * g)),
            );
        }
        case 11u: {
            d = min(
                sd_seg(p, vec2<f32>(-0.8 * g, 0.5 * g), vec2<f32>(-0.27 * g, -0.5 * g)),
                min(
                    sd_seg(p, vec2<f32>(-0.27 * g, -0.5 * g), vec2<f32>(0.27 * g, 0.5 * g)),
                    sd_seg(p, vec2<f32>(0.27 * g, 0.5 * g), vec2<f32>(0.8 * g, -0.5 * g)),
                ),
            ) - t;
        }
        // Filled diamond (keyframe marker): L1-norm distance.
        case 14u: {
            d = (abs(p.x) + abs(p.y)) - 0.82 * g;
        }
        // Cogwheel: a disc whose rim ripples with 8 square-ish teeth, minus
        // a hub hole.
        case 15u: {
            let teeth = clamp(sin(atan2(p.y, p.x) * 8.0) * 2.0, -1.0, 1.0) * 0.11 * g;
            d = max(length(p) - (0.70 * g + teeth), -(length(p) - 0.30 * g));
        }
        // Eye: almond outline (two-circle intersection) + pupil; the hidden
        // variant swaps the pupil for a diagonal slash.
        case 16u, 17u: {
            let er = 0.85 * g;
            let rr = 1.05 * er;
            let ec = 0.55 * er;
            let almond = abs(max(
                length(p - vec2<f32>(0.0, ec)) - rr,
                length(p + vec2<f32>(0.0, ec)) - rr,
            )) - t;
            let pupil = length(p) - 0.25 * er;
            let slash = sd_seg(p, vec2<f32>(-er, er), vec2<f32>(er, -er)) - t;
            d = min(almond, select(pupil, slash, kind == 17u));
        }
        // Capsule: endpoints ride in `radii`, half-thickness in icon.y.
        case 18u: {
            d = sd_seg(p, r.radii.xy, r.radii.zw) - t;
        }
        // Arc: start and sweep ride in `radii`, half-thickness in icon.y.
        case 19u: {
            d = sd_arc(p, r.radii.x * TAU, r.radii.y * TAU, g, t);
        }
        // Star cluster: three four-point sparkles at descending sizes. One
        // sparkle alone reads as "shine"; three read as a field of them.
        case 21u: {
            d = min(
                sd_sparkle(p - vec2<f32>(-0.20 * g, -0.08 * g), 0.80 * g),
                min(
                    sd_sparkle(p - vec2<f32>(0.55 * g, 0.48 * g), 0.44 * g),
                    sd_sparkle(p - vec2<f32>(0.50 * g, -0.58 * g), 0.32 * g),
                ),
            );
        }
        // Die: a rounded square face with five pips cut out of it, so the
        // pips read as holes rather than dots sitting on a plate.
        case 22u: {
            let face = sd_round_box(p, vec2<f32>(0.82 * g, 0.82 * g), vec4<f32>(0.22 * g));
            let o = 0.42 * g;
            let pr = 0.15 * g;
            var pips = length(p) - pr;
            pips = min(pips, length(p - vec2<f32>(-o, -o)) - pr);
            pips = min(pips, length(p - vec2<f32>(o, -o)) - pr);
            pips = min(pips, length(p - vec2<f32>(-o, o)) - pr);
            pips = min(pips, length(p - vec2<f32>(o, o)) - pr);
            d = max(face, -pips);
        }
        // Chevron: a "v" opening upward, so it points down at rest.
        case 20u: {
            d = min(
                sd_seg(p, vec2<f32>(-0.72 * g, -0.36 * g), vec2<f32>(0.0, 0.36 * g)),
                sd_seg(p, vec2<f32>(0.0, 0.36 * g), vec2<f32>(0.72 * g, -0.36 * g)),
            ) - t;
        }
        // Cube: a hexagon outline with the three edges that meet at the
        // near corner — the kind glyph for a mesh object.
        case 23u: {
            let s = 0.85 * g;
            let a0 = vec2<f32>(0.0, -s);
            let a1 = vec2<f32>(0.866 * s, -0.5 * s);
            let a2 = vec2<f32>(0.866 * s, 0.5 * s);
            let a3 = vec2<f32>(0.0, s);
            let a4 = vec2<f32>(-0.866 * s, 0.5 * s);
            let a5 = vec2<f32>(-0.866 * s, -0.5 * s);
            let c = vec2<f32>(0.0, 0.0);
            var e = sd_seg(p, a0, a1);
            e = min(e, sd_seg(p, a1, a2));
            e = min(e, sd_seg(p, a2, a3));
            e = min(e, sd_seg(p, a3, a4));
            e = min(e, sd_seg(p, a4, a5));
            e = min(e, sd_seg(p, a5, a0));
            e = min(e, sd_seg(p, c, a1));
            e = min(e, sd_seg(p, c, a5));
            e = min(e, sd_seg(p, c, a3));
            d = e - t;
        }
        // Sun: a ring with eight rays — the kind glyph for a light.
        case 24u: {
            var e = abs(length(p) - 0.36 * g);
            for (var k = 0; k < 8; k++) {
                let a = f32(k) * TAU / 8.0;
                let dir = vec2<f32>(cos(a), sin(a));
                e = min(e, sd_seg(p, dir * 0.56 * g, dir * 0.85 * g));
            }
            d = e - t;
        }
        default: {}
    }
    return d;
}

// ------------------------------------------------------------------ dashes

// How far along the rounded box's outline the nearest boundary point sits,
// walking clockwise from the top-left corner. Non-uniform corner radii fall
// back to the top-left one for this walk only.
fn box_outline_t(p: vec2<f32>, half: vec2<f32>, radius: f32) -> f32 {
    let k = clamp(radius, 0.0, min(half.x, half.y));
    let sx = max(half.x - k, 0.0);
    let sy = max(half.y - k, 0.0);
    let quarter = HALF_PI * k;
    // Run lengths, accumulated: top, tr corner, right, br, bottom, bl, left.
    let b1 = 2.0 * sx;
    let b2 = b1 + quarter;
    let b3 = b2 + 2.0 * sy;
    let b4 = b3 + quarter;
    let b5 = b4 + 2.0 * sx;
    let b6 = b5 + quarter;
    let b7 = b6 + 2.0 * sy;

    // Corner arcs, each measured from the edge that feeds into it.
    let tr = b1 + atan2(p.x - sx, -(p.y + sy)) * k;
    let br = b3 + atan2(p.y - sy, p.x - sx) * k;
    let bl = b5 + atan2(-(p.x + sx), p.y - sy) * k;
    let tl = b7 + atan2(-(p.y + sy), -(p.x + sx)) * k;

    let on_x = abs(p.x) <= sx;
    let on_y = abs(p.y) <= sy;
    // Straight runs first, then the corner the point falls into.
    var t = select(bl, br, p.x > 0.0);
    t = select(t, select(tl, tr, p.x > 0.0), p.y < 0.0);
    t = select(t, select(b2 + p.y + sy, b6 + sy - p.y, p.x < 0.0), on_y);
    t = select(t, select(b4 + sx - p.x, p.x + sx, p.y < 0.0), on_x);
    return t;
}

// Distance along whatever outline the shape has, for dashing. Lines measure
// along the segment and arcs along the band, since on those the dashes break
// the shape itself rather than a border around it.
fn outline_t(r: Rect, raw: vec2<f32>, half: vec2<f32>) -> f32 {
    let kind = u32(r.icon.x + 0.5);
    let rf = select(0.20, r.icon.w, r.icon.w > 0.0);
    let g = min(r.size.x, r.size.y) * rf;
    // Walk the rotated outline, so dashes turn with the shape they mark.
    let rot = r.xform.x * TAU;
    let ca = cos(rot);
    let sa = sin(rot);
    let p = vec2<f32>(raw.x * ca + raw.y * sa, raw.y * ca - raw.x * sa);

    let ab = r.radii.zw - r.radii.xy;
    let seg_t = dot(p - r.radii.xy, ab) / max(length(ab), 0.0001);

    var arc_t = atan2(p.x, -p.y) - r.radii.x * TAU;
    arc_t = (arc_t - TAU * floor(arc_t / TAU)) * g;

    // Glyphs with no natural outline walk the angle around their center,
    // which is enough to break a border into even ticks.
    var t = atan2(p.x, -p.y) * g;
    t = select(t, box_outline_t(p, half, r.radii.x), kind == 0u);
    t = select(t, seg_t, kind == 18u);
    t = select(t, arc_t, kind == 19u);
    return t;
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
    cov = select(cov, 1.0, plain && sharp && !spills && r.xform.x == 0.0);

    // -- dashes -----------------------------------------------------------
    // Guarded, like everything below it: walking an outline is not free and
    // almost nothing in the chrome is dashed. The condition is the same for
    // every fragment of an instance, so the branch costs nothing.
    var fill_dash = 1.0;
    var edge_dash = 1.0;
    let period = r.dash.x + r.dash.y;
    if r.dash.x > 0.0 && period > 0.0 {
        let walk = outline_t(r, p, half) + r.dash.z;
        let phase = walk - period * floor(walk / period);
        // Symmetric depth into the dash, so both of its ends antialias.
        let dashed = smoothstep(-0.6, 0.6, min(phase, r.dash.x - phase));
        // A line or an arc is all edge and no interior, so there the dashes
        // break the shape itself rather than a border drawn around it.
        if kind == 18u || kind == 19u {
            fill_dash = dashed;
        } else {
            edge_dash = dashed;
        }
    }

    // -- fill ------------------------------------------------------------
    let ang = r.grad.y * 6.28318531;
    let dir = vec2<f32>(cos(ang), sin(ang));
    let extent = max(abs(dir.x) * half.x + abs(dir.y) * half.y, 0.0001);
    let t_lin = clamp(dot(p, dir) / extent * 0.5 + 0.5, 0.0, 1.0);
    let t_rad = clamp(length(p / max(half, vec2<f32>(0.0001))), 0.0, 1.0);
    let t_full = select(t_lin, t_rad, r.grad.z > 0.5);
    // The blend can be confined to a band: everything before `start` is the
    // fill, everything after `end` is the far colour, and the transition
    // happens in between. Without this a gradient always ran the whole
    // surface, so "a wash across the left quarter" was unaskable.
    let g0 = r.grad_span.x;
    let g1 = max(r.grad_span.y, g0 + 0.0001);
    let gt = select(
        t_full,
        clamp((t_full - g0) / (g1 - g0), 0.0, 1.0),
        r.grad_span.z > 0.5,
    );
    // Mixed in **display** space, not linear light. A linear lerp between
    // two colours is not the ramp anyone means by "gradient": at 3% of the
    // way across, linear 0.03 encodes to sRGB 0.20, so a fifth of the
    // brightness has already happened in the first thirtieth of the
    // surface. It read as the far colour taking ~98% of the run and the
    // fill getting a sliver. Interpolating where the eye is roughly linear
    // puts the halfway point halfway.
    let blended = vec4<f32>(
        lin_of_srgb(mix(srgb_of_lin(r.color.rgb), srgb_of_lin(r.color2.rgb), gt)),
        // Alpha is coverage, not light, so it stays a straight lerp.
        mix(r.color.a, r.color2.a, gt),
    );
    var fill = select(r.color, blended, r.grad.x > 0.5);

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
    // rounded plate and for a gear glyph. Sampling the field a second time
    // is the expensive part, so it only happens when there is a shadow.
    var out_a = 0.0;
    if r.outer_color.a > 0.0 {
        let d_out = shape_sd(r, p - r.outer.xy) - r.outer.w;
        out_a = (1.0 - smoothstep(0.0, max(r.outer.z, 0.0001), d_out))
            * r.outer_color.a
            * (1.0 - cov);
    }

    // -- inner shadow -----------------------------------------------------
    var in_a = 0.0;
    if r.inner_color.a > 0.0 {
        let d_in = shape_sd(r, p - r.inner.xy) + r.inner.w;
        in_a = (1.0 - smoothstep(0.0, max(r.inner.z, 0.0001), -d_in)) * r.inner_color.a * cov;
    }

    // -- bevel ------------------------------------------------------------
    // The distance field's own gradient is the outward surface normal, so
    // the rim light bends around every corner instead of stopping at the
    // straight edges the way a stack of thin rects would.
    let grad_d = vec2<f32>(dpdx(d), dpdy(d));
    let nrm = grad_d / max(length(grad_d), 0.0001);
    let rim = 1.0 - smoothstep(0.0, max(r.bevel.z, 0.0001), -d);
    // bevel.w flips the light to come from below — a recess's bottom lip
    // catches light where a raised face catches it on top.
    let ny = select(nrm.y, -nrm.y, r.bevel.w > 0.5);
    let lit = clamp(-ny, 0.0, 1.0) * r.bevel.x * rim * cov;
    let shade = clamp(ny, 0.0, 1.0) * r.bevel.y * rim * cov;

    // -- stroke -----------------------------------------------------------
    // A ring riding the silhouette itself: exactly `width` px thick all the
    // way around, corners included. No more oversized rect behind.
    // edge.y aligns it — 0 inside the edge, 1 outside, 0.5 straddling.
    let w = r.edge.x;
    let ring = abs(d + w * (0.5 - r.edge.y)) - w * 0.5;
    let edge_a = select(
        0.0,
        (1.0 - smoothstep(-aa, aa, ring)) * r.edge_color.a * edge_dash,
        w > 0.0,
    );

    var acc = vec4<f32>(0.0);
    acc = over(acc, vec4<f32>(r.outer_color.rgb, out_a));
    acc = over(acc, vec4<f32>(fill.rgb, fill.a * cov * fill_dash));
    acc = over(acc, vec4<f32>(r.inner_color.rgb, in_a));
    acc = over(acc, vec4<f32>(vec3<f32>(1.0), lit));
    acc = over(acc, vec4<f32>(vec3<f32>(0.0), shade));
    acc = over(acc, vec4<f32>(r.edge_color.rgb, edge_a));

    // Image blits stay a straight tinted sample. `textureSampleLevel` takes
    // an explicit LOD instead of deriving one, which is what makes it legal
    // inside a branch — and the branch spares every panel in the chrome a
    // texture fetch it would only throw away. The texture has one mip, so
    // level 0 is exactly what the implicit sample would have picked anyway.
    if kind == 8u {
        let uv = in.local / max(r.size, vec2<f32>(0.0001));
        let img = textureSampleLevel(image_tex, image_samp, uv, 0.0);
        return vec4<f32>(img.rgb * r.color.rgb, img.a * r.color.a);
    }
    return acc;
}
