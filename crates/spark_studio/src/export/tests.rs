//! The export, end to end: a tiny comp rendered through the GPU, encoded
//! by this machine's FFmpeg, decoded back and looked at. Nobody who can
//! run this can watch the video, so the pixels are asserted: the one red
//! rectangle has to come back red, where it was, on black. That catches
//! the things a green build can't — BGRA read as RGBA (red comes back
//! blue), a row-padding slip (the picture shears), a flipped axis (the
//! rectangle changes quadrant), the wrong colour matrix (the red drifts).
//! Skips, saying so, where there is no GPU adapter or no FFmpeg.

use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use spark_render::{Scene, Shape, wgpu};

use super::*;

fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = spark_render::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .ok()?;
    spark_render::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

#[test]
fn a_tiny_comp_exports_to_an_mp4_that_decodes_back_to_its_pixels() {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("no ffmpeg — skipping");
        return;
    }
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter available — skipping");
        return;
    };
    let dir = std::env::temp_dir().join(format!("spark-export-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tiny.mp4").to_string_lossy().into_owned();
    let (tx, rx) = mpsc::channel();
    // Four frames of a 256×144 comp.
    let (w, h) = (256usize, 144usize);
    let range = (0.0, 4.0 / FPS as f32);
    let mut job = Job::start(
        &device,
        &queue,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        [w as f32, h as f32],
        range,
        None,
        path,
        move |r| {
            let _ = tx.send(r);
        },
    )
    .expect("export starts");
    // A red rectangle in the upper-left quadrant: a flip on either axis
    // moves it to another.
    let red = Shape::rect([64.0, 36.0], [40.0, 20.0])
        .color(1.0, 0.0, 0.0)
        .intensity(1.0);
    let camera = job.camera();
    let mut frames = 0;
    while !job.rendered_all() {
        let scene = Scene {
            shapes: std::slice::from_ref(&red),
            models: &[],
            paths: &[],
            meshes: &[],
            lights: &[],
            camera: &camera,
            time: job.next_time(),
            clocks: &[],
            over: 0,
        };
        job.render(&device, &queue, &scene);
        frames += 1;
    }
    assert_eq!(frames, 4);
    let out = rx
        .recv_timeout(Duration::from_secs(90))
        .expect("ffmpeg reported back")
        .expect("the export succeeded");
    // Decode it back to full-range RGBA and look.
    let raw = Command::new("ffmpeg")
        .args(["-v", "error", "-i", &out, "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
        .output()
        .unwrap();
    assert!(raw.status.success(), "{}", String::from_utf8_lossy(&raw.stderr));
    assert_eq!(raw.stdout.len(), w * h * 4 * 4, "four frames came back");
    let px = |x: usize, y: usize| &raw.stdout[(y * w + x) * 4..(y * w + x) * 4 + 3];
    let centre = px(64, 36);
    assert!(
        centre[0] > 200 && centre[1] < 60 && centre[2] < 60,
        "the rectangle's centre is red: {centre:?}"
    );
    for (x, y) in [(192, 108), (64, 108), (192, 36), (2, 2)] {
        let p = px(x, y);
        assert!(p.iter().all(|&v| v < 24), "({x}, {y}) is black: {p:?}");
    }
    // The last frame is the same picture.
    let last = &raw.stdout[w * h * 4 * 3..];
    let c = &last[(36 * w + 64) * 4..(36 * w + 64) * 4 + 3];
    assert!(c[0] > 200 && c[1] < 60, "frame 4 still shows the rectangle: {c:?}");
    std::fs::remove_dir_all(&dir).ok();
}
