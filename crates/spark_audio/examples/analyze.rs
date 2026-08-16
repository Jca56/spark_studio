//! Headless engine check: decode + analyze a track, print the stats.
//!
//!     cargo run -p spark_audio --example analyze -- song.wav

use std::path::Path;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: analyze <audio file>");
    let start = std::time::Instant::now();
    match spark_audio::Track::load(Path::new(&path)) {
        Ok(t) => {
            println!("decoded + analyzed in {:.2?}", start.elapsed());
            println!(
                "duration: {:.2}s  stereo samples: {}",
                t.duration,
                t.samples.len()
            );
            println!(
                "peaks: {}  curves: {} samples @ {:.1}/s",
                t.peaks.len(),
                t.curves.bass.len(),
                t.curves.rate
            );
            println!(
                "beat grid: ~{:.1} BPM, first bar at {:.2}s",
                t.beat.bpm, t.beat.first_bar
            );
            let c = &t.curves;
            for (name, curve) in [
                ("bass", &c.bass),
                ("low_mid", &c.low_mid),
                ("mid", &c.mid),
                ("high", &c.high),
                ("rms", &c.rms),
                ("onset", &c.onset),
            ] {
                let avg = curve.iter().sum::<f32>() / curve.len().max(1) as f32;
                let hot = curve.iter().filter(|&&v| v > 0.8).count();
                println!("  {name:8} avg {avg:.3}   samples >0.8: {hot}");
            }
        }
        Err(e) => println!("FAILED: {e}"),
    }
}
