//! spark_text — Spark's text API, backed by lntrn-text.
//!
//! A deliberate seam: every call site goes through this wrapper, so the
//! backend can evolve (or be swapped) without touching widget code.

use std::sync::Arc;

use lntrn_draw::Color;
use lntrn_text::{FontStyle, FontWeight, TextRenderer};

/// Spark's bundled UI face: Space Mono (OFL) — Alva's pick.
const UI_FONT: &[u8] = include_bytes!("../assets/SpaceMono-Regular.ttf");
const UI_FONT_BOLD: &[u8] = include_bytes!("../assets/SpaceMono-Bold.ttf");

/// The weight the whole editor is set in.
///
/// Bold, and deliberately: Space Mono ships two weights, 400 and 700, and at
/// the sizes Spark draws chrome the regular's stems land near a single
/// pixel. A one-pixel stem is mostly *edge*, and an edge pixel is only
/// partly covered — so the face read as the thinnest thing on screen, and
/// dark text on a light surface nearly vanished. There is no intermediate
/// weight to reach for; 700 is the step.
const UI_WEIGHT: FontWeight = FontWeight::Bold;

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
        inner.load_font_data(UI_FONT_BOLD.to_vec());
        Self { inner }
    }

    /// Queue a label. `x`, `y` are the top-left of the line box in physical
    /// px; `size` is the font size in physical px.
    #[allow(clippy::too_many_arguments)]
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
        self.inner.queue_styled(
            text,
            size,
            x,
            y,
            Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3]),
            max_width,
            UI_WEIGHT,
            FontStyle::Normal,
            resolution.0,
            resolution.1,
        );
    }

    /// Queue a bold label (weight-matched against the loaded faces).
    #[allow(clippy::too_many_arguments)]
    pub fn label_bold(
        &mut self,
        text: &str,
        size: f32,
        x: f32,
        y: f32,
        rgba: [f32; 4],
        max_width: f32,
        resolution: (u32, u32),
    ) {
        self.inner.queue_styled(
            text,
            size,
            x,
            y,
            Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3]),
            max_width,
            FontWeight::Bold,
            FontStyle::Normal,
            resolution.0,
            resolution.1,
        );
    }

    /// Width of a label as `label` will draw it — same weight, or every
    /// centred string would be measured against a face nobody sees.
    pub fn measure(&mut self, text: &str, size: f32) -> f32 {
        self.inner
            .measure_width_styled(text, size, UI_WEIGHT, FontStyle::Normal)
    }

    pub fn measure_bold(&mut self, text: &str, size: f32) -> f32 {
        self.inner
            .measure_width_styled(text, size, FontWeight::Bold, FontStyle::Normal)
    }

    /// Height of one line box at `size` (lntrn-text layout uses 1.2em).
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Building a renderer compiles lntrn-text's glyph shader on a real
    /// adapter, so a broken one fails here rather than as a window with no
    /// text in it. wgpu panics on uncaptured validation errors, so getting
    /// through `Text::new` at all is the assertion.
    ///
    /// spark_text is the only crate that knows the backend, which makes it
    /// the only place this check can live.
    #[test]
    fn the_glyph_shader_compiles_on_this_gpu() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let Ok(adapter) =
            spark_render::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            }))
        else {
            eprintln!("no GPU adapter available — skipping");
            return;
        };
        let Ok((device, queue)) =
            spark_render::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        else {
            eprintln!("no device — skipping");
            return;
        };
        let mut text = Text::new(&device, &queue, wgpu::TextureFormat::Rgba8UnormSrgb);
        // And that the bundled face actually measures, so a font that
        // failed to load reads as a failure rather than as zero-width text.
        assert!(text.measure("Spark", 20.0) > 0.0, "the UI face is missing");
    }
}
