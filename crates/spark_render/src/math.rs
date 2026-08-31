//! spark_render's own linear algebra: exactly what a scene needs and nothing
//! it doesn't. A `Vec3`, and a column-major `Mat4` laid out the way WGSL
//! reads a `mat4x4<f32>`, so a matrix goes into a buffer with a cast and no
//! transposing in between.
//!
//! Conventions, stated once: matrices multiply column vectors on the right
//! (`m * p`), so `a * b` applies `b` first. The canvas frame is x right,
//! y down, z *toward* the camera — the same frame every 2D shape has
//! always lived in, with depth added, so a comp that never leaves the
//! canvas plane is drawn by exactly the arithmetic it was before.

use std::ops::{Add, Mul, Neg, Sub};

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    /// Unit length, or zero if it had no length to begin with.
    pub fn normalized(self) -> Self {
        let l = self.length();
        if l > 1e-12 { self * (1.0 / l) } else { Self::ZERO }
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}

impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

/// A 4x4 matrix, column-major: element (row `r`, column `c`) is `0[c * 4 + r]`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Mat4(pub [f32; 16]);

impl Mat4 {
    pub const IDENTITY: Self = Self([
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]);

    /// Build from rows, which is how a matrix is written on paper.
    pub const fn from_rows(r: [[f32; 4]; 4]) -> Self {
        Self([
            r[0][0], r[1][0], r[2][0], r[3][0], //
            r[0][1], r[1][1], r[2][1], r[3][1], //
            r[0][2], r[1][2], r[2][2], r[3][2], //
            r[0][3], r[1][3], r[2][3], r[3][3],
        ])
    }

    pub fn translation(v: Vec3) -> Self {
        Self::from_rows([
            [1.0, 0.0, 0.0, v.x],
            [0.0, 1.0, 0.0, v.y],
            [0.0, 0.0, 1.0, v.z],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    pub fn scaling(v: Vec3) -> Self {
        Self::from_rows([
            [v.x, 0.0, 0.0, 0.0],
            [0.0, v.y, 0.0, 0.0],
            [0.0, 0.0, v.z, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// Rotation about x by `rad`: y toward z.
    pub fn rotation_x(rad: f32) -> Self {
        let (s, c) = rad.sin_cos();
        Self::from_rows([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, c, -s, 0.0],
            [0.0, s, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// Rotation about y by `rad`: z toward x.
    pub fn rotation_y(rad: f32) -> Self {
        let (s, c) = rad.sin_cos();
        Self::from_rows([
            [c, 0.0, s, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [-s, 0.0, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// Rotation about z by `rad`: x toward y — clockwise on a y-down
    /// canvas, which is the direction every shape's own rotation already
    /// turns.
    pub fn rotation_z(rad: f32) -> Self {
        let (s, c) = rad.sin_cos();
        Self::from_rows([
            [c, -s, 0.0, 0.0],
            [s, c, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// Rotation from a quaternion `[x, y, z, w]` — glTF's order. Any
    /// length; it is normalised here.
    pub fn from_quat(q: [f32; 4]) -> Self {
        let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        let [x, y, z, w] = if len > 1e-12 { q.map(|v| v / len) } else { [0.0, 0.0, 0.0, 1.0] };
        let (xx, yy, zz) = (x * x, y * y, z * z);
        let (xy, xz, yz) = (x * y, x * z, y * z);
        let (wx, wy, wz) = (w * x, w * y, w * z);
        Self::from_rows([
            [1.0 - 2.0 * (yy + zz), 2.0 * (xy - wz), 2.0 * (xz + wy), 0.0],
            [2.0 * (xy + wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz - wx), 0.0],
            [2.0 * (xz - wy), 2.0 * (yz + wx), 1.0 - 2.0 * (xx + yy), 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// Rotate about `pivot` rather than the origin.
    pub fn about(pivot: Vec3, rotation: Self) -> Self {
        Self::translation(pivot) * rotation * Self::translation(-pivot)
    }

    pub fn at(&self, row: usize, col: usize) -> f32 {
        self.0[col * 4 + row]
    }

    /// Apply to a point, dividing by w — a projected point comes back in
    /// normalized device coordinates.
    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        let [x, y, z, w] = self.transform4([p.x, p.y, p.z, 1.0]);
        let inv = if w.abs() > 1e-12 { 1.0 / w } else { 1.0 };
        Vec3::new(x * inv, y * inv, z * inv)
    }

    /// Apply to a direction: rotation and scale, no translation.
    pub fn transform_vec(&self, v: Vec3) -> Vec3 {
        let [x, y, z, _] = self.transform4([v.x, v.y, v.z, 0.0]);
        Vec3::new(x, y, z)
    }

    pub fn transform4(&self, v: [f32; 4]) -> [f32; 4] {
        let m = &self.0;
        std::array::from_fn(|r| {
            m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2] + m[12 + r] * v[3]
        })
    }

    pub fn transpose(&self) -> Self {
        let m = &self.0;
        Self(std::array::from_fn(|i| m[(i % 4) * 4 + i / 4]))
    }

    /// The inverse, or `None` for a singular matrix. Cofactor expansion —
    /// the layout doesn't matter to it, since the inverse of a transpose
    /// is the transpose of the inverse.
    pub fn inverse(&self) -> Option<Self> {
        let m = &self.0;
        let mut inv = [0.0f32; 16];
        inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
            + m[9] * m[7] * m[14]
            + m[13] * m[6] * m[11]
            - m[13] * m[7] * m[10];
        inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
            - m[8] * m[7] * m[14]
            - m[12] * m[6] * m[11]
            + m[12] * m[7] * m[10];
        inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
            + m[8] * m[7] * m[13]
            + m[12] * m[5] * m[11]
            - m[12] * m[7] * m[9];
        inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
            - m[8] * m[6] * m[13]
            - m[12] * m[5] * m[10]
            + m[12] * m[6] * m[9];
        inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
            - m[9] * m[3] * m[14]
            - m[13] * m[2] * m[11]
            + m[13] * m[3] * m[10];
        inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
            + m[8] * m[3] * m[14]
            + m[12] * m[2] * m[11]
            - m[12] * m[3] * m[10];
        inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
            - m[8] * m[3] * m[13]
            - m[12] * m[1] * m[11]
            + m[12] * m[3] * m[9];
        inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
            + m[8] * m[2] * m[13]
            + m[12] * m[1] * m[10]
            - m[12] * m[2] * m[9];
        inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
            + m[5] * m[3] * m[14]
            + m[13] * m[2] * m[7]
            - m[13] * m[3] * m[6];
        inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
            - m[4] * m[3] * m[14]
            - m[12] * m[2] * m[7]
            + m[12] * m[3] * m[6];
        inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
            + m[4] * m[3] * m[13]
            + m[12] * m[1] * m[7]
            - m[12] * m[3] * m[5];
        inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
            - m[4] * m[2] * m[13]
            - m[12] * m[1] * m[6]
            + m[12] * m[2] * m[5];
        inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
            - m[5] * m[3] * m[10]
            - m[9] * m[2] * m[7]
            + m[9] * m[3] * m[6];
        inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
            + m[4] * m[3] * m[10]
            + m[8] * m[2] * m[7]
            - m[8] * m[3] * m[6];
        inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
            - m[4] * m[3] * m[9]
            - m[8] * m[1] * m[7]
            + m[8] * m[3] * m[5];
        inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
            + m[4] * m[2] * m[9]
            + m[8] * m[1] * m[6]
            - m[8] * m[2] * m[5];
        let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
        if det.abs() < 1e-12 {
            return None;
        }
        let d = 1.0 / det;
        Some(Self(inv.map(|v| v * d)))
    }
}

impl Mul for Mat4 {
    type Output = Self;
    /// `self * rhs`: `rhs` is applied first.
    fn mul(self, rhs: Self) -> Self {
        let (a, b) = (&self.0, &rhs.0);
        Self(std::array::from_fn(|i| {
            let (c, r) = (i / 4, i % 4);
            a[r] * b[c * 4] + a[4 + r] * b[c * 4 + 1] + a[8 + r] * b[c * 4 + 2] + a[12 + r] * b[c * 4 + 3]
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn identity_leaves_a_point_alone() {
        let p = Vec3::new(3.0, -2.0, 7.5);
        assert!(close(Mat4::IDENTITY.transform_point(p), p));
    }

    #[test]
    fn translation_moves_points_not_directions() {
        let t = Mat4::translation(Vec3::new(10.0, 20.0, 30.0));
        assert!(close(t.transform_point(Vec3::new(1.0, 1.0, 1.0)), Vec3::new(11.0, 21.0, 31.0)));
        assert!(close(t.transform_vec(Vec3::new(1.0, 1.0, 1.0)), Vec3::new(1.0, 1.0, 1.0)));
    }

    #[test]
    fn a_quarter_turn_about_z_sends_x_to_y() {
        let r = Mat4::rotation_z(FRAC_PI_2);
        assert!(close(r.transform_vec(Vec3::new(1.0, 0.0, 0.0)), Vec3::new(0.0, 1.0, 0.0)));
    }

    #[test]
    fn a_quarter_turn_about_y_sends_z_to_x() {
        let r = Mat4::rotation_y(FRAC_PI_2);
        assert!(close(r.transform_vec(Vec3::new(0.0, 0.0, 1.0)), Vec3::new(1.0, 0.0, 0.0)));
    }

    #[test]
    fn a_quarter_turn_about_x_sends_y_to_z() {
        let r = Mat4::rotation_x(FRAC_PI_2);
        assert!(close(r.transform_vec(Vec3::new(0.0, 1.0, 0.0)), Vec3::new(0.0, 0.0, 1.0)));
    }

    #[test]
    fn a_quaternion_is_the_same_rotation() {
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let q = Mat4::from_quat([0.0, 0.0, s, s]);
        let r = Mat4::rotation_z(FRAC_PI_2);
        for (a, b) in q.0.iter().zip(r.0.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
        // Unnormalised input is normalised; a zero quaternion is identity.
        assert!(close(
            Mat4::from_quat([0.0, 0.0, 3.0, 3.0]).transform_vec(Vec3::new(1.0, 0.0, 0.0)),
            Vec3::new(0.0, 1.0, 0.0)
        ));
        assert_eq!(Mat4::from_quat([0.0; 4]), Mat4::IDENTITY);
    }

    #[test]
    fn multiplication_applies_the_right_hand_side_first() {
        let t = Mat4::translation(Vec3::new(5.0, 0.0, 0.0));
        let r = Mat4::rotation_z(FRAC_PI_2);
        // Rotate first, then translate: x-hat -> y-hat -> (5, 1, 0).
        let p = (t * r).transform_point(Vec3::new(1.0, 0.0, 0.0));
        assert!(close(p, Vec3::new(5.0, 1.0, 0.0)));
        // The other order translates first: (6, 0, 0) -> (0, 6, 0).
        let q = (r * t).transform_point(Vec3::new(1.0, 0.0, 0.0));
        assert!(close(q, Vec3::new(0.0, 6.0, 0.0)));
    }

    #[test]
    fn about_a_pivot_leaves_the_pivot_where_it_is() {
        let pivot = Vec3::new(100.0, 50.0, 0.0);
        let m = Mat4::about(pivot, Mat4::rotation_y(1.2));
        assert!(close(m.transform_point(pivot), pivot));
        // A point one unit right of the pivot swings toward the camera.
        let p = m.transform_point(pivot + Vec3::new(1.0, 0.0, 0.0));
        assert!((p.x - pivot.x - 1.2f32.cos()).abs() < 1e-4);
        assert!((p.z + 1.2f32.sin()).abs() < 1e-4);
    }

    #[test]
    fn the_inverse_undoes_the_matrix() {
        let m = Mat4::translation(Vec3::new(3.0, -4.0, 9.0))
            * Mat4::rotation_y(0.7)
            * Mat4::rotation_x(-0.3)
            * Mat4::scaling(Vec3::new(2.0, 2.0, 0.5));
        let inv = m.inverse().expect("invertible");
        let p = Vec3::new(1.5, -2.5, 4.0);
        assert!(close(inv.transform_point(m.transform_point(p)), p));
        let id = m * inv;
        for (i, v) in id.0.iter().enumerate() {
            let want = if i % 5 == 0 { 1.0 } else { 0.0 };
            assert!((v - want).abs() < 1e-4, "element {i}: {v}");
        }
    }

    #[test]
    fn a_flat_matrix_has_no_inverse() {
        assert!(Mat4::scaling(Vec3::new(1.0, 0.0, 1.0)).inverse().is_none());
    }

    #[test]
    fn transpose_swaps_rows_and_columns() {
        let m = Mat4::from_rows([
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [9.0, 10.0, 11.0, 12.0],
            [13.0, 14.0, 15.0, 16.0],
        ]);
        assert_eq!(m.at(0, 1), 2.0);
        assert_eq!(m.transpose().at(1, 0), 2.0);
        assert_eq!(m.transpose().transpose(), m);
    }
}
