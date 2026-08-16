//! Timeline panel: a slim waveform strip with transport up top — a ruler,
//! not the main event. The space below is reserved for arranging comps.

use spark_render::Viewport;
use spark_ui::{ICON_PAUSE, ICON_PLAY, UiRect, theme};

/// Waveform strip height in logical px.
const WAVE_H: f32 = 110.0;

/// Solved strip geometry: the play/pause button and the waveform area.
pub struct Strip {
    pub button: Viewport,
    pub wave: Viewport,
}

pub fn strip(tl: Viewport, scale: f32) -> Strip {
    let pad = 12.0 * scale;
    let h = (WAVE_H * scale).min(tl.h - pad * 2.0);
    let btn = (64.0 * scale).min(h);
    let button = Viewport {
        x: tl.x + pad,
        y: tl.y + pad + (h - btn) * 0.5,
        w: btn,
        h: btn,
    };
    Strip {
        button,
        wave: Viewport {
            x: button.x + btn + pad,
            y: tl.y + pad,
            w: (tl.w - pad * 3.0 - btn).max(1.0),
            h,
        },
    }
}

/// One vertical min/max bar per ~2 logical px, aggregated from the track's
/// peak buckets so the whole song always spans the strip.
pub fn waveform_rects(strip: &Strip, scale: f32, peaks: &[[f32; 2]]) -> Vec<UiRect> {
    if peaks.is_empty() {
        return Vec::new();
    }
    let t = theme();
    let wave = strip.wave;
    let mid = wave.y + wave.h * 0.5;
    let half_h = wave.h * 0.5;
    let step = 2.0 * scale;
    let cols = (wave.w / step).max(1.0) as usize;
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
                x: wave.x + col as f32 * step,
                y: mid - hi * half_h,
                w: (step * 0.75).max(1.0),
                h: ((hi - lo) * half_h).max(1.0),
            },
            t.wave,
        ));
    }
    out
}

pub fn transport_rects(strip: &Strip, scale: f32, playing: bool, hover: bool) -> Vec<UiRect> {
    let t = theme();
    let bg = if playing {
        t.accent_bg
    } else if hover {
        t.button_hover
    } else {
        t.card
    };
    vec![
        UiRect::region_rounded(strip.button, bg, 12.0 * scale),
        UiRect::icon_sized(
            strip.button,
            if playing { ICON_PAUSE } else { ICON_PLAY },
            2.0 * scale,
            if playing { t.accent } else { t.icon_hover },
            0.30,
        ),
    ]
}

/// Bar lines over the waveform, one per 4 beats; phrase lines (every 4
/// bars) draw brighter. White with low alpha so they read over the teal.
pub fn grid_rects(
    strip: &Strip,
    scale: f32,
    beat: &spark_audio::BeatGrid,
    duration: f32,
) -> Vec<UiRect> {
    let wave = strip.wave;
    let bar_s = 4.0 * 60.0 / beat.bpm.max(1.0);
    let mut out = Vec::new();
    let mut k = 0usize;
    loop {
        let time = beat.first_bar + k as f32 * bar_s;
        if time >= duration || duration <= 0.0 {
            break;
        }
        let phrase = k % 4 == 0;
        out.push(UiRect::region(
            Viewport {
                x: wave.x + time / duration * wave.w,
                y: wave.y,
                w: if phrase { 2.0 * scale } else { 1.0 * scale },
                h: wave.h,
            },
            if phrase {
                [1.0, 1.0, 1.0, 0.32]
            } else {
                [1.0, 1.0, 1.0, 0.13]
            },
        ));
        k += 1;
    }
    out
}

/// The gold line at normalized position `t01` across the waveform.
pub fn playhead_rect(strip: &Strip, scale: f32, t01: f32) -> UiRect {
    let t = theme();
    let x = strip.wave.x + strip.wave.w * t01.clamp(0.0, 1.0);
    UiRect::region(
        Viewport {
            x: x - 1.5 * scale,
            y: strip.wave.y - 6.0 * scale,
            w: 3.0 * scale,
            h: strip.wave.h + 12.0 * scale,
        },
        t.playhead,
    )
}

/// Normalized position of a click inside the waveform strip, if it hit.
pub fn seek_t(strip: &Strip, px: f32, py: f32) -> Option<f32> {
    strip
        .wave
        .contains(px, py)
        .then(|| ((px - strip.wave.x) / strip.wave.w).clamp(0.0, 1.0))
}
