//! Export: the comp rendered frame by frame at the canvas's size and
//! piped into FFmpeg as raw pixels, the song muxed in from the comp's
//! track — a real .mp4, made entirely with the tools.
//!
//! `frame = render(project, t)` is what makes this small. An export is
//! the same stage the viewport draws through, pointed at an offscreen
//! texture the canvas's size, asked for one frame per `1/FPS` of comp
//! time, read back and handed to a writer thread that feeds FFmpeg's
//! stdin. The editor keeps drawing between frames — the status strip
//! says how far along it is — and Esc cancels. Nothing about the
//! document knows it is being exported: the export poses it at each
//! frame's time and puts the playhead back where it found it.
//!
//! FFmpeg is a subprocess, as it is for audio decode (dependency policy:
//! piped through stdin/stdout, never linked). The encoder is whichever
//! H.264 the installed build has — NVENC on a machine with an NVIDIA
//! card, libx264 elsewhere — because H.264 in an MP4 is what every phone
//! app takes without a word. Frames go over as opaque BGRA over black:
//! the document's transparency is real, and a video has nowhere to put it.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::time::Instant;

use spark_render::{Camera, Framing, Quality, Scene, ShapePass, Stage, Viewport, wgpu};

mod ffmpeg;
#[cfg(test)]
mod tests;

pub use ffmpeg::{ffmpeg_args, frame_count};
use ffmpeg::{pix_fmt, probe_encoder};

/// Frames per second of comp time the video gets. Phone apps and
/// YouTube both take 60, and choreography cut to a riddim drop wants it.
pub const FPS: u32 = 60;

/// Frames ahead of FFmpeg the renderer may run before it waits.
const AHEAD: usize = 3;

/// An export in progress: the offscreen stage, the frames still to
/// render, and the thread feeding FFmpeg.
pub struct Job {
    size: (u32, u32),
    fps: u32,
    t0: f32,
    frames: u32,
    /// The next frame to render.
    next: u32,
    stage: Stage,
    pass: ShapePass,
    target: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    /// Bytes per row in the readback buffer: the frame's, padded to
    /// wgpu's 256-byte alignment.
    padded_bpr: u32,
    /// Frames on their way to FFmpeg. `None` once the last one has gone
    /// — dropping it is what tells the writer the picture is complete.
    tx: Option<SyncSender<Vec<u8>>>,
    cancel: Arc<AtomicBool>,
    started: Instant,
}

impl Job {
    /// Start an export: spawn FFmpeg, make the offscreen targets, and
    /// hand back the job to step. `canvas` is the comp's size in canvas
    /// units, which is the video's in pixels; `range` the comp seconds to
    /// render; `audio` the track's file to mux in. `done` is called from
    /// the writer thread once FFmpeg has finished, with the file it wrote
    /// or why it didn't — the studio turns that into an event.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        canvas: [f32; 2],
        range: (f32, f32),
        audio: Option<&str>,
        path: String,
        done: impl FnOnce(Result<String, String>) + Send + 'static,
    ) -> Result<Job, String> {
        let size = (canvas[0].round() as u32, canvas[1].round() as u32);
        if size.0 < 2 || size.1 < 2 || size.0 % 2 == 1 || size.1 % 2 == 1 {
            // yuv420p halves the chroma plane; an odd side has nowhere
            // to put its last column.
            return Err(format!("canvas {}x{} must be even on both sides", size.0, size.1));
        }
        if range.1 <= range.0 {
            return Err("nothing to export: the range is empty".into());
        }
        let pix = pix_fmt(format)?;
        let encoder = probe_encoder()?;
        let args = ffmpeg_args(encoder, pix, size, FPS, range, audio, &path);
        let mut child = Command::new("ffmpeg")
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("ffmpeg spawn failed: {e}"))?;
        let mut stdin = child.stdin.take().ok_or("ffmpeg has no stdin")?;
        println!(
            "exporting {}x{} @ {FPS} fps, {:.2}s–{:.2}s, {encoder} -> {path}",
            size.0, size.1, range.0, range.1
        );

        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(AHEAD);
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let out_path = path.clone();
        std::thread::spawn(move || {
            let mut broke = false;
            for frame in &rx {
                if stdin.write_all(&frame).is_err() {
                    // FFmpeg went away mid-stream: say why, then keep the
                    // channel draining so the renderer never blocks on a
                    // reader that isn't there.
                    broke = true;
                    break;
                }
            }
            drop(stdin);
            let result = if flag.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&out_path);
                Err("cancelled".to_string())
            } else {
                match child.wait_with_output() {
                    Ok(out) if out.status.success() && !broke => Ok(out_path),
                    Ok(out) => {
                        let _ = std::fs::remove_file(&out_path);
                        Err(format!(
                            "ffmpeg: {}",
                            String::from_utf8_lossy(&out.stderr).trim()
                        ))
                    }
                    Err(e) => Err(format!("ffmpeg: {e}")),
                }
            };
            done(result);
            for _ in rx {}
        });

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("export frame"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let padded_bpr = (size.0 * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("export readback"),
            size: (padded_bpr * size.1) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Job {
            size,
            fps: FPS,
            t0: range.0,
            frames: frame_count(range, FPS),
            next: 0,
            // Its own stage and pass: the editor's hold the window-sized
            // picture, and swapping targets every frame would thrash both.
            stage: Stage::new(device, queue, format),
            pass: ShapePass::new(device, format),
            target,
            view,
            readback,
            padded_bpr,
            tx: Some(tx),
            cancel,
            started: Instant::now(),
        })
    }

    /// Comp time of the frame to render next.
    pub fn next_time(&self) -> f32 {
        self.t0 + self.next as f32 / self.fps as f32
    }

    /// Every frame has been handed to FFmpeg; only the encode is left.
    pub fn rendered_all(&self) -> bool {
        self.next >= self.frames
    }

    /// The render camera for this export: the stage's, on this canvas.
    pub fn camera(&self) -> Camera {
        Camera::stage([self.size.0 as f32, self.size.1 as f32])
    }

    /// Draw `scene` — the document posed at [`Job::next_time`] — as the
    /// next frame and hand it to FFmpeg. Blocks while FFmpeg is more than
    /// [`AHEAD`] frames behind.
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, scene: &Scene) {
        let (w, h) = self.size;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("export"),
        });
        // Opaque black under the document: transparency is real in the
        // editor and a video has no alpha, so black is what shows.
        encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("export clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            })
            .forget_lifetime();
        // One canvas unit is one pixel: the video is the canvas.
        let framing = Framing::Canvas {
            cview: (1.0, 0.0, 0.0),
            clip: Viewport {
                x: 0.0,
                y: 0.0,
                w: w as f32,
                h: h as f32,
            },
        };
        self.stage.draw(
            device,
            queue,
            &mut encoder,
            &self.view,
            &mut self.pass,
            scene,
            self.size,
            framing,
            Quality::Full,
        );
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bpr),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let slice = self.readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .expect("poll");
        let frame = {
            let data = slice.get_mapped_range();
            unpad(&data, w, h, self.padded_bpr)
        };
        self.readback.unmap();
        self.next += 1;
        if let Some(tx) = &self.tx
            && tx.send(frame).is_err()
        {
            // The writer is gone (FFmpeg failed and it has reported);
            // there is nothing left to feed.
            self.next = self.frames;
        }
        if self.rendered_all() {
            // Close the pipe: FFmpeg writes the file's index and exits,
            // and the writer thread reports back.
            self.tx = None;
        }
    }

    /// Esc: stop, kill FFmpeg, and remove the half-written file.
    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.next = self.frames;
        self.tx = None;
    }

    /// What the status strip says.
    pub fn status(&self) -> String {
        let (w, h) = self.size;
        if self.rendered_all() {
            format!("Encoding {} frames · {w}×{h} @ {} fps", self.frames, self.fps)
        } else {
            let pct = self.next * 100 / self.frames.max(1);
            let rate = self.next as f32 / self.started.elapsed().as_secs_f32().max(0.001);
            format!(
                "Rendering {pct}% · {} / {} frames · {w}×{h} @ {} fps · {rate:.0} fps · Esc cancels",
                self.next, self.frames, self.fps
            )
        }
    }

    pub fn elapsed(&self) -> f32 {
        self.started.elapsed().as_secs_f32()
    }
}

/// The frame's rows out of a row-padded readback, tightly packed.
fn unpad(data: &[u8], w: u32, h: u32, padded_bpr: u32) -> Vec<u8> {
    let row = (w * 4) as usize;
    let mut out = Vec::with_capacity(row * h as usize);
    for y in 0..h as usize {
        let start = y * padded_bpr as usize;
        out.extend_from_slice(&data[start..start + row]);
    }
    out
}

#[cfg(test)]
mod unpad_tests {
    use super::*;

    /// Rows come out of the padded readback tightly packed.
    #[test]
    fn readback_rows_are_unpadded() {
        // 3 px wide (12 bytes), padded to 16, 2 rows.
        let mut data = vec![0u8; 32];
        for (i, b) in data.iter_mut().enumerate() {
            *b = i as u8;
        }
        let out = unpad(&data, 3, 2, 16);
        assert_eq!(out.len(), 24);
        assert_eq!(&out[..12], &data[..12]);
        assert_eq!(&out[12..], &data[16..28]);
    }
}
