//! CPU-side signed distance functions, mirroring shaders/shape.wgsl, for
//! hit-testing and geometry queries.

pub fn sd_box(p: [f32; 2], half: [f32; 2]) -> f32 {
    let d = [p[0].abs() - half[0], p[1].abs() - half[1]];
    let outside = (d[0].max(0.0).powi(2) + d[1].max(0.0).powi(2)).sqrt();
    outside + d[0].max(d[1]).min(0.0)
}

pub fn sd_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let pa = [p[0] - a[0], p[1] - a[1]];
    let ba = [b[0] - a[0], b[1] - a[1]];
    let dot_bb = (ba[0] * ba[0] + ba[1] * ba[1]).max(0.0001);
    let h = ((pa[0] * ba[0] + pa[1] * ba[1]) / dot_bb).clamp(0.0, 1.0);
    let d = [pa[0] - ba[0] * h, pa[1] - ba[1] * h];
    (d[0] * d[0] + d[1] * d[1]).sqrt()
}

pub fn sd_ngon(p: [f32; 2], radius: f32, sides: f32) -> f32 {
    let an = std::f32::consts::PI / sides;
    let (an_sin, an_cos) = an.sin_cos();
    let m = 2.0 * an;
    let mut ang = p[0].atan2(p[1]);
    ang -= m * (ang / m).floor();
    let bn = ang - an;
    let len = (p[0] * p[0] + p[1] * p[1]).sqrt();
    let mut q = [
        len * bn.cos() - radius * an_cos,
        len * bn.sin().abs() - radius * an_sin,
    ];
    q[1] += (-q[1]).clamp(0.0, radius * an_sin);
    (q[0] * q[0] + q[1] * q[1]).sqrt() * q[0].signum()
}
