//! Frame assembly: one redraw = shape pass, UI rect pass, text pass.
//! Split from main so the event plumbing stays readable.

use std::path::Path;

use spark_render::{Scene, wgpu};
use spark_ui::{ICON_DICE, IconBar, Slider, TextField, TitleBar, UiRect, theme};

use crate::props::TOOLS;
use crate::{Studio, chrome, handles, lanes, layers, menu, timeline};

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
        let canvas = self.editor.canvas();
        let cmap = self.canvas_view.map(layout.viewport, canvas);
        // Held fly keys move the eye before the camera is read.
        self.fly_tick();
        let camera = self.camera();
        let framing = self.framing(&layout);
        self.editor.set_camera(camera);
        // The editor's marks in the scene: the transform gizmo on the
        // selection, drawn over everything so it can't hide inside a
        // mesh, and whatever the view adds (see `viewpoint`), which sits
        // in the scene with depth.
        let mut over = Vec::new();
        if let Some(g) = self.gizmo(&layout) {
            over.extend(g.overlays(&camera, self.gizmo_hover));
            // Where an arrow drag is locked: the other object's edge.
            if let Some(guide) = self.gizmo_drag.as_ref().and_then(|d| d.guide()) {
                over.extend(guide.overlays(g.px(2.0)));
            }
        }
        let extra = self.view_overlays(&camera);
        let tool = self.editor.tool();
        let title_hover = self.title_hover;
        // The timeline's clock, read before the passes take their &mut
        // borrows of `self`'s fields. A comp keeps time whether or not a
        // track is loaded — see `Studio::grid`.
        let (beat, duration) = (self.grid(), self.duration());
        let playing = self.playing();
        // Half-resolution while the song runs, if asked for; the moment it
        // stops, the full picture is back.
        let preview = self.half_res_play && playing;
        // The status strip, built before the passes borrow `self`'s fields.
        // An export in progress owns the left half; what the last one
        // came to stays there until the next click.
        let status = crate::status::Status {
            left: match (&self.export, &self.export_note) {
                (Some(job), _) => job.status(),
                (None, Some(note)) => note.clone(),
                (None, None) => crate::status::selection(
                    &self
                        .editor
                        .selection()
                        .iter()
                        .map(|&i| self.editor.display_name(i))
                        .collect::<Vec<_>>(),
                ),
            },
            right: crate::status::playhead(self.editor.time(), &beat),
        };
        let (Some(gpu), Some(shape_pass), Some(stage), Some(ui_pass), Some(bg_pass), Some(text)) = (
            &mut self.gpu,
            &mut self.shape_pass,
            &mut self.stage,
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
        self.anchor_ws = menu::LABELS.map(|l| text.measure(l, chrome::MENU_TEXT * scale));
        self.menu_item_w =
            menu::all_items().fold(0.0f32, |w, s| w.max(text.measure(s, ui_size)));
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
        // Field access only: gpu and text hold `&mut` borrows of their own
        // fields, so `self` cannot be borrowed whole here.
        let chrome_target = self
            .materials_open
            .then_some(self.material_target)
            .flatten();
        let (color_vp, cards_vp) = crate::colorhome::split(
            layout.right,
            scale,
            self.picker_hsv.is_some(),
            chrome_target.is_some(),
        );
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
        let color = crate::colorhome::build_for(
            color_vp,
            scale,
            self.picker_hsv,
            chrome_target.map(|t| (t, self.material_pick)),
            (self.editor.color(), self.editor.palette_match()),
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
        // The checkerboard is the canvas's; the fly view has no canvas
        // rectangle to sit on, only the gutter behind the scene.
        let checker_ui = if self.fly.is_some() {
            Vec::new()
        } else {
            crate::view::checker_rects(cmap, layout.viewport, scale, canvas)
        };
        bg_pass.draw_batches(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &frame.view,
            &[(&bg_ui, None), (&checker_ui, Some(layout.viewport))],
            gpu.size(),
            Some(clear),
        );
        // The comp is a scene, looked at through this frame's camera. The
        // playhead goes straight through to the shaders: a star field
        // twinkles on song time, so scrubbing back lands on the same sky.
        let assembled = crate::scene::assemble(
            &self.editor,
            self.audio.as_ref(),
            &self.meshes,
            &self.subcomps,
            &camera,
            extra,
            over,
            true,
        );
        let scene = Scene {
            shapes: &assembled.shapes,
            models: &assembled.models,
            paths: &assembled.paths,
            meshes: &assembled.meshes,
            lights: &assembled.lights,
            camera: &camera,
            time: self.editor.time(),
            over: assembled.over,
        };
        // Through the stage cache: a redraw that changed nothing the passes
        // read (a hover, a menu, a card scroll) costs one blit, not a
        // re-light of every glow on the canvas.
        stage.draw(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &frame.view,
            shape_pass,
            &scene,
            gpu.size(),
            framing,
            if preview {
                spark_render::Quality::Preview
            } else {
                spark_render::Quality::Live
            },
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
        // The effects browser fills the left panel under the tool strip.
        let browser = crate::browser::build(layout.left, scale);
        ui.extend(crate::browser::rects(
            &browser,
            scale,
            self.fx_drag.or(self.fx_browser_hover),
        ));
        let zb = crate::view::zoom_bar(layout.zoom, scale);
        ui.extend(crate::view::zoom_bar_rects(&zb, scale, self.zoom_hover));
        let th = theme();
        // Panel content lives in its own scissored batches so scrolled
        // overflow clips at the panel edge instead of spilling.
        // The color home: swatches, the current-color bar (gold-ringed
        // while the picker is open), and the picker itself.
        let mut color_ui = Vec::new();
        // Whichever palette the home is offering — the neon chips for a
        // shape, the grey ladder while the chrome is being painted.
        color_ui.extend(color.swatches.rects(&color.chips, color.palette));
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
        // The dice: a plate like the transport toggles, gold on the purple
        // highlight while armed — the same lit look as the active tool.
        if let Some(d) = color.dice {
            let armed = self.editor.random;
            color_ui.push(if armed {
                UiRect::region_rounded(d, th.accent_alt_bg, 8.0 * scale)
            } else {
                spark_ui::surfaces().plate.rect(d, scale)
            });
            let fg = if armed { th.accent } else { th.icon };
            color_ui.push(UiRect::icon_sized(d, ICON_DICE, 0.0, fg, 0.40));
        }
        if let Some((p, [h, s, v], _)) = &color.picker {
            color_ui.extend(p.rects(*h, *s, *v, scale));
        }
        // Opacity, where opacity means something. The track is drawn over a
        // light-to-dark ramp so the thumb's position reads as *how much
        // shows through* rather than as an unlabelled number.
        if let Some((track, a)) = color.alpha {
            let [light, dark] = th.checker;
            color_ui.push(UiRect::region_rounded(track, dark, track.h * 0.5).gradient_h(light));
            color_ui.extend(Slider::rects(track, a));
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
        let editing = self.field_edit.as_ref().and_then(|(t, p, _)| match *t {
            crate::ScrubTarget::Shape => self
                .editor
                .primary()
                .map(|i| layers::EditField::Shape(i, *p)),
            crate::ScrubTarget::Folder(id) => Some(layers::EditField::Folder(id, *p)),
        });
        let mut layers_ui =
            layers::rects(&cards, scale, self.grad_edit_b, self.card_hover, editing);
        // The caret and its selection, measured against the same face the
        // value is drawn in — the text engine is right here, so the rects
        // can be exact rather than estimated.
        if let (Some((_, _, tb)), Some(f)) = (
            &self.field_edit,
            editing.and_then(|e| cards.focused_field(e)),
        ) {
            let card_size = layers::CARD_TEXT * scale;
            // Right-aligned, so the text's origin moves as it's typed —
            // the boundary table has to start from where it actually sits.
            let x0 = f.rect.x + f.rect.w
                - layers::FIELD_PAD * scale
                - text.measure(tb.text(), card_size);
            self.field_caret_xs =
                crate::textbox::boundaries(tb.text(), x0, |s| text.measure(s, card_size));
            layers_ui.extend(crate::textbox::caret_rects(
                &self.field_caret_xs,
                f.rect,
                tb,
                spark_text::Text::line_height(card_size),
            ));
        }
        // The timeline is unconditional — a comp without a track keeps its
        // own clock (see `Studio::grid`), so lanes, ruler and playhead exist
        // from the first shape you draw. Only the waveform and playback
        // actually need a song.
        let mut lanes_ui = Vec::new();
        // Tab content clipped to the time axis: key markers in Keys, the
        // waveform in Wave, clip bars in Arrange.
        let mut axis_ui = Vec::new();
        let mut lane_rows = Vec::new();
        let mut arrange_scene: Option<crate::arrange::ArrangeScene> = None;
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
        // ...but only one thing at a time may *own* the bottom panel. The
        // playground takes it over whole while it's open: its grid paints
        // controls, not a background, so a timeline left drawing underneath
        // showed straight through it — bar shading behind the swatches, the
        // ruler's numbers behind Print and Reset. A panel you can see two
        // screens through is not a panel.
        let axis_shown = !self.materials_open;
        // The axis backdrop (alternating bars) goes under everything on the
        // time axis; ruler and control column sit beside it.
        if axis_shown {
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
        }
        // Lane batch: row furniture clipped to the lanes region; key markers
        // clipped to the axis so nothing pokes into the sidebar; the playhead
        // rules over everything on the time axis.
        if !axis_shown {
            // The playground has the panel; nothing on the axis draws.
        } else if self.timeline_tab == timeline::Tab::Keys {
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
        } else if self.timeline_tab == timeline::Tab::Arrange {
            let content = crate::arrange::content_height(&self.editor, scale);
            self.lanes_scroll = self.lanes_scroll.min((content - lanes_area.h).max(0.0));
            let sc = crate::arrange::build(
                &panel,
                &view,
                scale,
                &self.editor,
                &self.subcomps,
                self.selected_clip,
                self.lanes_scroll,
            );
            let (lu, au) = crate::arrange::rects(&sc, scale);
            lanes_ui = lu;
            axis_ui = au;
            arrange_scene = Some(sc);
        } else if self.timeline_tab == timeline::Tab::Wave
            && let Some(track) = &self.audio
        {
            // The one tab that genuinely needs a song.
            axis_ui = timeline::wave_rects(&panel, &view, scale, track);
        }
        // The playhead follows the editor's clock, which the player drives
        // when there is one — so it draws with or without audio.
        let playhead =
            axis_shown.then(|| timeline::playhead_rect(&panel, &view, scale, self.editor.time()));
        let playhead = playhead.flatten();
        let axis_clip = spark_render::Viewport {
            x: panel.axis.0,
            y: panel.axis_y.0,
            w: panel.axis.1,
            h: (panel.axis_y.1 - panel.axis_y.0).max(1.0),
        };
        let tl_scene = chrome::TlScene {
            // No marks while the playground owns the panel — text is drawn
            // in its own pass, so hiding the ruler's rects would otherwise
            // leave its bar numbers floating over the colour grid.
            marks: match axis_shown {
                true => timeline::ruler_marks(&panel, &view, scale, &beat, duration),
                false => Vec::new(),
            },
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
        // The 2D rig is drawn on the canvas plane's map: in the fly view
        // the gizmo does its job.
        let handles_ui = if self.fly.is_some() {
            Vec::new()
        } else {
            handles::build(&self.editor, cmap, scale)
                .map(|h| h.rects(scale))
                .unwrap_or_default()
        };
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
        // The card a dragged effect would land on, outlined so the drop
        // isn't a guess.
        if let Some(i) = self.fx_drop
            && let Some(lr) = cards.rows.iter().find(|lr| lr.index == i)
        {
            overlay_ui.push(crate::browser::drop_rect(lr.row, scale));
        }
        // The open menu is its *own* batch, drawn after the base text.
        // Floating it in the same pass as everything else only floated its
        // rects: text is a separate pass with no z-order against rects, so
        // every label in the editor printed back through the panel.
        let mut menu_ui = Vec::new();
        if let Some(mi) = self.menu_open {
            menu_ui.extend(menus[mi].panel_rects(self.menu_hover));
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
            arrange: arrange_scene.as_ref(),
            timeline: &tl_scene,
            browser: &browser,
            menus: &menus,
            menu_open: self.menu_open,
            canvas_pick: menu::preset_index(canvas),
            view_flags: [
                self.view_black,
                self.editor.snap_grid,
                self.editor.smart_guides,
                self.cursor_choice == Some(0),
                self.cursor_choice == Some(1),
                self.materials_open,
                self.half_res_play,
                self.fly.is_some(),
                self.floor,
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

        // -- overlay layer -------------------------------------------------
        // Everything above is one full stack: rects, then the words that go
        // on them. A floating panel has to be a *second* such stack, or it
        // can only ever cover the rects of what it floats over and never the
        // words — which is exactly what an open File menu did to the layer
        // browser underneath it.
        //
        // The frame so far is **submitted first**, and that is not tidiness:
        // one `UiPass` owns one instance buffer and every `draw_batches`
        // call rewrites it from the start, so queueing both into a single
        // encoder lands both buffer writes before either render pass runs
        // and *both* passes draw the overlay's rects. The first attempt at
        // this replaced the entire editor with the menu; a readback test in
        // `spark_ui` now holds that line.
        gpu.queue.submit([encoder.finish()]);
        if !menu_ui.is_empty() {
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            ui_pass.draw_batches(
                &gpu.device,
                &gpu.queue,
                &mut encoder,
                &frame.view,
                &[(&menu_ui, None)],
                gpu.size(),
                None,
            );
            chrome::menu_labels(text, scale, &scene, res);
            text.draw(&mut encoder, &frame.view, res);
            gpu.queue.submit([encoder.finish()]);
        }
        frame.present();
    }
}
