// SDF glowing shapes: instanced quads, crisp core + exponential neon halo.
// Composited back-to-front with premultiplied alpha: cores occlude, halos add.
// Kinds: 0 circle/ellipse, 1 box, 2 regular n-gon, 3 line segment,
// 4 path (polyline through `path_verts[b.x ..]`, closed when b.y < 0),
// 5 star field (a hashed scatter across the box `b`).

const TAU: f32 = 6.2831853;

struct Globals {
    resolution: vec2<f32>,
    // Canvas-units -> window-px view: offset + world * scale. The caller
    // (the editor's CanvasView) owns fit, zoom, and pan.
    view_offset: vec2<f32>,
    // x = scale, y = playhead seconds. Time is a *view* input, not document
    // state: the document says how fast a field twinkles, `t` says when we
    // are, and together they make the frame — which is the whole
    // frame = render(project, t) bargain, held at the shader boundary.
    view_scale: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> path_verts: array<vec2<f32>>;

struct VsIn {
    @builtin(vertex_index) vi: u32,
    @location(0) kind_rot: vec2<f32>,
    @location(1) a: vec2<f32>,
    @location(2) b: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) style: vec4<f32>,
    // Gradient end color; a > 0.5 turns the two-color fill on.
    @location(5) color2: vec4<f32>,
    // Kind-specific extras. Stars: seed, twinkle amount, twinkle rate, form.
    @location(6) extra: vec4<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec2<f32>,
    @location(1) kind_rot: vec2<f32>,
    @location(2) a: vec2<f32>,
    @location(3) b: vec2<f32>,
    @location(4) color: vec4<f32>,
    @location(5) style: vec4<f32>,
    @location(6) color2: vec4<f32>,
    @location(7) extra: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    let corner = vec2<f32>(
        f32(in.vi & 1u) * 2.0 - 1.0,
        f32(in.vi >> 1u) * 2.0 - 1.0,
    );
    let kind = u32(in.kind_rot.x + 0.5);

    var center: vec2<f32>;
    var extent: vec2<f32>;
    switch kind {
        case 3u: {
            center = (in.a + in.b) * 0.5;
            extent = abs(in.b - in.a) * 0.5 + vec2<f32>(in.style.y);
        }
        // Box and star field: `b` is the half-extent of a region that spins
        // with the shape, so the quad has to cover its diagonal.
        case 1u, 5u: {
            center = in.a;
            extent = vec2<f32>(length(in.b) + in.style.y);
        }
        case 4u: {
            center = in.a;
            extent = vec2<f32>(in.style.z + in.style.y);
        }
        default: {
            center = in.a;
            extent = vec2<f32>(max(in.b.x, in.b.y) + in.style.y);
        }
    }
    let margin = in.style.x * 4.0 + 12.0;
    let world = center + corner * (extent + vec2<f32>(margin));

    let px = globals.view_offset + world * globals.view_scale.x;
    var ndc = px / globals.resolution * 2.0 - vec2<f32>(1.0);
    ndc.y = -ndc.y;

    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.world = world;
    out.kind_rot = in.kind_rot;
    out.a = in.a;
    out.b = in.b;
    out.color = in.color;
    out.style = in.style;
    out.color2 = in.color2;
    out.extra = in.extra;
    return out;
}

fn sd_box(p: vec2<f32>, half: vec2<f32>) -> f32 {
    let d = abs(p) - half;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

fn sd_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / max(dot(ba, ba), 0.0001), 0.0, 1.0);
    return length(pa - ba * h);
}

// ------------------------------------------------------------------ light

// The glow halo at distance `d`, zero when `radius` is zero.
//
// Zero means off, so a shape with no glow gets no halo at all rather than an
// infinitely tight one — an almost-zero radius still lights the single
// fragment sitting exactly on the boundary, which shows up as a bright rim
// along an edge that was supposed to be hard.
//
// The subtraction windows the falloff to reach exactly zero at the instance
// quad's edge (margin = 4 radii); without it the cutoff reads as a faint
// square around every shape.
fn glow_at(d: f32, radius: f32) -> f32 {
    if radius <= 0.0 {
        return 0.0;
    }
    return max(exp(-max(d, 0.0) / radius) - 0.0183, 0.0) * 1.0187;
}

// How brightly a fragment burns, given how much of it the shape's body
// covers and how much halo reaches it.
//
// The halo is light *spilling out* of the body, so it only lights what the
// body doesn't already cover. It used to be added on top of the core
// unconditionally, and since the exponential is at full strength everywhere
// inside a shape (`max(d, 0)` is 0 for the whole interior), every filled
// shape rendered at 1.55x its own colour — 2.17x at the old default
// brightness. Saturated fills clipped their bright channels first and came
// out pastel, which is why a solid shape only looked like the colour you
// picked with the brightness crushed to nearly nothing. On a hairline stroke
// that overdrive read as neon, so it hid there for a long time.
//
// Now the body renders at exactly its colour at brightness 1.0, and glow is
// something you add rather than something you subtract.
fn lit(core: f32, halo: f32) -> f32 {
    return core + halo * (1.0 - core) * 0.55;
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

// ------------------------------------------------------------- star fields

// Two uncorrelated values in [0,1) from a grid cell. No texture, no buffer:
// the scatter has to be reproducible from the seed alone or a re-render
// would give a different sky.
fn hash22(p: vec2<f32>) -> vec2<f32> {
    var q = fract(vec3<f32>(p.xyx) * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yzx + 33.33);
    return fract((q.xx + q.yz) * q.zy);
}

// One star, centered on `q`. Form 0 is the classic round point; 1 is a
// four-point sparkle (two crossed slivers); 2 is a lens diffraction cross —
// a small core with long thin spikes.
fn star_sd(q: vec2<f32>, r: f32, form: u32) -> f32 {
    if form == 1u {
        let a = abs(q.x) * 3.4 + abs(q.y) - r * 1.4;
        let b = abs(q.x) + abs(q.y) * 3.4 - r * 1.4;
        // The union of two diamonds is not unit-gradient; the constant pulls
        // it back to roughly one, which is all the AA ramp needs.
        return min(a, b) * 0.29;
    }
    if form == 2u {
        let spike = min(
            sd_box(q, vec2<f32>(r * 3.0, r * 0.14)),
            sd_box(q, vec2<f32>(r * 0.14, r * 3.0)),
        );
        return min(length(q) - r * 0.5, spike);
    }
    return length(q) - r;
}

// A whole field in one instance. The region is a grid of cells, each holding
// exactly one hashed star, so a fragment only ever visits its own 3x3
// neighbourhood — five stars and five thousand cost the same.
//
// `p` is the field-local point (rotation already undone), `aa` a pixel's
// width in those same units. Returns premultiplied color, like fs_main.
fn draw_stars(in: VsOut, p: vec2<f32>, aa: f32) -> vec4<f32> {
    let half = max(in.b, vec2<f32>(1.0));
    // Only for the early-out's reach; `glow_at` handles a zero radius itself.
    let glow = max(in.style.x, 0.0);
    let base = max(in.style.y, 0.3);
    // Density is stars across the *canvas*, not across the field: spacing is
    // a property of the sky, so a small patch is fewer stars rather than the
    // same count crammed together, and stretching a field reveals more sky
    // instead of magnifying what's there. 1920.0 is CANVAS_W.
    let cell = max(1920.0 / max(in.style.z, 1.0), 1.0);
    let seed = in.extra.x;
    let tw = clamp(in.extra.y, 0.0, 1.0);
    let rate = in.extra.z;
    let form = u32(in.extra.w + 0.5);
    let t = globals.view_scale.y;

    // Nothing to draw past the region plus the reach of the brightest halo:
    // a big field's quad is mostly empty margin, and this skips it.
    if sd_box(p, half) > glow * 4.0 + base * 4.0 {
        return vec4<f32>(0.0);
    }

    let home = floor(p / cell);
    var core = 0.0;
    var light = 0.0;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let c = home + vec2<f32>(f32(dx), f32(dy));
            let h1 = hash22(c + vec2<f32>(seed, seed * 1.7 + 3.1));
            let h2 = hash22(c + vec2<f32>(seed * 0.37 + 11.7, seed + 5.3));
            // Inset from the cell edge so neighbours can't collide.
            let star = (c + vec2<f32>(0.12) + h1 * 0.76) * cell;
            // A star whose cell falls outside the region doesn't exist: the
            // box you dragged is the edge of the sky.
            let inside = step(abs(star.x), half.x) * step(abs(star.y), half.y);
            // Squared hash biases the spread toward small: mostly faint dust
            // with a few bright ones, which is what reads as depth rather
            // than as polka dots.
            let r = base * (0.35 + h2.x * h2.x * 1.55);
            // Phase off a *different* hash value than brightness. Sharing one
            // correlated the two perfectly, so every star of a given
            // brightness pulsed in step with the rest and the sky twinkled in
            // bands instead of at random.
            let phase = fract(h1.x + h2.x) * TAU;
            let pulse = mix(1.0, 0.5 + 0.5 * sin(t * rate + phase), tw);
            let bright = (0.45 + h2.y * 0.55) * pulse * inside;
            let d = star_sd(p - star, r, form);
            core = max(core, (1.0 - smoothstep(-aa, aa, d)) * bright);
            light += glow_at(d, in.style.x) * bright;
        }
    }

    var col = in.color.rgb;
    if in.color2.a > 0.5 {
        col = mix(col, in.color2.rgb, clamp(p.y / max(half.y * 2.0, 0.001) + 0.5, 0.0, 1.0));
    }
    let e = in.color.a;
    let rgb = col * e * lit(core, light);
    return vec4<f32>(rgb, core * (1.0 - min(in.style.w, 1.0)));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let kind = u32(in.kind_rot.x + 0.5);
    let rot = in.kind_rot.y;
    // A pixel's width in canvas units. Taken here, in uniform control flow,
    // because a star field antialiases nine distance fields inside a loop and
    // derivatives can't be asked for down there.
    let world_aa = max(fwidth(in.world.x), 0.0001);

    var d: f32;
    var p = vec2<f32>(0.0);
    if kind == 3u {
        d = sd_segment(in.world, in.a, in.b) - in.style.y;
    } else {
        p = in.world - in.a;
        let cs = cos(-rot);
        let sn = sin(-rot);
        p = vec2<f32>(p.x * cs - p.y * sn, p.x * sn + p.y * cs);
        // A field is many shapes at once, so it composites itself rather
        // than handing one distance back to the single-silhouette path below.
        if kind == 5u {
            return draw_stars(in, p, world_aa);
        }
        if kind == 0u {
            // Ellipse (b = radii): scaled-space approximation — near-exact
            // for circles, good enough for glow when squashed.
            d = (length(p / max(in.b, vec2<f32>(0.001))) - 1.0) * min(in.b.x, in.b.y);
        } else if kind == 1u {
            d = sd_box(p, in.b);
        } else if kind == 4u {
            // Polyline: min distance over every segment — one continuous
            // neon tube, corners welded.
            let start = u32(in.b.x);
            let cnt = u32(abs(in.b.y));
            let total = arrayLength(&path_verts);
            var md = 1e6;
            for (var k = 0u; k + 1u < cnt; k++) {
                if start + k + 1u >= total { break; }
                md = min(md, sd_segment(p, path_verts[start + k], path_verts[start + k + 1u]));
            }
            if in.b.y < 0.0 && cnt > 2u && start + cnt - 1u < total {
                md = min(md, sd_segment(p, path_verts[start + cnt - 1u], path_verts[start]));
            }
            d = md - in.style.y;
        } else {
            // Negated: canvas y points down, so flip the ngon point-up.
            d = sd_ngon(-p, in.b.x, max(in.style.z, 3.0));
        }
        // Outline mode: carve the fill into a stroke (paths already are one).
        if in.style.y > 0.0 && kind != 4u {
            d = abs(d) - in.style.y;
        }
    }

    let px = max(fwidth(d), 0.0001);
    let core = 1.0 - smoothstep(-px, px, d);
    let halo = glow_at(d, in.style.x);
    let e = in.color.a;
    // Two-color gradient fill: radial for circles, along the segment for
    // lines, along local Y (riding the shape's rotation) for the rest.
    var col = in.color.rgb;
    if in.color2.a > 0.5 {
        var t: f32;
        if kind == 3u {
            let ba = in.b - in.a;
            t = clamp(dot(in.world - in.a, ba) / max(dot(ba, ba), 0.0001), 0.0, 1.0);
        } else if kind == 0u {
            t = clamp(length(p / max(in.b, vec2<f32>(0.001))), 0.0, 1.0);
        } else {
            var half_y = in.b.x;
            if kind == 1u { half_y = in.b.y; }
            if kind == 4u { half_y = in.style.z; }
            t = clamp(p.y / max(half_y * 2.0, 0.001) + 0.5, 0.0, 1.0);
        }
        col = mix(col, in.color2.rgb, t);
    }
    let rgb = col * e * lit(core, halo);
    // Premultiplied output: alpha is the core's coverage, so the crisp body
    // occludes shapes behind it (real z-order) while the halo, at alpha 0,
    // stays pure additive light. style.w: 1 = pure light (guides, additive
    // shapes); 2 = dashed light (selection ants, diagonal-striped).
    let overlay = in.style.w;
    let stripe = step(0.5, fract((in.world.x + in.world.y) * 0.055));
    let lit = select(1.0, stripe, overlay > 1.5);
    return vec4<f32>(rgb * lit, core * (1.0 - min(overlay, 1.0)));
}
