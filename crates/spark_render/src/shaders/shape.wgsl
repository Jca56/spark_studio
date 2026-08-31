// SDF glowing shapes: instanced quads, crisp core + exponential neon halo.
// Composited back-to-front with premultiplied alpha: cores occlude, halos add.
// Every shape is flat: its field is evaluated on its own plane, in canvas
// units, and a per-instance model matrix places that plane in the scene.
// Kinds: 0 circle/ellipse, 1 box, 2 regular n-gon, 3 line segment,
// 4 path (polyline through `path_verts[b.x ..]`, closed when b.y < 0),
// 5 star field (a hashed scatter across the box `b`).

const TAU: f32 = 6.2831853;

// How far a halo reaches, in glow radii. The exponential is windowed to hit
// exactly zero here so the quad's edge never shows as a faint square; the
// floor is exp(-HALO_REACH) and the gain puts the boundary back at full
// strength. Three radii (a 5% tail) rather than four (2%): the quad area
// at large glows is mostly tail, and every fragment of it is shaded.
const HALO_REACH: f32 = 3.0;
const HALO_FLOOR: f32 = 0.0498;
const HALO_GAIN: f32 = 1.0524;
// A halo narrower than this on screen is drawn with its body rather than in
// the halo layer: it costs next to nothing there, and it would go soft at
// the halo layer's resolution.
const SMALL_HALO_PX: f32 = 6.0;

struct Globals {
    // World -> the frame's clip space: the camera's view and projection with
    // the CanvasView's fit, zoom and pan composed in, so a point on the
    // canvas plane lands on exactly the window pixel the flat 2D map used
    // to put it on. The caller (the editor's CanvasView and Camera) owns it.
    view_proj: mat4x4<f32>,
    // x = playhead seconds. Time is a *view* input, not document state: the
    // document says how fast a field twinkles, `t` says when we are, and
    // together they make the frame — which is the whole
    // frame = render(project, t) bargain, held at the shader boundary.
    // y = which layer this pass draws (0 whole shapes, 1 bodies, 2 halos —
    // see `parts`), z = the frame's own px per canvas unit, the same in
    // every layer's pass, so the small-halo decision can't differ between
    // the pass that would keep a halo and the pass that would drop it.
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> path_verts: array<vec2<f32>>;
// One matrix per instance: the object's plane -> the world. This is what
// tilts, turns and moves a shape's plane through the scene; identity for a
// shape that has never left the canvas.
@group(0) @binding(2) var<storage, read> models: array<mat4x4<f32>>;

struct VsIn {
    @builtin(vertex_index) vi: u32,
    @builtin(instance_index) ii: u32,
    @location(0) kind_rot: vec2<f32>,
    @location(1) a: vec2<f32>,
    @location(2) b: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) style: vec4<f32>,
    // Gradient end color; a > 0.5 turns the two-color fill on.
    @location(5) color2: vec4<f32>,
    // Kind-specific extras. Stars: seed, twinkle amount, twinkle rate, form.
    @location(6) extra: vec4<f32>,
    // How the shape composites over what's behind it: opacity, then unused.
    @location(7) over: vec4<f32>,
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
    @location(8) over: vec4<f32>,
};

// Which of a shape's two parts this pass draws: x weights the body, y the
// halo. The whole-shape pass draws both. The stage splits them: bodies at
// full resolution in quads that hug them, halos at a lower resolution in
// the wide quads a halo needs — which is where the fragment budget went,
// since the halo is a smooth exponential that never needed 4K sampling.
// A halo that is small on screen stays with its body, and a star field's
// light is per star and stays with the field.
fn parts(kind: u32, r: f32) -> vec2<f32> {
    let layer = u32(globals.params.y + 0.5);
    if layer == 0u {
        return vec2<f32>(1.0, 1.0);
    }
    let with_body = kind == 5u || r * globals.params.z < SMALL_HALO_PX;
    if layer == 1u {
        return vec2<f32>(1.0, select(0.0, 1.0, with_body));
    }
    return vec2<f32>(0.0, select(1.0, 0.0, with_body || r <= 0.0));
}

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
        // A mesh or a light: the mesh pass draws one and the editor
        // gizmos the other. The instance here is only the stack slot — no
        // quad at all.
        case 6u, 7u: {
            center = in.a;
            extent = vec2<f32>(0.0);
        }
        default: {
            center = in.a;
            extent = vec2<f32>(max(in.b.x, in.b.y) + in.style.y);
        }
    }
    // The quad reaches past the body only as far as what this pass draws:
    // a body alone needs a sliver for antialiasing, a halo needs its reach.
    // A shape with nothing in this layer collapses to a point and costs no
    // fragments at all.
    let r = max(in.style.x, 0.0);
    let part = parts(kind, r);
    let margin = select(12.0, r * HALO_REACH + 12.0, part.y > 0.0);
    var reach = extent + vec2<f32>(margin);
    if part.x + part.y <= 0.0 || kind >= 6u {
        reach = vec2<f32>(0.0);
    }
    let world = center + corner * reach;

    var out: VsOut;
    // Plane-local -> world -> clip. `world` rides through to the fragment
    // stage perspective-correct, so a turned plane's field is evaluated on
    // the plane, not on the screen.
    out.pos = globals.view_proj * models[in.ii] * vec4<f32>(world, 0.0, 1.0);
    out.world = world;
    out.kind_rot = in.kind_rot;
    out.a = in.a;
    out.b = in.b;
    out.color = in.color;
    out.style = in.style;
    out.color2 = in.color2;
    out.extra = in.extra;
    out.over = in.over;
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
// quad's edge (margin = HALO_REACH radii); without it the cutoff reads as a
// faint square around every shape.
fn glow_at(d: f32, radius: f32) -> f32 {
    if radius <= 0.0 {
        return 0.0;
    }
    return max(exp(-max(d, 0.0) / radius) - HALO_FLOOR, 0.0) * HALO_GAIN;
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
//
// The two terms are separable, which is what lets the stage draw bodies and
// halos in different passes: `part` weights each, and the whole-shape pass
// weights both at one.
fn lit_parts(core: f32, halo: f32, part: vec2<f32>) -> f32 {
    return core * part.x + halo * (1.0 - core) * 0.55 * part.y;
}

fn lit(core: f32, halo: f32) -> f32 {
    return lit_parts(core, halo, vec2<f32>(1.0, 1.0));
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
    let t = globals.params.x;

    // Nothing to draw past the region plus the reach of the brightest halo:
    // a big field's quad is mostly empty margin, and this skips it.
    if sd_box(p, half) > glow * HALO_REACH + base * 4.0 {
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

// What the shape looks like at full strength. Split from `fs_main` so
// opacity has exactly one place to apply: the star path returns early from
// here, and a second exit is a second thing to forget.
fn shade(in: VsOut) -> vec4<f32> {
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
        // Its light stays with it, so the halo layer has nothing of it.
        if kind == 5u {
            if parts(kind, max(in.style.x, 0.0)).x <= 0.0 {
                return vec4<f32>(0.0);
            }
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
    let part = parts(kind, max(in.style.x, 0.0));
    let rgb = col * e * lit_parts(core, halo, part);
    // Premultiplied output: alpha is the core's coverage, so the crisp body
    // occludes shapes behind it (real z-order) while the halo, at alpha 0,
    // stays pure additive light. style.w: 1 = pure light (guides, additive
    // shapes); 2 = dashed light (selection ants, diagonal-striped).
    let overlay = in.style.w;
    let stripe = step(0.5, fract((in.world.x + in.world.y) * 0.055));
    let lit = select(1.0, stripe, overlay > 1.5);
    return vec4<f32>(rgb * lit, core * part.x * (1.0 - min(overlay, 1.0)));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Premultiplied output makes a fade one multiply on the whole result:
    // rgb is light already scaled by coverage, alpha *is* the coverage. At
    // opacity 0 the shape emits nothing and occludes nothing, which is the
    // only reading of "gone" that composites correctly against the layers
    // behind it. A halo (alpha 0, pure light) fades by its colour alone, and
    // an additive shape fades without ever starting to occlude.
    return shade(in) * in.over.x;
}
