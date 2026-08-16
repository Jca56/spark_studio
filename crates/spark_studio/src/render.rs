//! Frame assembly: one redraw = shape pass, UI rect pass, text pass.
//! Split from main so the event plumbing stays readable.

use std::path::Path;

use spark_render::wgpu;
use spark_ui::{IconBar, Slider, TitleBar, UiRect, theme};

use crate::{Studio, TOOLS, chrome, editor, inspector, layers, menu, timeline};

impl Studio {
    pub(crate) fn redraw(&mut self) {
        let Some(layout) = self.layout() else { return };
        let scale = self.scale();
        let tool = self.editor.tool();
        let title_hover = self.title_hover;
        let (Some(gpu), Some(shape_pass), Some(ui_pass), Some(text)) = (
            &mut self.gpu,
            &mut self.shape_pass,
            &mut self.ui_pass,
            &mut self.text,
        ) else {
            return;
        };
        let wm_size = chrome::WM_SIZE * scale;
        let wordmark_w = text.measure_bold("SPARK STUDIO", wm_size);
        self.wordmark_w = wordmark_w;
        let ui_size = chrome::UI_TEXT * scale;
        self.file_w = text.measure("File", chrome::MENU_TEXT * scale);
        self.menu_item_w = menu::FILE_ITEMS
            .iter()
            .fold(0.0f32, |w, s| w.max(text.measure(s, ui_size)));
        let tb = TitleBar::new(layout.title, scale, wordmark_w);
        let file_menu = menu::build(&layout, scale, self.file_w, self.menu_item_w);
        let insp = self
            .editor
            .selected_props()
            .map(|p| inspector::build(layout.left, scale, &p));
        let layer_rows = layers::rows(
            layout.right,
            scale,
            self.editor.shapes(),
            self.editor.selection(),
        );
        let Some(frame) = gpu.begin_frame() else {
            return;
        };
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let clear = wgpu::Color {
            r: 0.008,
            g: 0.004,
            b: 0.022,
            a: 1.0,
        };
        shape_pass.draw(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &frame.view,
            &self.editor.display_shapes(),
            gpu.size(),
            layout.viewport,
            clear,
        );
        let mut ui = layout.panel_rects(scale);
        ui.extend(tb.rects(title_hover));
        ui.extend(file_menu.anchor_rects(self.menu_open, self.menu_anchor_hover));
        ui.extend(IconBar::new(layout.top, scale, &TOOLS).rects(self.tool_hover, Some(tool)));
        let th = theme();
        if let Some(insp) = &insp {
            for row in &insp.rows {
                ui.extend(Slider::rects(row.track, row.t));
            }
            ui.extend(insp.swatches.rects(&editor::PALETTE, insp.palette));
            if let Some(mode) = &insp.mode {
                ui.extend(mode.seg.rects(mode.outline as usize));
            }
        }
        for lr in &layer_rows {
            let bg = if lr.selected { th.accent_bg } else { th.card };
            ui.push(UiRect::region_rounded(lr.row, bg, 10.0 * scale));
            ui.push(UiRect::region_rounded(
                lr.chip,
                [lr.rgb[0], lr.rgb[1], lr.rgb[2], 1.0],
                lr.chip.w * 0.3,
            ));
            ui.push(UiRect::icon_sized(
                lr.icon,
                lr.icon_kind,
                2.0 * scale,
                th.icon,
                0.28,
            ));
        }
        if let Some(track) = &self.audio {
            let strip = timeline::strip(layout.timeline, scale);
            ui.extend(timeline::waveform_rects(&strip, scale, &track.peaks));
            let playing = self.player.as_ref().is_some_and(|p| p.is_playing());
            ui.extend(timeline::transport_rects(
                &strip,
                scale,
                playing,
                self.transport_hover,
            ));
            if let Some(p) = &self.player {
                let t01 = (p.time() / track.duration.max(0.001)).clamp(0.0, 1.0);
                ui.push(timeline::playhead_rect(&strip, scale, t01));
            }
        }
        if self.menu_open {
            // Last so the panel floats over everything beneath it.
            ui.extend(file_menu.panel_rects(self.menu_hover));
        }
        ui_pass.draw(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &frame.view,
            &ui,
            gpu.size(),
        );

        // Labels — lntrn-text's first flight outside Lantern.
        let res = gpu.size();
        let file_name = Path::new(&self.current_file)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.current_file.clone());
        let audio_note = self
            .audio_loading
            .as_ref()
            .map(|name| format!("Analyzing {name}..."));
        let scene = chrome::Scene {
            insp: insp.as_ref(),
            layers: &layer_rows,
            menu: &file_menu,
            menu_open: self.menu_open,
            file: &file_name,
            audio_note: audio_note.as_deref(),
        };
        chrome::labels(text, &layout, scale, &tb, &scene, res);
        text.draw(&mut encoder, &frame.view, res);

        gpu.queue.submit([encoder.finish()]);
        frame.present();
    }
}
