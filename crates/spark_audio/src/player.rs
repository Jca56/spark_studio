//! Playback: a cpal output stream fed straight from the baked track. The
//! transport clock *is* the audio callback's cursor, so what you hear and
//! what the playhead shows can never drift.
//!
//! The device buffer is asked for at [`BUFFER_FRAMES`] — two PipeWire
//! quanta, 43 ms — rather than cpal's 100 ms default: what is queued
//! ahead of the ear is what a press has to wait through. The stream
//! also **times every play**: the callback prints how long the press
//! took to reach it and how long the callback had been asleep before,
//! so "it starts a second late" is a number in the log, not a guess
//! (2026-09-01: the first guess — the headset — was wrong).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::SAMPLE_RATE;

/// The device buffer asked for, in frames: two PipeWire quanta at 48 k.
/// cpal's default is 100 ms, all of it queued ahead of the ear.
pub const BUFFER_FRAMES: u32 = 2048;

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
    /// When play was last pressed, as nanoseconds after `epoch`; zero
    /// once the callback has reported it.
    pressed: AtomicU64,
    epoch: Instant,
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
        let shared = Arc::new(Shared {
            samples,
            frame: AtomicUsize::new(0),
            playing: AtomicBool::new(false),
            loop_start: AtomicUsize::new(0),
            loop_end: AtomicUsize::new(0),
            pressed: AtomicU64::new(0),
            epoch: Instant::now(),
        });
        // The small buffer first; a device that refuses it gets cpal's
        // default, and says so.
        let mut stream = None;
        for size in [
            cpal::BufferSize::Fixed(BUFFER_FRAMES),
            cpal::BufferSize::Default,
        ] {
            let config = cpal::StreamConfig {
                channels: 2,
                sample_rate: cpal::SampleRate(SAMPLE_RATE),
                buffer_size: size,
            };
            match Self::open(&device, &config, shared.clone()) {
                Ok(s) => {
                    println!("audio: output stream open, buffer {size:?}");
                    stream = Some(s);
                    break;
                }
                Err(e) => println!("audio: buffer {size:?} refused ({e})"),
            }
        }
        let stream = stream.ok_or("audio stream: no buffer size accepted")?;
        stream.play().map_err(|e| format!("audio start: {e}"))?;
        Ok(Player {
            _stream: stream,
            shared,
        })
    }

    fn open(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        cb: Arc<Shared>,
    ) -> Result<cpal::Stream, String> {
        let mut last_call: Option<Instant> = None;
        let mut was_playing = false;
        device
            .build_output_stream(
                config,
                move |out: &mut [f32], _| {
                    let now = Instant::now();
                    let gap = last_call.map(|t| now.duration_since(t).as_secs_f32() * 1000.0);
                    last_call = Some(now);
                    let playing = cb.playing.load(Ordering::Relaxed);
                    if playing && !was_playing {
                        // The first buffer of a play: how long the press
                        // took to get here, and how long this thread had
                        // been asleep — the two places a delay can hide.
                        let pressed = cb.pressed.swap(0, Ordering::Relaxed);
                        let since_press = (pressed > 0).then(|| {
                            (now.duration_since(cb.epoch).as_nanos() as i128 - pressed as i128)
                                as f32
                                / 1.0e6
                        });
                        println!(
                            "audio: play reached the callback after {} ms; callback gap before it {} ms; buffer {} frames",
                            since_press.map(|ms| format!("{ms:.1}")).unwrap_or_else(|| "?".into()),
                            gap.map(|ms| format!("{ms:.1}")).unwrap_or_else(|| "?".into()),
                            out.len() / 2
                        );
                    }
                    was_playing = playing;
                    if !playing {
                        out.fill(0.0);
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
            .map_err(|e| format!("audio stream: {e}"))
    }

    /// Toggle play/pause; returns whether we're playing now. Toggling at the
    /// end of the track restarts from the top.
    pub fn toggle(&self) -> bool {
        let s = &self.shared;
        let now = !s.playing.load(Ordering::Relaxed);
        if now && s.frame.load(Ordering::Relaxed) * 2 >= s.samples.len() {
            s.frame.store(0, Ordering::Relaxed);
        }
        if now {
            let ns = Instant::now().duration_since(s.epoch).as_nanos() as u64;
            s.pressed.store(ns.max(1), Ordering::Relaxed);
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

#[cfg(test)]
mod tests {
    /// How much audio sits between the callback and the ear on this
    /// machine, by buffer setting — the number behind "press play and it
    /// starts late". Opens the real output device and plays silence, so
    /// it runs only when asked:
    /// `cargo test -p spark_audio -- --ignored --nocapture probe`.
    #[test]
    #[ignore]
    fn probe_output_latency() {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        use std::sync::{Arc, Mutex};
        let device = cpal::default_host().default_output_device().expect("an output device");
        println!("device: {}", device.name().unwrap_or_default());
        for (label, size) in [
            ("Default", cpal::BufferSize::Default),
            ("Fixed(1024)", cpal::BufferSize::Fixed(1024)),
            ("Fixed(2048)", cpal::BufferSize::Fixed(2048)),
        ] {
            let config = cpal::StreamConfig {
                channels: 2,
                sample_rate: cpal::SampleRate(crate::SAMPLE_RATE),
                buffer_size: size,
            };
            let seen: Arc<Mutex<Vec<(usize, f32)>>> = Arc::new(Mutex::new(Vec::new()));
            let log = seen.clone();
            let stream = match device.build_output_stream(
                &config,
                move |out: &mut [f32], info: &cpal::OutputCallbackInfo| {
                    out.fill(0.0);
                    let ts = info.timestamp();
                    let lat = ts
                        .playback
                        .duration_since(&ts.callback)
                        .map(|d| d.as_secs_f32() * 1000.0)
                        .unwrap_or(-1.0);
                    log.lock().unwrap().push((out.len() / 2, lat));
                },
                |e| eprintln!("stream error: {e}"),
                None,
            ) {
                Ok(s) => s,
                Err(e) => {
                    println!("{label}: could not open: {e}");
                    continue;
                }
            };
            stream.play().expect("start");
            std::thread::sleep(std::time::Duration::from_millis(1500));
            drop(stream);
            let v = seen.lock().unwrap();
            let n = v.len();
            let frames: Vec<usize> = v.iter().map(|x| x.0).collect();
            let (lo, hi) = v.iter().fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(x.1), b.max(x.1)));
            println!(
                "{label}: {n} callbacks in 1.5 s, frames/callback {:?}..{:?}, reported latency {lo:.1}..{hi:.1} ms",
                frames.iter().min(),
                frames.iter().max()
            );
        }
    }
}
