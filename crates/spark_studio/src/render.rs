//! Frame assembly: one redraw = shape pass, UI rect pass, text pass.
//! Split from main so the event plumbing stays readable.

use std::path::Path;

use spark_render::{CANVAS_H, CANVAS_W, Shape, wgpu};
use spark_ui::{IconBar, Slider, TextField, TitleBar, UiRect, srgb, theme};

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
        self.anchor_ws = [
            text.measure("File", chrome::MENU_TEXT * scale),
            text.measure("View", chrome::MENU_TEXT * scale),
        ];
        self.menu_item_w = menu::FILE_ITEMS
            .iter()
            .chain(menu::VIEW_ITEMS.iter())
            .fold(0.0f32, |w, s| w.max(text.measure(s, ui_size)));
        let tb = TitleBar::new(layout.title, scale, wordmark_w);
        let menus = menu::build(&layout, scale, self.anchor_ws, self.menu_item_w);
        let insp = self
            .editor
            .selected_props()
            .map(|p| inspector::build(layout.left, scale, &p));
        let layer_rows = layers::rows(
            layout.right,
            scale,
            self.editor.shapes(),
            self.editor.names(),
            self.editor.selection(),
        );
        let Some(frame) = gpu.begin_frame() else {
            return;
        };
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        // The gutter around the stage: near-black chrome, the void behind
        // every surface.
        let void = srgb(0x0a0a0a);
        let clear = wgpu::Color {
            r: void[0] as f64,
            g: void[1] as f64,
            b: void[2] as f64,
            a: 1.0,
        };
        // The stage itself paints its background as the bottom shape — with
        // layered compositing it reads as its own surface.
        let [sr, sg, sb] = if self.view_black {
            [0.0, 0.0, 0.0]
        } else {
            [0.008, 0.004, 0.022]
        };
        let stage = Shape::rect(
            [CANVAS_W * 0.5, CANVAS_H * 0.5],
            [CANVAS_W * 0.5, CANVAS_H * 0.5],
        )
        .color(sr, sg, sb)
        .intensity(1.0)
        .glow(2.0);
        let mut shapes = vec![stage];
        let mut overlay_n = 1;
        if self.editor.snap_grid {
            // Faint 60-unit grid, drawn as light under the document shapes.
            for gx in 1..(CANVAS_W / 60.0) as usize {
                let x = gx as f32 * 60.0;
                let mut l = Shape::line([x, 0.0], [x, CANVAS_H], 0.75)
                    .color(1.0, 1.0, 1.0)
                    .intensity(0.05)
                    .glow(2.0);
                l.set_additive(true);
                shapes.push(l);
            }
            for gy in 1..(CANVAS_H / 60.0) as usize {
                let y = gy as f32 * 60.0;
                let mut l = Shape::line([0.0, y], [CANVAS_W, y], 0.75)
                    .color(1.0, 1.0, 1.0)
                    .intensity(0.05)
                    .glow(2.0);
                l.set_additive(true);
                shapes.push(l);
            }
            overlay_n = shapes.len();
        }
        shapes.extend(self.editor.display_shapes());
        if let (Some(track), Some(player)) = (&self.audio, &self.player)
            && player.is_playing()
        {
            // Render-time audio reaction: the document never changes, the
            // copies drawn this frame just ride the analysis curves.
            let t = player.time();
            let c = &track.curves;
            let bass = spark_audio::Curves::sample(&c.bass, c.rate, t);
            let mid = spark_audio::Curves::sample(&c.mid, c.rate, t);
            let onset = spark_audio::Curves::sample(&c.onset, c.rate, t);
            // Skip the stage background and grid overlay. Bass moves size
            // and glow (kick/sub weight); mids carry the wobble into
            // brightness; onsets snap.
            let n = (overlay_n + self.editor.shapes().len()).min(shapes.len());
            for s in &mut shapes[overlay_n..n] {
                s.add_glow(bass * 40.0);
                s.add_intensity(bass * 0.3 + mid * 0.45 + onset * 0.25);
                s.scale_by(1.0 + bass * 0.05);
            }
        }
        shape_pass.draw(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &frame.view,
            &shapes,
            gpu.size(),
            layout.viewport,
            clear,
        );
        let mut ui = layout.panel_rects(scale);
        ui.extend(tb.rects(title_hover));
        for (mi, m) in menus.iter().enumerate() {
            ui.extend(m.anchor_rects(
                self.menu_open == Some(mi),
                self.menu_anchor_hover == Some(mi),
            ));
        }
        ui.extend(IconBar::new(layout.top, scale, &TOOLS).rects(self.tool_hover, Some(tool)));
        let th = theme();
        if let Some(insp) = &insp {
            ui.push(UiRect::region_rounded(insp.card, th.card, 12.0 * scale));
            for row in &insp.rows {
                ui.extend(Slider::rects(row.track, row.t));
            }
            ui.extend(insp.swatches.rects(&editor::PALETTE, insp.palette));
            if let Some(mode) = &insp.mode {
                ui.extend(mode.seg.rects(mode.on as usize));
            }
            ui.extend(insp.blend.seg.rects(insp.blend.on as usize));
        }
        for lr in &layer_rows {
            let bg = if lr.selected { th.accent_bg } else { th.card };
            ui.push(UiRect::region_rounded(lr.row, bg, 10.0 * scale));
            ui.push(UiRect::region_rounded(
                lr.chip,
                [lr.rgb[0], lr.rgb[1], lr.rgb[2], 1.0],
                lr.chip.w * 0.3,
            ));
            let mut icon = UiRect::icon_sized(lr.icon, lr.icon_kind, 2.0 * scale, th.icon, 0.28);
            // Ngon glyphs draw with the shape's real side count.
            icon.icon[2] = lr.icon_sides;
            ui.push(icon);
        }
        if let Some(track) = &self.audio {
            let strip = timeline::strip(layout.timeline, scale);
            ui.extend(timeline::waveform_rects(&strip, scale, &track.peaks));
            ui.extend(timeline::grid_rects(
                &strip,
                scale,
                &track.beat,
                track.duration,
            ));
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
        // The rename field floats over the primary layer row.
        let rename_field = self.rename.as_ref().and_then(|_| {
            let pi = self.editor.primary()?;
            let lr = layer_rows.iter().find(|lr| lr.index == pi)?;
            Some(TextField::new(lr.row, scale))
        });
        if let (Some(field), Some(buf)) = (&rename_field, &self.rename) {
            ui.extend(field.rects(true, text.measure(buf, ui_size)));
        }
        if let Some(mi) = self.menu_open {
            // Last so the panel floats over everything beneath it.
            ui.extend(menus[mi].panel_rects(self.menu_hover));
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
            menus: &menus,
            menu_open: self.menu_open,
            view_flags: [
                self.view_black,
                self.editor.snap_grid,
                self.editor.smart_guides,
            ],
            file: &file_name,
            audio_note: audio_note.as_deref(),
            rename: self.rename.as_deref().zip(rename_field.as_ref()),
        };
        chrome::labels(text, &layout, scale, &tb, &scene, res);
        text.draw(&mut encoder, &frame.view, res);

        gpu.queue.submit([encoder.finish()]);
        frame.present();
    }
}
