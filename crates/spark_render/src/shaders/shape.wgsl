// SDF glowing shapes: instanced quads, crisp core + exponential neon halo.
// Composited back-to-front with premultiplied alpha: cores occlude, halos add.
// Kinds: 0 circle, 1 box, 2 regular n-gon, 3 line segment.

struct Globals {
    resolution: vec2<f32>,
    vp_origin: vec2<f32>,
    vp_size: vec2<f32>,
    canvas: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

struct VsIn {
    @builtin(vertex_index) vi: u32,
    @location(0) kind_rot: vec2<f32>,
    @location(1) a: vec2<f32>,
    @location(2) b: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) style: vec4<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec2<f32>,
    @location(1) kind_rot: vec2<f32>,
    @location(2) a: vec2<f32>,
    @location(3) b: vec2<f32>,
    @location(4) color: vec4<f32>,
    @location(5) style: vec4<f32>,
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
        default: {
            center = in.a;
            extent = vec2<f32>(in.b.x + in.style.y);
        }
    }
    let margin = in.style.x * 4.0 + 12.0;
    let world = center + corner * (extent + vec2<f32>(margin));

    // Aspect-fit the 1920x1080 canvas into the viewport region, centered.
    let scale = min(
        globals.vp_size.x / globals.canvas.x,
        globals.vp_size.y / globals.canvas.y,
    );
    let offset = globals.vp_origin + (globals.vp_size - globals.canvas * scale) * 0.5;
    let px = offset + world * scale;
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
    if kind == 3u {
        d = sd_segment(in.world, in.a, in.b) - in.style.y;
    } else {
        var p = in.world - in.a;
        let cs = cos(-rot);
        let sn = sin(-rot);
        p = vec2<f32>(p.x * cs - p.y * sn, p.x * sn + p.y * cs);
        if kind == 0u {
            d = length(p) - in.b.x;
        } else if kind == 1u {
            d = sd_box(p, in.b);
        } else {
            d = sd_ngon(p, in.b.x, max(in.style.z, 3.0));
        }
        // Outline mode: carve the fill into a stroke.
        if in.style.y > 0.0 {
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
    let rgb = in.color.rgb * (core * e + halo * e * 0.55);
    // Premultiplied output: alpha is the core's coverage, so the crisp body
    // occludes shapes behind it (real z-order) while the halo, at alpha 0,
    // stays pure additive light. style.w = 1 marks overlay shapes (selection
    // halo) that only ever add light.
    return vec4<f32>(rgb, core * (1.0 - in.style.w));
}
