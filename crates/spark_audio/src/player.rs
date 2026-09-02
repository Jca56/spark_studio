//! Playback: a cpal output stream fed straight from the baked track. The
//! transport clock *is* the audio callback's cursor, so what you hear and
//! what the playhead shows can never drift.
//!
//! Paused, the stream keeps running and plays a **whisper** — noise at
//! [`WHISPER`], far under anything a DAC can resolve — rather than
//! digital zero. Wireless headsets (Alva's G522) and some DACs power
//! their link down after a few seconds of exact silence and take most
//! of a second to wake, which is what "press play and the audio starts
//! a second late — unless I just stopped it" was (2026-09-01). Real
//! silence is the one thing that must never reach them.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::SAMPLE_RATE;

/// The paused whisper's peak amplitude: about -84 dBFS, a couple of
/// 16-bit steps — inaudible, but never a run of exact zeros.
pub const WHISPER: f32 = 6.0e-5;

struct Shared {
    /// Interleaved stereo at [`SAMPLE_RATE`].
    samples: Arc<Vec<f32>>,
    /// Playback cursor in stereo frames.
    frame: AtomicUsize,
    playing: AtomicBool,
    /// Loop region in frames; `end == 0` means no loop. The callback wraps
    /// sample-accurately, so loops stay musically tight.
    loop_start: AtomicUsize,
    loop_end: AtomicUsize,
}

pub struct Player {
    _stream: cpal::Stream,
    shared: Arc<Shared>,
}

impl Player {
    pub fn new(samples: Arc<Vec<f32>>) -> Result<Player, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no audio output device")?;
        let config = cpal::StreamConfig {
            channels: 2,
            sample_rate: cpal::SampleRate(SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Default,
        };
        let shared = Arc::new(Shared {
            samples,
            frame: AtomicUsize::new(0),
            playing: AtomicBool::new(false),
            loop_start: AtomicUsize::new(0),
            loop_end: AtomicUsize::new(0),
        });
        let cb = shared.clone();
        // The whisper's generator lives with the callback: one xorshift
        // state, no allocation, no locking.
        let mut hiss: u32 = 0x9E37_79B9;
        let stream = device
            .build_output_stream(
                &config,
                move |out: &mut [f32], _| {
                    if !cb.playing.load(Ordering::Relaxed) {
                        for v in out.iter_mut() {
                            hiss ^= hiss << 13;
                            hiss ^= hiss >> 17;
                            hiss ^= hiss << 5;
                            *v = ((hiss >> 8) as f32 / 8_388_608.0 - 1.0) * WHISPER;
                        }
                        return;
                    }
                    let total = cb.samples.len() / 2;
                    let l0 = cb.loop_start.load(Ordering::Relaxed);
                    let l1 = cb.loop_end.load(Ordering::Relaxed).min(total);
                    let looping = l1 > l0;
                    let mut pos = cb.frame.load(Ordering::Relaxed);
                    let mut filled = 0usize;
                    let out_frames = out.len() / 2;
                    while filled < out_frames {
                        // Arriving at the loop end (fills clamp exactly to
                        // it) wraps to the start, even mid-buffer, so loops
                        // stay sample-tight. A cursor seeked past the region
                        // plays on normally.
                        if looping && pos == l1 {
                            pos = l0;
                        }
                        let limit = if looping && pos < l1 { l1 } else { total };
                        let n = (out_frames - filled).min(limit.saturating_sub(pos));
                        if n == 0 {
                            // Track over — stop, leave the cursor at the end.
                            out[filled * 2..].fill(0.0);
                            cb.playing.store(false, Ordering::Relaxed);
                            break;
                        }
                        out[filled * 2..(filled + n) * 2]
                            .copy_from_slice(&cb.samples[pos * 2..(pos + n) * 2]);
                        pos += n;
                        filled += n;
                    }
                    cb.frame.store(pos, Ordering::Relaxed);
                },
                |e| eprintln!("audio stream error: {e}"),
                None,
            )
            .map_err(|e| format!("audio stream: {e}"))?;
        stream.play().map_err(|e| format!("audio start: {e}"))?;
        Ok(Player {
            _stream: stream,
            shared,
        })
    }

    /// Toggle play/pause; returns whether we're playing now. Toggling at the
    /// end of the track restarts from the top.
    pub fn toggle(&self) -> bool {
        let s = &self.shared;
        let now = !s.playing.load(Ordering::Relaxed);
        if now && s.frame.load(Ordering::Relaxed) * 2 >= s.samples.len() {
            s.frame.store(0, Ordering::Relaxed);
        }
        s.playing.store(now, Ordering::Relaxed);
        now
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    /// Current position in seconds, straight off the audio cursor.
    pub fn time(&self) -> f32 {
        self.shared.frame.load(Ordering::Relaxed) as f32 / SAMPLE_RATE as f32
    }

    pub fn seek(&self, t: f32) {
        let max = self.shared.samples.len() / 2;
        let frame = ((t.max(0.0) * SAMPLE_RATE as f32) as usize).min(max);
        self.shared.frame.store(frame, Ordering::Relaxed);
    }

    /// Loop playback between `start` and `end` seconds (sample-accurate).
    pub fn set_loop(&self, start: f32, end: f32) {
        let max = self.shared.samples.len() / 2;
        let a = ((start.max(0.0) * SAMPLE_RATE as f32) as usize).min(max);
        let b = ((end.max(0.0) * SAMPLE_RATE as f32) as usize).min(max);
        self.shared.loop_start.store(a.min(b), Ordering::Relaxed);
        self.shared.loop_end.store(a.max(b), Ordering::Relaxed);
    }

    pub fn clear_loop(&self) {
        self.shared.loop_end.store(0, Ordering::Relaxed);
    }
}
