// SDF glowing shapes: instanced quads, crisp core + exponential neon halo.
// Composited back-to-front with premultiplied alpha: cores occlude, halos add.
// Kinds: 0 circle/ellipse, 1 box, 2 regular n-gon, 3 line segment,
// 4 path (polyline through `path_verts[b.x ..]`, closed when b.y < 0).

struct Globals {
    resolution: vec2<f32>,
    // Canvas-units -> window-px view: offset + world * scale. The caller
    // (the editor's CanvasView) owns fit, zoom, and pan.
    view_offset: vec2<f32>,
    view_scale: vec4<f32>, // x = scale, rest padding
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
        case 1u: {
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

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let kind = u32(in.kind_rot.x + 0.5);
    let rot = in.kind_rot.y;

    var d: f32;
    var p = vec2<f32>(0.0);
    if kind == 3u {
        d = sd_segment(in.world, in.a, in.b) - in.style.y;
    } else {
        p = in.world - in.a;
        let cs = cos(-rot);
        let sn = sin(-rot);
        p = vec2<f32>(p.x * cs - p.y * sn, p.x * sn + p.y * cs);
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
    // Window the halo so it reaches exactly zero at the instance quad edge
    // (margin = 4 glow radii) — otherwise the cutoff shows as a faint square.
    let g = max(in.style.x, 0.001);
    let halo = max(exp(-max(d, 0.0) / g) - 0.0183, 0.0) * 1.0187;
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
    let rgb = col * (core * e + halo * e * 0.55);
    // Premultiplied output: alpha is the core's coverage, so the crisp body
    // occludes shapes behind it (real z-order) while the halo, at alpha 0,
    // stays pure additive light. style.w: 1 = pure light (guides, additive
    // shapes); 2 = dashed light (selection ants, diagonal-striped).
    let overlay = in.style.w;
    let stripe = step(0.5, fract((in.world.x + in.world.y) * 0.055));
    let lit = select(1.0, stripe, overlay > 1.5);
    return vec4<f32>(rgb * lit, core * (1.0 - min(overlay, 1.0)));
}
