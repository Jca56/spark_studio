//! Decode any audio file to raw PCM via an FFmpeg subprocess (piped, never
//! linked). Output: interleaved stereo f32 at [`crate::SAMPLE_RATE`].

use std::path::Path;
use std::process::Command;

pub fn decode(path: &Path) -> Result<Vec<f32>, String> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "f32le", "-ac", "2", "-ar", "48000", "pipe:1"])
        .output()
        .map_err(|e| format!("ffmpeg spawn failed: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "ffmpeg: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out
        .stdout
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect())
}
