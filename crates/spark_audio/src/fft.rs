//! Our own FFT: iterative radix-2 Cooley–Tukey, split re/im arrays.
//! Small, allocation-free, and plenty fast for offline analysis.

use std::f32::consts::PI;

/// In-place FFT. `re.len()` must equal `im.len()` and be a power of two.
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two() && im.len() == n);

    // Bit-reversal permutation.
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    // Butterfly stages.
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (w_re, w_im) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let mut c_re = 1.0f32;
            let mut c_im = 0.0f32;
            let half = len / 2;
            for k in start..start + half {
                let (e_re, e_im) = (re[k], im[k]);
                let (o_re, o_im) = (re[k + half], im[k + half]);
                let t_re = o_re * c_re - o_im * c_im;
                let t_im = o_re * c_im + o_im * c_re;
                re[k] = e_re + t_re;
                im[k] = e_im + t_im;
                re[k + half] = e_re - t_re;
                im[k + half] = e_im - t_im;
                let n_re = c_re * w_re - c_im * w_im;
                c_im = c_re * w_im + c_im * w_re;
                c_re = n_re;
            }
        }
        len <<= 1;
    }
}
