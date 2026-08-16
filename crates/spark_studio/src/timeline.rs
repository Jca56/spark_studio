//! Timeline panel drawing: the baked waveform as instanced bars. Transport,
//! playhead, and scrubbing arrive with playback.

use spark_render::Viewport;
use spark_ui::{UiRect, theme};

/// One vertical min/max bar per ~2 logical px, aggregated from the track's
/// peak buckets so the whole song always fits the panel width.
pub fn waveform_rects(tl: Viewport, scale: f32, peaks: &[[f32; 2]]) -> Vec<UiRect> {
    if peaks.is_empty() {
        return Vec::new();
    }
    let t = theme();
    let pad = 14.0 * scale;
    let mid = tl.y + tl.h * 0.5;
    let half_h = (tl.h * 0.5 - pad).max(1.0);
    let step = 2.0 * scale;
    let cols = ((tl.w - pad * 2.0) / step).max(1.0) as usize;
    let mut out = Vec::with_capacity(cols);
    for col in 0..cols {
        let a = col * peaks.len() / cols;
        let b = ((col + 1) * peaks.len() / cols).max(a + 1).min(peaks.len());
        let mut lo = 0.0f32;
        let mut hi = 0.0f32;
        for p in &peaks[a..b] {
            lo = lo.min(p[0]);
            hi = hi.max(p[1]);
        }
        out.push(UiRect::region(
            Viewport {
                x: tl.x + pad + col as f32 * step,
                y: mid - hi * half_h,
                w: (step * 0.75).max(1.0),
                h: ((hi - lo) * half_h).max(1.0),
            },
            t.grad_purple,
        ));
    }
    out
}
