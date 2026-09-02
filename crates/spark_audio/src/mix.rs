//! The mix: every audio clip on the timeline summed into one stereo
//! stream, by timeline frame. Pure, so playback and export hear the
//! same thing — the device callback fills its buffer through [`mix`],
//! the export renders the whole range through [`render`] — and a test
//! can listen without a device.

use std::sync::Arc;

/// One audio clip as the mixer hears it: which samples, where on the
/// timeline it starts, how far into the file its left edge sits, how
/// long it plays, and how loud.
#[derive(Clone)]
pub struct Voice {
    /// Interleaved stereo at [`crate::SAMPLE_RATE`].
    pub samples: Arc<Vec<f32>>,
    /// The timeline frame the clip starts on.
    pub at: usize,
    /// Frames into `samples` the clip's left edge sits — a left-trim.
    pub offset: usize,
    /// Frames the clip plays for.
    pub len: usize,
    /// Linear gain; 1 is the file as it is.
    pub gain: f32,
}

impl Voice {
    /// The frame after the clip's last.
    pub fn end(&self) -> usize {
        self.at.saturating_add(self.len)
    }
}

/// Fill `out` — interleaved stereo frames starting at timeline frame
/// `pos` — with every voice that covers it, summed. Silence where none
/// does. The sum is clamped to full scale, which is what the converter
/// would do to it anyway; the export's encoder gets the same picture.
pub fn mix(out: &mut [f32], pos: usize, voices: &[Voice]) {
    out.fill(0.0);
    let n = out.len() / 2;
    let end = pos.saturating_add(n);
    for v in voices {
        let a = pos.max(v.at);
        let b = end.min(v.end());
        if b <= a {
            continue;
        }
        let total = v.samples.len() / 2;
        for f in a..b {
            let src = v.offset + (f - v.at);
            if src >= total {
                // The clip runs past its file: silence from here.
                break;
            }
            let o = (f - pos) * 2;
            out[o] += v.samples[src * 2] * v.gain;
            out[o + 1] += v.samples[src * 2 + 1] * v.gain;
        }
    }
    for s in out.iter_mut() {
        *s = s.clamp(-1.0, 1.0);
    }
}

/// `frames` stereo frames of the mix from timeline frame `from`, offline.
pub fn render(voices: &[Voice], from: usize, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; frames * 2];
    mix(&mut out, from, voices);
    out
}

/// The frame after the last voice ends — how long the mix is.
pub fn last_frame(voices: &[Voice]) -> usize {
    voices.iter().map(Voice::end).max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of rising numbers, so a sample says which frame it is.
    fn ramp(frames: usize) -> Arc<Vec<f32>> {
        Arc::new(
            (0..frames)
                .flat_map(|f| [f as f32 * 0.01, -(f as f32) * 0.01])
                .collect(),
        )
    }

    /// A clip placed later on the timeline plays its file from its own
    /// start: silence before it, the file from its first frame at the
    /// clip's first frame, silence after its length runs out.
    #[test]
    fn a_placed_clip_plays_from_where_it_sits() {
        let v = Voice {
            samples: ramp(100),
            at: 10,
            offset: 0,
            len: 5,
            gain: 1.0,
        };
        let out = render(&[v], 8, 10);
        // Frames 8, 9: nothing yet.
        assert_eq!(&out[..4], &[0.0; 4]);
        // Frame 10 is the file's frame 0, 11 its frame 1...
        assert!((out[4] - 0.0).abs() < 1e-6 && (out[6] - 0.01).abs() < 1e-6);
        assert!((out[7] - -0.01).abs() < 1e-6, "right channel rides along");
        assert!((out[12] - 0.04).abs() < 1e-6, "frame 14 is the file's 4");
        // Frames 15..18: the clip is over.
        assert_eq!(&out[14..], &[0.0; 6]);
    }

    /// A left-trim eats the file's head: offset frames are skipped, and
    /// the gain scales what plays.
    #[test]
    fn a_trimmed_clip_skips_its_offset_and_wears_its_gain() {
        let v = Voice {
            samples: ramp(100),
            at: 0,
            offset: 20,
            len: 3,
            gain: 0.5,
        };
        let out = render(&[v], 0, 3);
        assert!((out[0] - 0.10).abs() < 1e-6, "file frame 20 at half");
        assert!((out[4] - 0.11).abs() < 1e-6);
    }

    /// Two voices sum; the sum is clamped to full scale; a clip that
    /// runs past its file goes quiet rather than reading off the end.
    #[test]
    fn voices_sum_clamp_and_run_out() {
        // Eight frames of 0.8 (sixteen floats: stereo).
        let loud = Arc::new(vec![0.8f32; 16]);
        let a = Voice {
            samples: loud.clone(),
            at: 0,
            offset: 0,
            len: 4,
            gain: 1.0,
        };
        let b = Voice {
            samples: loud,
            at: 2,
            offset: 2,
            len: 10,
            gain: 1.0,
        };
        let out = render(&[a, b], 0, 10);
        assert!((out[0] - 0.8).abs() < 1e-6, "only a at frame 0");
        assert_eq!(out[4], 1.0, "a + b at frame 2 clamps to full scale");
        assert!((out[8] - 0.8).abs() < 1e-6, "only b at frame 4 (file frame 4)");
        assert!((out[14] - 0.8).abs() < 1e-6, "b's last file frame at frame 7");
        assert_eq!(out[16], 0.0, "b ran out of file at frame 8");
        assert_eq!(last_frame(&[]), 0);
    }

    /// Mixing straight into a buffer at a position works the same as a
    /// render — the device path and the export path are one function.
    #[test]
    fn mix_and_render_agree() {
        let v = Voice {
            samples: ramp(50),
            at: 3,
            offset: 1,
            len: 40,
            gain: 1.0,
        };
        let mut buf = vec![9.0f32; 16];
        mix(&mut buf, 5, std::slice::from_ref(&v));
        assert_eq!(buf, render(&[v], 5, 8));
    }
}
