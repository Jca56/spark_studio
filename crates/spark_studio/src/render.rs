//! Frame assembly: one redraw = shape pass, UI rect pass, text pass.
//! Split from main so the event plumbing stays readable.

use std::path::Path;

use spark_render::{CANVAS_H, CANVAS_W, Shape, wgpu};
use spark_ui::{IconBar, Slider, TextField, TitleBar, UiRect, theme};

use crate::props::TOOLS;
use crate::{Studio, chrome, editor, handles, lanes, layers, menu, timeline};

impl Studio {
    pub(crate) fn redraw(&mut self) {
        let Some(layout) = self.layout() else { return };
        // Pose the document at the playhead before anything reads it — the
        // frame is a pure function of (document, t). The audio cursor is the
        // clock whenever there's a stream; without one (no output device)
        // the editor's own time stands, so scrubbing and keying still work.
        if let Some(p) = &self.player {
            self.editor.set_time(p.time());
        } else {
            // No track: the transport runs on wall time instead.
            self.advance_clock();
        }
        self.editor.sync_to_time();
        let scale = self.scale();
        let cmap = self.canvas_view.map(layout.viewport, scale);
        let tool = self.editor.tool();
        let title_hover = self.title_hover;
        // The timeline's clock, read before the passes take their &mut
        // borrows of `self`'s fields. A comp keeps time whether or not a
        // track is loaded — see `Studio::grid`.
        let (beat, duration) = (self.grid(), self.duration());
        let playing = self.playing();
        // The status strip, built before the passes borrow `self`'s fields.
        let status = crate::status::Status {
            left: crate::status::selection(
                &self
                    .editor
                    .selection()
                    .iter()
                    .map(|&i| self.editor.display_name(i))
                    .collect::<Vec<_>>(),
            ),
            right: crate::status::playhead(self.editor.time(), &beat),
        };
        let (Some(gpu), Some(shape_pass), Some(ui_pass), Some(bg_pass), Some(text)) = (
            &mut self.gpu,
            &mut self.shape_pass,
            &mut self.ui_pass,
            &mut self.bg_pass,
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
        // Right panel: color home pinned on top, layer cards below. Field
        // access only — gpu/text hold &mut borrows of their own fields.
        if self
            .card_open
            .is_some_and(|i| i >= self.editor.shapes().len())
        {
            self.card_open = None;
        }
        let (color_vp, cards_vp) =
            crate::colorhome::split(layout.right, scale, self.picker_hsv.is_some());
        let mut cards = layers::rows(
            cards_vp,
            scale,
            &self.editor,
            self.card_open,
            self.card_tab,
            self.layers_scroll,
        );
        let max_scroll = (cards.content_h - cards_vp.h).max(0.0);
        if self.layers_scroll > max_scroll {
            // Clamp the scroll to the content and lay out again.
            self.layers_scroll = max_scroll;
            cards = layers::rows(
                cards_vp,
                scale,
                &self.editor,
                self.card_open,
                self.card_tab,
                self.layers_scroll,
            );
        }
        let color = crate::colorhome::build(
            color_vp,
            scale,
            self.editor.color(),
            self.editor.palette_match(),
            self.picker_hsv,
        );
        let Some(frame) = gpu.begin_frame() else {
            return;
        };
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        // Base coat, its own pass: near-black void behind every surface,
        // the viewport gutter in deep purple (View > Black flips it), and
        // the transparency checkerboard under the stage. The document
        // itself has no background shape anymore — transparency is real,
        // and export can render straight to alpha.
        let void = theme().void;
        let clear = wgpu::Color {
            r: void[0] as f64,
            g: void[1] as f64,
            b: void[2] as f64,
            a: 1.0,
        };
        let gutter = if self.view_black {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            theme().gutter
        };
        let bg_ui = vec![UiRect::region(layout.viewport, gutter)];
        let checker_ui = crate::view::checker_rects(cmap, layout.viewport, scale);
        bg_pass.draw_batches(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &frame.view,
            &[(&bg_ui, None), (&checker_ui, Some(layout.viewport))],
            gpu.size(),
            Some(clear),
        );
        let mut shapes = Vec::new();
        let mut overlay_n = 0;
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
        if let Some(track) = &self.audio {
            // Render-time audio reaction: the document never changes, the
            // copies drawn this frame just ride the analysis curves.
            //
            // Sampled at the playhead, not at a running player's clock. It
            // used to be gated on `is_playing()`, so parking on the drop to
            // tune a React amount showed you a shape with no reaction on it
            // — and a paused frame differed from the same frame in motion,
            // which `frame = render(project, t)` says can never happen.
            let t = self.editor.time();
            let c = &track.curves;
            let bass = spark_audio::Curves::sample(&c.bass, c.rate, t);
            let mid = spark_audio::Curves::sample(&c.mid, c.rate, t);
            let onset = spark_audio::Curves::sample(&c.onset, c.rate, t);
            // Skip the stage background and grid overlay. Bass moves size
            // and glow (kick/sub weight); mids carry the wobble into
            // brightness; onsets snap — each scaled by the shape's own
            // React amounts, so shapes ride the track as hard as they like.
            let n = (overlay_n + self.editor.shapes().len()).min(shapes.len());
            for (k, s) in shapes[overlay_n..n].iter_mut().enumerate() {
                let r = self.editor.react(k);
                s.add_glow(bass * 40.0 * r[1]);
                s.add_intensity((bass * 0.3 + mid * 0.45 + onset * 0.25) * r[2]);
                s.scale_by(1.0 + bass * 0.05 * r[0]);
            }
        }
        // Flatten path vertex lists into this frame's pool, repointing each
        // display copy at its slice. The bound ratio carries any render-time
        // scaling (wub) onto the vertices themselves.
        let mut path_pool: Vec<[f32; 2]> = Vec::new();
        for s in &mut shapes {
            if let Some((id, _, _)) = s.path_meta() {
                let vs = self.editor.path(id);
                let vb = vs
                    .iter()
                    .map(|v| (v[0] * v[0] + v[1] * v[1]).sqrt())
                    .fold(1.0f32, f32::max);
                let f = s.size() / vb.max(0.001);
                let start = path_pool.len();
                path_pool.extend(vs.iter().map(|v| [v[0] * f, v[1] * f]));
                s.set_path_start(start);
            }
        }
        shape_pass.draw(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &frame.view,
            &shapes,
            &path_pool,
            gpu.size(),
            cmap,
            // The playhead, straight through to the shaders: a star field
            // twinkles on song time, so scrubbing back lands on the same sky.
            self.editor.time(),
            layout.viewport,
        );
        let mut ui = layout.panel_rects(scale);
        ui.extend(tb.rects(title_hover));
        // The title bar's gold underline — after the title bar's own
        // background, which would otherwise paint over it.
        let seam = (3.0 * scale).max(1.0);
        ui.push(UiRect::region(
            spark_render::Viewport {
                x: layout.title.x,
                y: layout.title.y + layout.title.h - seam,
                w: layout.title.w,
                h: seam,
            },
            theme().seam,
        ));
        for (mi, m) in menus.iter().enumerate() {
            ui.extend(m.anchor_rects(
                self.menu_open == Some(mi),
                self.menu_anchor_hover == Some(mi),
            ));
        }
        ui.extend(IconBar::new(layout.tools, scale, &TOOLS).rects(self.tool_hover, Some(tool)));
        let zb = crate::view::zoom_bar(layout.zoom, scale);
        ui.extend(crate::view::zoom_bar_rects(&zb, scale, self.zoom_hover));
        let th = theme();
        // Panel content lives in its own scissored batches so scrolled
        // overflow clips at the panel edge instead of spilling.
        // The color home: swatches, the current-color bar (gold-ringed
        // while the picker is open), and the picker itself.
        let mut color_ui = Vec::new();
        color_ui.extend(color.swatches.rects(&editor::PALETTE, color.palette));
        let custom = UiRect::region_rounded(
            color.custom,
            [
                color.custom_rgb[0],
                color.custom_rgb[1],
                color.custom_rgb[2],
                1.0,
            ],
            8.0 * scale,
        );
        // Picker open: a gold ring around the current color, outside the bar
        // so none of the color it's reporting gets covered up.
        color_ui.push(if color.picker.is_some() {
            custom.stroke_outer(3.0 * scale, th.accent)
        } else {
            custom
        });
        if let Some((p, [h, s, v], _)) = &color.picker {
            color_ui.extend(p.rects(*h, *s, *v, scale));
        }
        // A soft separator under the pinned color home.
        color_ui.push(UiRect::region(
            spark_render::Viewport {
                x: color_vp.x,
                y: color_vp.y + color_vp.h - 1.5 * scale,
                w: color_vp.w,
                h: 1.5 * scale,
            },
            [1.0, 1.0, 1.0, 0.10],
        ));
        let editing = self.field_edit.as_ref().and_then(|(t, p, _)| match t {
            crate::ScrubTarget::Shape => self.editor.primary().map(|i| (i, *p)),
            // Folder fields ring gold via the folder strip, not a card.
            crate::ScrubTarget::Folder(_) => None,
        });
        let mut layers_ui =
            layers::rects(&cards, scale, self.grad_edit_b, self.card_hover, editing);
        // The caret and its selection, measured against the same face the
        // value is drawn in — the text engine is right here, so the rects
        // can be exact rather than estimated.
        if let (Some((_, prop, tb)), Some(i)) = (&self.field_edit, editing.map(|(i, _)| i))
            && let Some(f) = cards
                .rows
                .iter()
                .find(|lr| lr.index == i)
                .and_then(|lr| lr.scrubs.iter().find(|f| f.prop == *prop))
        {
            let card_size = layers::CARD_TEXT * scale;
            layers_ui.extend(crate::textbox::caret_rects(
                f.rect,
                tb,
                layers::FIELD_PAD * scale,
                spark_text::Text::line_height(card_size),
                |s| text.measure(s, card_size),
            ));
        }
        // The timeline is unconditional — a comp without a track keeps its
        // own clock (see `Studio::grid`), so lanes, ruler and playhead exist
        // from the first shape you draw. Only the waveform and playback
        // actually need a song.
        let mut lanes_ui = Vec::new();
        // Tab content clipped to the time axis: key markers in Keys, the
        // waveform in Wave.
        let mut axis_ui = Vec::new();
        let mut lane_rows = Vec::new();
        let mut react_rows: Vec<lanes::ReactRow> = Vec::new();
        let panel = timeline::panel(layout.timeline, scale);
        let view = self.time_view;
        let lanes_area = panel.lanes;
        let controls = timeline::controls(layout.toolbar, scale, self.timeline_tab);
        // While it's being typed into, the field shows the buffer, so an
        // empty one reads empty rather than as the number you're replacing.
        let bpm_scene = (
            controls.bpm,
            match &self.bpm_edit {
                Some(buf) => buf.clone(),
                None => format!("{:.0}", beat.bpm),
            },
            self.bpm_edit.is_some(),
        );
        ui.extend(timeline::toolbar_rects(
            &controls,
            scale,
            playing,
            self.transport_hover,
            self.timeline_tab,
            self.snap_playhead,
            self.bpm_edit.is_some(),
        ));
        // The axis backdrop (alternating bars) goes under everything on the
        // time axis; ruler and control column sit beside it.
        ui.extend(timeline::shade_rects(&panel, &view, scale, &beat, duration));
        ui.extend(timeline::ruler_rects(&panel, &view, scale, &beat, duration));
        if let Some(region) = self.loop_region {
            ui.extend(timeline::loop_rects(
                &panel,
                &view,
                scale,
                region,
                self.loop_on,
            ));
        }
        ui.extend(timeline::sidebar_rects(
            &panel,
            scale,
            self.timeline_tab,
            self.key_hover,
        ));
        // Lane batch: row furniture clipped to the lanes region; key markers
        // clipped to the axis so nothing pokes into the sidebar; the playhead
        // rules over everything on the time axis.
        if self.timeline_tab == timeline::Tab::Keys {
            let content = lanes::content_height(&self.editor, self.lane_open, scale);
            self.lanes_scroll = self.lanes_scroll.min((content - lanes_area.h).max(0.0));
            lane_rows = lanes::rows(
                &panel,
                &view,
                scale,
                &self.editor,
                self.lane_open,
                self.lanes_scroll,
            );
            lanes_ui = lanes::rects(&lane_rows, &panel, scale);
            axis_ui = lanes::key_rects(&lane_rows, &panel, scale, &self.selected_keys);
            // React sliders now live inside the expanded lane, so chrome
            // draws their labels from the rows themselves.
            for lr in &lane_rows {
                for r in &lr.detail {
                    lanes_ui.extend(Slider::rects(r.track, r.t));
                    react_rows.push(lanes::ReactRow { ..r.clone() });
                }
            }
        } else if self.timeline_tab == timeline::Tab::Wave
            && let Some(track) = &self.audio
        {
            // The one tab that genuinely needs a song.
            axis_ui = timeline::wave_rects(&panel, &view, scale, track);
        }
        // The playhead follows the editor's clock, which the player drives
        // when there is one — so it draws with or without audio.
        let playhead = timeline::playhead_rect(&panel, &view, scale, self.editor.time());
        let axis_clip = spark_render::Viewport {
            x: panel.axis.0,
            y: panel.axis_y.0,
            w: panel.axis.1,
            h: (panel.axis_y.1 - panel.axis_y.0).max(1.0),
        };
        let tl_scene = chrome::TlScene {
            marks: timeline::ruler_marks(&panel, &view, scale, &beat, duration),
            ruler: panel.ruler,
        };
        // The playground owns the bottom panel while it's open — the one
        // region with enough width for a colour grid, and already
        // user-resizable by dragging its top edge.
        let mut materials_ui = Vec::new();
        let mut materials_panel = None;
        if self.materials_open {
            // Field access only: gpu and text hold &mut borrows of their
            // own fields, so `self` can't be borrowed whole here.
            let st = crate::materials::State {
                tab: self.material_tab,
                pick: self.material_pick,
                editing: self.material_edit.clone(),
            };
            let panel = crate::materials::build(layout.timeline, scale, &st);
            materials_ui = crate::materials::rects(&panel, scale, self.material_pick);
            materials_panel = Some(panel);
        }
        // Transform handles clip to the viewport — a big shape's rig must
        // not paint over the side panels.
        let handles_ui = handles::build(&self.editor, cmap, scale)
            .map(|h| h.rects(scale))
            .unwrap_or_default();
        // The rename field floats over the primary layer row.
        let rename_field = self.rename.as_ref().and_then(|_| {
            if let Some(id) = self.rename_folder {
                let f = cards.folders.iter().find(|f| f.id == id)?;
                return Some(TextField::new(f.row, scale));
            }
            let pi = self.editor.primary()?;
            let lr = cards.rows.iter().find(|lr| lr.index == pi)?;
            Some(TextField::new(lr.head, scale))
        });
        let mut rename_ui = Vec::new();
        if let (Some(field), Some(buf)) = (&rename_field, &self.rename) {
            rename_ui = field.rects(true, text.measure(buf, ui_size));
        }
        let mut overlay_ui = Vec::new();
        if let Some(r) = playhead {
            overlay_ui.push(r);
        }
        if let Some(b) = &self.box_sel
            && b.moved
        {
            // The rubber band, floating over the lanes — gold, like the
            // keys it's about to catch.
            overlay_ui.push(UiRect::region_rounded(
                spark_render::Viewport {
                    x: b.x0.min(b.x1),
                    y: b.y0.min(b.y1),
                    w: (b.x1 - b.x0).abs().max(1.0),
                    h: (b.y1 - b.y0).abs().max(1.0),
                },
                [th.accent[0], th.accent[1], th.accent[2], 0.14],
                4.0 * scale,
            ));
        }
        if let Some(mi) = self.menu_open {
            // Last so the panel floats over everything beneath it.
            overlay_ui.extend(menus[mi].panel_rects(self.menu_hover));
        }
        ui_pass.draw_batches(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &frame.view,
            &[
                (&ui, None),
                (&handles_ui, Some(layout.viewport)),
                (&color_ui, None),
                (&layers_ui, Some(cards_vp)),
                (&rename_ui, Some(cards_vp)),
                (&materials_ui, Some(layout.timeline)),
                (&lanes_ui, Some(lanes_area)),
                (&axis_ui, Some(axis_clip)),
                (&overlay_ui, None),
            ],
            gpu.size(),
            None,
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
            color: &color,
            cards: cards_vp,
            renaming: self.rename.as_ref().and_then(|_| self.editor.primary()),
            editing,
            edit_buf: self.field_edit.as_ref().map(|(_, _, b)| b.text()),
            react: &react_rows,
            layers: &cards.rows,
            folders: &cards.folders,
            renaming_folder: self
                .rename
                .is_some()
                .then_some(self.rename_folder)
                .flatten(),
            lanes: &lane_rows,
            timeline: &tl_scene,
            menus: &menus,
            menu_open: self.menu_open,
            view_flags: [
                self.view_black,
                self.editor.snap_grid,
                self.editor.smart_guides,
                self.cursor_choice == Some(0),
                self.cursor_choice == Some(1),
                self.materials_open,
            ],
            materials: materials_panel.as_ref(),
            zoom_pct: self.canvas_view.pct(),
            file: &file_name,
            audio_note: audio_note.as_deref(),
            rename: self.rename.as_deref().zip(rename_field.as_ref()),
            bpm: bpm_scene,
        };
        chrome::labels(text, &layout, scale, &tb, &scene, res);
        crate::status::labels(text, layout.status, scale, &status, res);
        text.draw(&mut encoder, &frame.view, res);

        gpu.queue.submit([encoder.finish()]);
        frame.present();
    }
}
