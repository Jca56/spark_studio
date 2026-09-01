// Spark's signed-distance library: the shapes SparkUI is built out of,
// plus the few color helpers the material stack composites with.
//
// wgpu has no #include, so `UiPass` concatenates this file ahead of
// `ui.wgsl` at build time. Keeping it separate is not just tidiness — a
// distance field is the same thing whether it is drawing a button or a
// keyframe diamond, and this is where that vocabulary lives.

const TAU: f32 = 6.28318531;
const HALF_PI: f32 = 1.57079633;

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

// Exact triangle field (IQ's formulation): negative inside, so the wedge
// pointer takes bevels, shadows and strokes like every other silhouette.
fn sd_tri(p: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> f32 {
    let e0 = p1 - p0;
    let e1 = p2 - p1;
    let e2 = p0 - p2;
    let v0 = p - p0;
    let v1 = p - p1;
    let v2 = p - p2;
    let pq0 = v0 - e0 * clamp(dot(v0, e0) / dot(e0, e0), 0.0, 1.0);
    let pq1 = v1 - e1 * clamp(dot(v1, e1) / dot(e1, e1), 0.0, 1.0);
    let pq2 = v2 - e2 * clamp(dot(v2, e2) / dot(e2, e2), 0.0, 1.0);
    let sgn = sign(e0.x * e2.y - e0.y * e2.x);
    let d = min(
        min(
            vec2<f32>(dot(pq0, pq0), sgn * (v0.x * e0.y - v0.y * e0.x)),
            vec2<f32>(dot(pq1, pq1), sgn * (v1.x * e1.y - v1.y * e1.x)),
        ),
        vec2<f32>(dot(pq2, pq2), sgn * (v2.x * e2.y - v2.y * e2.x)),
    );
    return -sqrt(d.x) * sign(d.y);
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

// A ring segment with round caps. Angles run clockwise from straight up,
// matching how a knob reads. `half` is half the band's thickness.
fn sd_arc(p: vec2<f32>, start: f32, sweep: f32, radius: f32, half: f32) -> f32 {
    // Where p sits around the ring, as an offset from the arc's start.
    var t = atan2(p.x, -p.y) - start;
    t = t - TAU * floor(t / TAU);
    // The band itself, and the two round caps it ends in.
    let band = abs(length(p) - radius) - half;
    let a = vec2<f32>(sin(start), -cos(start)) * radius;
    let b = vec2<f32>(sin(start + sweep), -cos(start + sweep)) * radius;
    let caps = min(length(p - a), length(p - b)) - half;
    return select(caps, band, t <= sweep);
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

// A four-point sparkle: two elongated diamonds crossed, so the arms taper to
// points and the waist pinches in. The constant pulls the union back toward
// unit gradient, which is all an antialiasing ramp needs of it.
fn sd_sparkle(p: vec2<f32>, r: f32) -> f32 {
    let a = abs(p.x) * 3.4 + abs(p.y) - r;
    let b = abs(p.x) + abs(p.y) * 3.4 - r;
    return min(a, b) * 0.29;
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
