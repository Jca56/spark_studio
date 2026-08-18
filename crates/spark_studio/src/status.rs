//! The status strip along the bottom of the window.
//!
//! It closes the layout — an edge needs something on the other side of it
//! to read as an edge — and it is where the editor says what it just did,
//! somewhere Alva can actually see it. Today that's what's selected and
//! where the playhead is; the action log currently going to the terminal
//! belongs here next.
//!
//! Formatting is kept apart from drawing so the musician-facing numbers
//! (bars count from one, beats count from one) can be tested without a
//! window.

use spark_audio::BeatGrid;
use spark_render::Viewport;
use spark_text::Text;
use spark_ui::theme;

/// Strip text size in logical px. Sized to fill the strip rather than to
/// sit politely inside it — the bar is short, so the text has to be most
/// of its height to stay readable across a room.
pub const TEXT: f32 = 18.0;

/// What the strip reports this frame.
pub struct Status {
    /// Left: what the editor is currently acting on.
    pub left: String,
    /// Right: where the playhead is.
    pub right: String,
}

/// The playhead as a musician reads it: `Bar 5.3 · 0:08.42`.
///
/// Bars and beats are 1-based, matching the ruler's own numbering — bar
/// zero beat zero would be a programmer's answer to a musician's question.
/// Time before the first bar line clamps to the start rather than counting
/// backwards into negative bars.
pub fn playhead(t: f32, beat: &BeatGrid) -> String {
    let bar_s = 4.0 * 60.0 / beat.bpm.max(1.0);
    let beat_s = bar_s * 0.25;
    let into = (t - beat.first_bar).max(0.0);
    let bar = (into / bar_s).floor();
    let sub = (into - bar * bar_s) / beat_s;
    let mins = (t.max(0.0) / 60.0).floor();
    let secs = t.max(0.0) - mins * 60.0;
    format!(
        "Bar {}.{} · {}:{:05.2}",
        bar as i64 + 1,
        sub.floor() as i64 + 1,
        mins as i64,
        secs
    )
}

/// What the editor is acting on, for the left half.
pub fn selection(names: &[String]) -> String {
    match names {
        [] => "no selection".to_string(),
        [one] => one.clone(),
        many => format!("{} layers selected", many.len()),
    }
}

pub fn labels(text: &mut Text, area: Viewport, scale: f32, s: &Status, res: (u32, u32)) {
    let size = TEXT * scale;
    let pad = 18.0 * scale;
    let y = area.y + (area.h - Text::line_height(size)) * 0.5;
    let th = theme();
    text.label(
        &s.left,
        size,
        area.x + pad,
        y,
        th.text_dim,
        (area.w * 0.6).max(1.0),
        res,
    );
    let w = text.measure(&s.right, size);
    text.label(
        &s.right,
        size,
        area.x + area.w - pad - w,
        y,
        th.text_dim,
        w + 2.0,
        res,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(bpm: f32) -> BeatGrid {
        BeatGrid {
            bpm,
            first_bar: 0.0,
        }
    }

    /// Bars and beats count from one, like the ruler does and like a
    /// musician does. At 120 BPM a bar is 2s and a beat is 0.5s.
    #[test]
    fn the_playhead_reads_in_bars_and_beats() {
        assert!(playhead(0.0, &grid(120.0)).starts_with("Bar 1.1"));
        assert!(playhead(0.5, &grid(120.0)).starts_with("Bar 1.2"));
        assert!(playhead(1.5, &grid(120.0)).starts_with("Bar 1.4"));
        assert!(playhead(2.0, &grid(120.0)).starts_with("Bar 2.1"));
        assert!(playhead(8.0, &grid(120.0)).starts_with("Bar 5.1"));
    }

    /// The grid's phase is respected: a track whose first bar lands late
    /// still calls that moment bar one.
    #[test]
    fn the_first_bar_is_bar_one_wherever_it_falls() {
        let g = BeatGrid {
            bpm: 140.0,
            first_bar: 0.37,
        };
        assert!(playhead(0.37, &g).starts_with("Bar 1.1"));
        // ...and anything before it clamps there rather than counting
        // backwards into bar zero.
        assert!(playhead(0.0, &g).starts_with("Bar 1.1"));
    }

    #[test]
    fn the_clock_reads_minutes_and_seconds() {
        assert!(playhead(0.0, &grid(120.0)).ends_with("0:00.00"));
        assert!(playhead(8.42, &grid(120.0)).ends_with("0:08.42"));
        assert!(playhead(75.5, &grid(120.0)).ends_with("1:15.50"));
    }

    #[test]
    fn the_selection_reads_plainly() {
        assert_eq!(selection(&[]), "no selection");
        assert_eq!(selection(&["Circle 2".to_string()]), "Circle 2");
        assert_eq!(
            selection(&["a".to_string(), "b".to_string()]),
            "2 layers selected"
        );
    }
}
