//! spark_text — Spark's text API, backed by lntrn-type.
//!
//! A deliberate seam: every call site goes through this wrapper, so the
//! backend can evolve (or be swapped) without touching widget code.

use std::sync::Arc;

use lntrn_draw::Color;
use lntrn_type::TextRenderer;

/// Spark's bundled UI face: Space Mono (OFL) — Alva's pick.
const UI_FONT: &[u8] = include_bytes!("../assets/SpaceMono-Regular.ttf");

pub struct Text {
    inner: TextRenderer,
}

impl Text {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let mut inner = TextRenderer::from_wgpu(
            Arc::new(device.clone()),
            Arc::new(queue.clone()),
            format,
            false,
        );
        inner.load_font_data(UI_FONT.to_vec());
        Self { inner }
    }

    /// Queue a label. `x`, `y` are the top-left of the line box in physical
    /// px; `size` is the font size in physical px.
    pub fn label(
        &mut self,
        text: &str,
        size: f32,
        x: f32,
        y: f32,
        rgba: [f32; 4],
        max_width: f32,
        resolution: (u32, u32),
    ) {
        self.inner.queue(
            text,
            size,
            x,
            y,
            Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3]),
            max_width,
            resolution.0,
            resolution.1,
        );
    }

    pub fn measure(&mut self, text: &str, size: f32) -> f32 {
        self.inner.measure_width(text, size)
    }

    /// Height of one line box at `size` (lntrn-type layout uses 1.2em).
    pub fn line_height(size: f32) -> f32 {
        size * 1.2
    }

    /// Render everything queued this frame, compositing over `view`.
    pub fn draw(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        resolution: (u32, u32),
    ) {
        self.inner.render(encoder, view, resolution.0, resolution.1);
    }
}
