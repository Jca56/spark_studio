//! The FFmpeg side of an export: which encoder this machine has, and the
//! command line that turns raw frames on stdin into an .mp4. Pure apart
//! from the probe, so the shape of the command can be tested without a
//! GPU or an FFmpeg.

use std::process::Command;

use spark_render::wgpu;

/// H.264 encoders in order of preference, by FFmpeg's names for them.
const ENCODERS: [&str; 3] = ["h264_nvenc", "libx264", "libopenh264"];

/// What an encoder is told, after `-c:v <name>`.
fn encoder_args(name: &str) -> &'static [&'static str] {
    match name {
        // Constant quality on the card: `cq 19` is visually lossless for
        // neon on black and encodes faster than the frames render.
        "h264_nvenc" => &[
            "-preset", "p6", "-tune", "hq", "-rc", "vbr", "-cq", "19", "-b:v", "0", "-profile:v",
            "high",
        ],
        "libx264" => &["-preset", "medium", "-crf", "18", "-profile:v", "high"],
        // No quality mode at all; a fat bitrate stands in.
        _ => &["-b:v", "16M"],
    }
}

/// The H.264 encoder this machine's FFmpeg has, or why none.
pub(super) fn probe_encoder() -> Result<&'static str, String> {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .map_err(|e| format!("ffmpeg not found: {e}"))?;
    let listing = String::from_utf8_lossy(&out.stdout);
    let have: Vec<&str> = listing
        .lines()
        .filter_map(|l| l.split_whitespace().nth(1))
        .collect();
    ENCODERS
        .iter()
        .copied()
        .find(|e| have.contains(e))
        .ok_or_else(|| format!("ffmpeg has no H.264 encoder (looked for {})", ENCODERS.join(", ")))
}

/// FFmpeg's name for the bytes a frame of `format` reads back as.
pub(super) fn pix_fmt(format: wgpu::TextureFormat) -> Result<&'static str, String> {
    match format {
        wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Bgra8Unorm => Ok("bgra"),
        wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Rgba8Unorm => Ok("rgba"),
        other => Err(format!("can't export from a {other:?} frame")),
    }
}

/// How many frames `range` seconds of comp time are at `fps` — never
/// none: a video has at least one picture in it.
pub fn frame_count((t0, t1): (f32, f32), fps: u32) -> u32 {
    (((t1 - t0) * fps as f32).round() as u32).max(1)
}

/// The whole FFmpeg command line after the binary: raw frames on stdin,
/// the song (if any) from its file cut to the same range, H.264 in an
/// MP4 at `path`. Pure, so the shape of it can be tested without a GPU
/// or an FFmpeg.
pub fn ffmpeg_args(
    encoder: &str,
    pix: &str,
    size: (u32, u32),
    fps: u32,
    range: (f32, f32),
    audio: Option<&str>,
    path: &str,
) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-v".into(),
        "error".into(),
        "-y".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        pix.into(),
        "-s".into(),
        format!("{}x{}", size.0, size.1),
        "-r".into(),
        fps.to_string(),
        "-i".into(),
        "pipe:0".into(),
    ];
    let frames = frame_count(range, fps);
    if let Some(song) = audio {
        // Seek before the input, so the cut is accurate and the audio's
        // zero is the comp's zero: the frames start at `t0`, so does the
        // sound, and both run exactly `frames / fps` long.
        a.extend([
            "-ss".into(),
            format!("{}", range.0),
            "-t".into(),
            format!("{}", frames as f32 / fps as f32),
            "-i".into(),
            song.into(),
        ]);
    }
    a.extend(["-map".into(), "0:v".into()]);
    if audio.is_some() {
        a.extend(["-map".into(), "1:a".into()]);
    }
    a.extend(["-c:v".into(), encoder.into()]);
    a.extend(encoder_args(encoder).iter().map(|s| s.to_string()));
    // Neon is colour, so the RGB→YUV matrix is said out loud: HD video
    // is BT.709, and left unsaid the converter reaches for BT.601 while
    // the player assumes 709, and every hue drifts.
    a.extend([
        "-vf".into(),
        "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p".into(),
        "-colorspace".into(),
        "bt709".into(),
        "-color_primaries".into(),
        "bt709".into(),
        "-color_trc".into(),
        "bt709".into(),
        "-color_range".into(),
        "tv".into(),
    ]);
    if audio.is_some() {
        a.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "320k".into()]);
    }
    a.extend(["-movflags".into(), "+faststart".into(), path.into()]);
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A video is a whole number of frames, never none.
    #[test]
    fn a_range_is_a_whole_number_of_frames() {
        assert_eq!(frame_count((0.0, 10.0), 60), 600);
        assert_eq!(frame_count((8.0, 12.0), 30), 120);
        assert_eq!(frame_count((0.0, 0.001), 60), 1);
        assert_eq!(frame_count((0.0, 1.0 / 60.0 * 90.4), 60), 90);
    }

    /// A phone-sized export: raw BGRA in at the canvas's size and rate,
    /// the song cut to the same range, H.264 + AAC in an MP4 — and no
    /// audio stream at all for a silent comp.
    #[test]
    fn the_ffmpeg_line_carries_the_size_the_rate_and_the_song() {
        let a = ffmpeg_args(
            "h264_nvenc",
            "bgra",
            (1080, 1920),
            60,
            (8.0, 12.0),
            Some("/music/drop.wav"),
            "/out/first.mp4",
        );
        let has = |pair: [&str; 2]| a.windows(2).any(|w| w[0] == pair[0] && w[1] == pair[1]);
        assert!(has(["-s", "1080x1920"]));
        assert!(has(["-r", "60"]));
        assert!(has(["-pix_fmt", "bgra"]));
        assert!(has(["-i", "pipe:0"]));
        assert!(has(["-ss", "8"]) && has(["-t", "4"]), "{a:?}");
        assert!(has(["-i", "/music/drop.wav"]));
        assert!(has(["-map", "0:v"]) && has(["-map", "1:a"]));
        assert!(has(["-c:v", "h264_nvenc"]) && has(["-c:a", "aac"]));
        assert_eq!(a.last().map(String::as_str), Some("/out/first.mp4"));
        // The song comes after the frames, so `1:a` really is the song.
        let song = a.iter().position(|s| s == "/music/drop.wav").unwrap();
        let pipe = a.iter().position(|s| s == "pipe:0").unwrap();
        assert!(pipe < song);

        let silent = ffmpeg_args("libx264", "rgba", (1920, 1080), 60, (0.0, 3.0), None, "x.mp4");
        assert!(!silent.iter().any(|s| s == "-ss" || s == "1:a" || s == "aac"));
        assert!(silent.iter().any(|s| s == "libx264"));
    }
}
