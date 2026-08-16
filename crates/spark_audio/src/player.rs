//! Playback: a cpal output stream fed straight from the baked track. The
//! transport clock *is* the audio callback's cursor, so what you hear and
//! what the playhead shows can never drift.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::SAMPLE_RATE;

struct Shared {
    /// Interleaved stereo at [`SAMPLE_RATE`].
    samples: Arc<Vec<f32>>,
    /// Playback cursor in stereo frames.
    frame: AtomicUsize,
    playing: AtomicBool,
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
        });
        let cb = shared.clone();
        let stream = device
            .build_output_stream(
                &config,
                move |out: &mut [f32], _| {
                    if !cb.playing.load(Ordering::Relaxed) {
                        out.fill(0.0);
                        return;
                    }
                    let start = cb.frame.load(Ordering::Relaxed) * 2;
                    let n = out.len().min(cb.samples.len().saturating_sub(start));
                    out[..n].copy_from_slice(&cb.samples[start..start + n]);
                    out[n..].fill(0.0);
                    cb.frame.fetch_add(n / 2, Ordering::Relaxed);
                    if n < out.len() {
                        // Track over — stop, leave the cursor at the end.
                        cb.playing.store(false, Ordering::Relaxed);
                    }
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
}
