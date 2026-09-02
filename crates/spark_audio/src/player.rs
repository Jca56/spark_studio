//! Playback: a cpal output stream running on **timeline time**, fed by
//! the mixer. The transport clock *is* the audio callback's cursor, so
//! what you hear and what the playhead shows can never drift — and the
//! cursor runs whether or not any clip covers it: an intro before the
//! song is silence the stream plays through, not time the clock skips.
//!
//! The voices — the audio clips as the mixer hears them — are swapped
//! in whole by the studio whenever the arrangement changes; the
//! callback picks the new set up on its next buffer without ever
//! waiting on a lock.
//!
//! The device buffer is asked for at [`BUFFER_FRAMES`] — two PipeWire
//! quanta, 43 ms — rather than cpal's 100 ms default: what is queued
//! ahead of the ear is what a press has to wait through. The stream
//! also **times every play**: the callback reports how long the press
//! took to reach it and how long the callback had been asleep before,
//! so "it starts a second late" is a number, not a guess.
//!
//! **Stopped means stopped**: the stream is paused whenever the
//! transport is — no zeros flow to the device between plays, which is
//! what Firefox and Ableton do and what a paused player ought to do.
//! Measured on Alva's machine (2026-09-01, `probe_output_latency`): the
//! PipeWire ALSA plugin pauses cleanly (no callbacks while paused) and
//! resumes to the first callback within 0.5–40 ms.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::SAMPLE_RATE;
use crate::mix::{Voice, mix};

/// The device buffer asked for, in frames: two PipeWire quanta at 48 k.
/// cpal's default is 100 ms, all of it queued ahead of the ear.
pub const BUFFER_FRAMES: u32 = 2048;

struct Shared {
    /// The clips as the mixer hears them. The callback keeps its own
    /// copy and refreshes it with a `try_lock` — a swap in progress
    /// means one more buffer of the old set, never a stall.
    voices: Mutex<Arc<Vec<Voice>>>,
    /// Playback cursor in timeline frames.
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
    /// The last play's timing, for the status strip: press-to-callback
    /// and the callback's sleep before it, both in tenths of a ms,
    /// packed high/low; zero once read.
    report: AtomicU64,
}

pub struct Player {
    stream: cpal::Stream,
    shared: Arc<Shared>,
    /// Whether the device stream is paused (the transport stopped).
    paused: AtomicBool,
}

impl Player {
    /// Open the output device and hold it paused. Nothing plays until
    /// voices arrive and the transport runs.
    pub fn new() -> Result<Player, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no audio output device")?;
        let shared = Arc::new(Shared {
            voices: Mutex::new(Arc::new(Vec::new())),
            frame: AtomicUsize::new(0),
            playing: AtomicBool::new(false),
            loop_start: AtomicUsize::new(0),
            loop_end: AtomicUsize::new(0),
            pressed: AtomicU64::new(0),
            epoch: Instant::now(),
            report: AtomicU64::new(0),
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
        // Start, then hold: ALSA only pauses a running PCM, and the worker
        // needs a moment to have written its first period. From here the
        // stream runs only while the transport does.
        stream.play().map_err(|e| format!("audio start: {e}"))?;
        std::thread::sleep(std::time::Duration::from_millis(60));
        stream.pause().map_err(|e| format!("audio hold: {e}"))?;
        Ok(Player {
            stream,
            shared,
            paused: AtomicBool::new(true),
        })
    }

    fn open(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        cb: Arc<Shared>,
    ) -> Result<cpal::Stream, String> {
        let mut last_call: Option<Instant> = None;
        let mut was_playing = false;
        let mut voices: Arc<Vec<Voice>> = Arc::new(Vec::new());
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
                        let tenths = |ms: Option<f32>| (ms.unwrap_or(0.0).max(0.0) * 10.0) as u64;
                        cb.report.store(
                            (tenths(since_press) << 32) | tenths(gap).min(u32::MAX as u64) | 1,
                            Ordering::Relaxed,
                        );
                    }
                    was_playing = playing;
                    if !playing {
                        out.fill(0.0);
                        return;
                    }
                    if let Ok(v) = cb.voices.try_lock() {
                        voices = v.clone();
                    }
                    let l0 = cb.loop_start.load(Ordering::Relaxed);
                    let l1 = cb.loop_end.load(Ordering::Relaxed);
                    let looping = l1 > l0;
                    let mut pos = cb.frame.load(Ordering::Relaxed);
                    let mut filled = 0usize;
                    let out_frames = out.len() / 2;
                    while filled < out_frames {
                        // Arriving at the loop end (fills clamp exactly to
                        // it) wraps to the start, even mid-buffer, so loops
                        // stay sample-tight. A cursor seeked past the region
                        // plays on normally — and on, and on: the timeline
                        // has no end, so neither does the clock.
                        if looping && pos == l1 {
                            pos = l0;
                        }
                        let limit = if looping && pos < l1 { l1 } else { usize::MAX };
                        let n = (out_frames - filled).min(limit - pos).max(1);
                        mix(&mut out[filled * 2..(filled + n) * 2], pos, &voices);
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

    /// Replace what the mixer hears. Cheap enough to call whenever the
    /// arrangement changes.
    pub fn set_voices(&self, voices: Vec<Voice>) {
        if let Ok(mut v) = self.shared.voices.lock() {
            *v = Arc::new(voices);
        }
    }

    /// Toggle play/pause; returns whether we're playing now. The device
    /// stream runs while playing and is paused otherwise.
    pub fn toggle(&self) -> bool {
        let s = &self.shared;
        let now = !s.playing.load(Ordering::Relaxed);
        if now {
            let ns = Instant::now().duration_since(s.epoch).as_nanos() as u64;
            s.pressed.store(ns.max(1), Ordering::Relaxed);
            s.playing.store(true, Ordering::Relaxed);
            self.run(true);
        } else {
            // Pause first: what was queued ahead of the ear stops with it.
            self.run(false);
            s.playing.store(false, Ordering::Relaxed);
        }
        now
    }

    /// Run or hold the device stream, once per change.
    fn run(&self, on: bool) {
        if self.paused.swap(!on, Ordering::Relaxed) != on {
            return;
        }
        let r: Result<(), String> = if on {
            self.stream.play().map_err(|e| e.to_string())
        } else {
            self.stream.pause().map_err(|e| e.to_string())
        };
        if let Err(e) = r {
            eprintln!("audio {}: {e}", if on { "resume" } else { "hold" });
        }
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    /// The last play's timing, once: (press to callback, the callback's
    /// sleep before it), in ms — what the status strip shows.
    pub fn take_play_report(&self) -> Option<(f32, f32)> {
        let v = self.shared.report.swap(0, Ordering::Relaxed);
        (v != 0).then(|| ((v >> 32) as f32 / 10.0, (v & 0xFFFF_FFFF) as f32 / 10.0))
    }

    /// Current position in seconds, straight off the audio cursor.
    pub fn time(&self) -> f32 {
        self.shared.frame.load(Ordering::Relaxed) as f32 / SAMPLE_RATE as f32
    }

    /// To the nearest frame — truncating landed every seek a hair *early*,
    /// and a hair before a clip's start is outside the clip (the object
    /// vanished, the playhead hid, a looping clip's local clock wrapped to
    /// its end: Alva, 2026-09-01).
    pub fn seek(&self, t: f32) {
        let frame = (t.max(0.0) * SAMPLE_RATE as f32).round() as usize;
        self.shared.frame.store(frame, Ordering::Relaxed);
    }

    /// Loop playback between `start` and `end` seconds (sample-accurate).
    pub fn set_loop(&self, start: f32, end: f32) {
        let a = (start.max(0.0) * SAMPLE_RATE as f32) as usize;
        let b = (end.max(0.0) * SAMPLE_RATE as f32) as usize;
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
            // Can the stream be paused, and how fast does it come back?
            // Callbacks during a paused second say whether pause took;
            // the wait for the first callback after play() is the resume.
            let n_before = seen.lock().unwrap().len();
            let paused = stream.pause();
            std::thread::sleep(std::time::Duration::from_millis(1000));
            let n_paused = seen.lock().unwrap().len() - n_before;
            let t0 = std::time::Instant::now();
            let n_mark = seen.lock().unwrap().len();
            stream.play().expect("resume");
            while seen.lock().unwrap().len() == n_mark && t0.elapsed().as_millis() < 3000 {
                std::thread::sleep(std::time::Duration::from_micros(200));
            }
            println!(
                "{label}: pause() -> {paused:?}; callbacks during the paused second: {n_paused}; first callback after play(): {:.1} ms",
                t0.elapsed().as_secs_f32() * 1000.0
            );
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
