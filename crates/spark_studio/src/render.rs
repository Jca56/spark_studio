//! Frame assembly: one redraw = shape pass, UI rect pass, text pass.
//! Split from main so the event plumbing stays readable.

use spark_render::{Scene, wgpu};
use spark_ui::{TitleBar, UiRect, theme};

use crate::{Studio, chrome, handles, menu, timeline};

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
        // Read before the passes take their &mut borrows: whether the
        // document differs from its last save (the title's star).
        let dirty_mark = if self.is_dirty() { "*" } else { "" };
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
        let title_hover = self.title_hover;
        // The timeline's clock, read before the passes take their &mut
        // borrows of `self`'s fields. A comp keeps time whether or not a
        // track is loaded — see `Studio::grid`.
        let (beat, duration) = (self.grid(), self.duration());
        let playing = self.playing();
        // The context menu, if it's up: its rects and words, built from
        // the same inputs its hit tests use — before the passes take
        // their borrows of `self`.
        let ctx_frame = self.context_frame();
        // The inspector: its picker follows the colour, its scroll stays
        // inside its content, then its rects and words — before the
        // passes take their borrows of `self`. The caret is added below,
        // once the text engine can measure the field.
        self.inspector_tick(layout.right);
        // The clip view, while the bottom panel is one: its clip resolved
        // fresh, its window clamped, then its rects and words — before
        // the passes take their borrows of `self`.
        let panel = timeline::panel(layout.timeline, scale);
        self.clip_view_tick(&panel, scale);
        let clip_frame = self.clip_view_frame(&panel, scale);
        let crate::inspector::Frame {
            pinned: insp_pinned,
            body: mut insp_body,
            body_clip: insp_clip,
            labels: insp_labels,
            edit_box: insp_edit,
            popup: insp_popup,
        } = self.inspector_frame(layout.right);
        // The left panel: its tab strip and page, and the ghost of an
        // effect row being dragged, which floats with the popup.
        let crate::left::Frame {
            rects: left_ui,
            labels: left_labels,
            ghost: left_ghost,
        } = self.left_frame(layout.left);
        // Half-resolution while the song runs, if asked for; the moment it
        // stops, the full picture is back.
        let preview = self.half_res_play && playing;
        // The status strip, built before the passes borrow `self`'s fields.
        // An export in progress owns the left half; what the last one came
        // to stays there until the next click. The center is the project's
        // name (`project > comp` inside one — clicking it is Back),
        // starred while unsaved — moved down from the title bar, which
        // keeps only the menus and the wordmark.
        let base_name = |p: &str| {
            std::path::Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.to_string())
        };
        let file_name = match self.comp_stack.last() {
            Some(c) => format!(
                "{} > {}{dirty_mark}",
                base_name(&c.file),
                base_name(&self.current_file)
            ),
            None => format!("{}{dirty_mark}", base_name(&self.current_file)),
        };
        let status = crate::status::Status {
            left: match (&self.export, &self.export_note) {
                (Some(job), _) => job.status(),
                (None, Some(note)) => note.clone(),
                (None, None) => self.clip_view_status().unwrap_or_else(|| {
                    crate::status::selection(
                        &self
                            .editor
                            .selection()
                            .iter()
                            .map(|&i| self.editor.display_name(i))
                            .collect::<Vec<_>>(),
                    )
                }),
            },
            center: file_name,
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
        self.menu_item_w = menu::all_items().fold(0.0f32, |w, s| w.max(text.measure(s, ui_size)));
        // A field being typed into: its selection wash and caret, from
        // the boundary table the text engine measures — cached for the
        // next click to place the caret by.
        let (mut popup_ui, mut popup_labels, popup_edit) = match insp_popup {
            Some((r, l, e)) => (r, l, e),
            None => (Vec::new(), Vec::new(), None),
        };
        if let Some((g_rects, g_labels)) = left_ghost {
            popup_ui.extend(g_rects);
            popup_labels.extend(g_labels);
        }
        for (edit, rects) in [(insp_edit, &mut insp_body), (popup_edit, &mut popup_ui)] {
            if let Some((rect, x0, size)) = edit
                && let Some((_, tb)) = &self.inspector.edit
            {
                let xs = crate::textbox::boundaries(tb.text(), x0, |s| text.measure(s, size));
                rects.extend(crate::textbox::caret_rects(
                    &xs,
                    rect,
                    tb,
                    spark_text::Text::line_height(size),
                ));
                self.inspector.caret_xs = xs;
            }
        }
        let tb = TitleBar::new(layout.title, scale, wordmark_w);
        let menus = menu::build(&layout, scale, self.anchor_ws, self.menu_item_w);
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
        // The side panels are empty shells while the redesign lands: their
        // surfaces and seams come from `panel_rects`, and nothing draws in
        // them.
        // The timeline is unconditional — a comp without a track keeps its
        // own clock (see `Studio::grid`), so tracks, ruler and playhead
        // exist from the first object you draw. Only the waveform and
        // playback actually need a song.
        // The axis draws in song time on the arrangement and in clip-local
        // time on the clip view — the same painters, the view's own clock.
        let (view, grid, span) = match &clip_frame {
            Some(cf) => (cf.view, cf.grid, cf.span),
            None => (self.time_view, beat, duration),
        };
        let lanes_area = panel.lanes;
        let controls = timeline::controls(layout.toolbar, scale);
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
            self.snap_playhead,
            self.wave_overlay,
            self.bpm_edit.is_some(),
            self.zoom_hover,
        ));
        // The axis backdrop (alternating bars) goes under everything on the
        // time axis; ruler and control column sit beside it.
        ui.extend(timeline::shade_rects(&panel, &view, scale, &grid, span, self.grid_div));
        // The waveform overlay: the song laid faintly across the whole
        // grid, under the clips — in the clip view, mapped through the
        // clip into local time.
        if self.wave_overlay && let Some(track) = &self.audio {
            let time_at: Box<dyn Fn(f32) -> f32> = match &clip_frame {
                Some(cf) => {
                    let (clip, cv) = (cf.clip.clone(), cf.view);
                    Box::new(move |x| crate::clipview::song_time_for(&clip, cv.t_at(x, panel.axis)))
                }
                None => Box::new(move |x| view.t_at(x, panel.axis)),
            };
            ui.extend(timeline::wave_rects(&panel, panel.axis_y, scale, track, 0.16, &*time_at));
        }
        ui.extend(timeline::ruler_rects(&panel, &view, scale, &grid, span));
        // The brace on the ruler: the transport loop, or — in the clip
        // view — the clip's own loop (lit) or its trimmed span (dimmed).
        match &clip_frame {
            Some(cf) => {
                ui.extend(timeline::loop_rects(
                    &panel,
                    &view,
                    scale,
                    cf.brace.0,
                    cf.brace.1,
                ));
                ui.extend(cf.ruler.iter().copied());
            }
            None => {
                if let Some(region) = self.loop_region {
                    ui.extend(timeline::loop_rects(
                        &panel,
                        &view,
                        scale,
                        region,
                        self.loop_on,
                    ));
                }
            }
        }
        ui.extend(timeline::sidebar_rects(&panel, scale, self.key_hover));
        // The arrangement: track rows clipped to the lanes region, clip
        // bars and the waveform clipped to the axis, the playhead ruling
        // over everything on it.
        // Either the arrangement or the clip view fills the lanes and the
        // axis this frame; the other is empty.
        let mut arrange_scene = crate::arrange::ArrangeScene {
            rows: Vec::new(),
            clips: Vec::new(),
            wave_band: None,
            dragged: None,
            drop_y: None,
        };
        let mut rows_ui = Vec::new();
        let mut rows_clip = lanes_area;
        let cv_labels;
        let (lanes_ui, axis_ui, marks, playhead) = match clip_frame {
            Some(cf) => {
                rows_ui = cf.rows;
                rows_clip = cf.rows_clip;
                cv_labels = cf.labels;
                (
                    cf.sidebar,
                    cf.axis,
                    cf.marks,
                    // The playhead at its clip-local position, while the
                    // song is inside the clip.
                    cf.playhead
                        .and_then(|t| timeline::playhead_rect(&panel, &view, scale, t)),
                )
            }
            None => {
                cv_labels = Vec::new();
                let content = crate::arrange::content_height(
                    &self.editor,
                    self.audio_file.is_some(),
                    scale,
                );
                // A newcomer lands at the bottom of the list: the list
                // scrolls to show it (a fresh document doesn't jump).
                let n_rows = crate::arrange::row_count(&self.editor, self.audio_file.is_some());
                if n_rows > self.rows_seen && self.rows_seen > 0 {
                    self.lanes_scroll = f32::MAX;
                }
                self.rows_seen = n_rows;
                self.lanes_scroll = self.lanes_scroll.min((content - lanes_area.h).max(0.0));
                // Field access only: gpu and text hold `&mut` borrows of
                // their own fields, so `self` can't be borrowed whole here
                // — the free function is the same one `arrange_scene` wraps.
                let audio_name = self.audio_file.as_ref().map(|p| {
                    std::path::Path::new(p)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| p.clone())
                });
                // Field access only past here (gpu and text are borrowed),
                // so the drag view is computed with the same free helpers
                // `arrange_scene` uses.
                let drag_view = self.row_drag.filter(|d| d.moved).map(|d| {
                    crate::arrange::RowDragView {
                        kind: d.kind,
                        dy: d.dy,
                        slot: crate::arrange::drop_slot(
                            &panel,
                            scale,
                            self.lanes_scroll,
                            self.cursor_px.1 as f32,
                            crate::arrange::object_rows(&self.editor).len(),
                            crate::arrange::head_rows(self.audio_file.is_some()),
                        ),
                    }
                });
                arrange_scene = crate::arrange::build(
                    &panel,
                    &view,
                    scale,
                    &self.editor,
                    &self.subcomps,
                    self.selected_clip,
                    self.lanes_scroll,
                    audio_name.as_deref(),
                    drag_view,
                );
                let (lanes_ui, mut axis_ui) = crate::arrange::rects(&arrange_scene, scale);
                if let (Some(band), Some(track)) = (arrange_scene.wave_band, &self.audio) {
                    axis_ui.extend(timeline::wave_rects(&panel, band, scale, track, 1.0, &|x| {
                        view.t_at(x, panel.axis)
                    }));
                }
                (
                    lanes_ui,
                    axis_ui,
                    timeline::ruler_marks(&panel, &view, scale, &beat, duration),
                    // The playhead follows the editor's clock, which the
                    // player drives when there is one — so it draws with
                    // or without audio.
                    timeline::playhead_rect(&panel, &view, scale, self.editor.time()),
                )
            }
        };
        let axis_clip = spark_render::Viewport {
            x: panel.axis.0,
            y: panel.axis_y.0,
            w: panel.axis.1,
            h: (panel.axis_y.1 - panel.axis_y.0).max(1.0),
        };
        let tl_scene = chrome::TlScene {
            marks,
            ruler: panel.ruler,
        };
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
        let mut overlay_ui = Vec::new();
        if let Some(r) = playhead {
            overlay_ui.push(r);
        }
        // The open menu is its *own* batch, drawn after the base text.
        // Floating it in the same pass as everything else only floated its
        // rects: text is a separate pass with no z-order against rects, so
        // every label in the editor printed back through the panel.
        let mut menu_ui = Vec::new();
        if let Some(mi) = self.menu_open {
            menu_ui.extend(menus[mi].panel_rects(self.menu_hover));
        }
        // The context menu floats too, so it rides the same overlay
        // submit — rects here, its words through `chrome::context_labels`.
        let (ctx_ui, ctx_scene) = match ctx_frame {
            Some((mut rects, labels, edit)) => {
                // The value box's selection wash and caret, measured the
                // way the inspector's are.
                if let Some((rect, x0, size)) = edit
                    && let Some(tb) = &self.ctx_edit
                {
                    let xs = crate::textbox::boundaries(tb.text(), x0, |s| text.measure(s, size));
                    rects.extend(crate::textbox::caret_rects(
                        &xs,
                        rect,
                        tb,
                        spark_text::Text::line_height(size),
                    ));
                }
                (rects, Some(chrome::CtxScene { labels }))
            }
            None => (Vec::new(), None),
        };
        ui_pass.draw_batches(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &frame.view,
            &[
                (&ui, None),
                // The inspector: its colour home clipped to the panel,
                // its scrolling body to the window under the home.
                (&insp_pinned, Some(layout.right)),
                (&insp_body, Some(insp_clip)),
                (&left_ui, Some(layout.left)),
                (&handles_ui, Some(layout.viewport)),
                (&lanes_ui, Some(lanes_area)),
                (&rows_ui, Some(rows_clip)),
                (&axis_ui, Some(axis_clip)),
                (&overlay_ui, None),
            ],
            gpu.size(),
            None,
        );

        // Labels — lntrn-text's first flight outside Lantern.
        let res = gpu.size();
        let audio_note = self
            .audio_loading
            .as_ref()
            .map(|name| format!("Analyzing {name}..."));
        let scene = chrome::Scene {
            arrange: &arrange_scene,
            timeline: &tl_scene,
            menus: &menus,
            menu_open: self.menu_open,
            ctx: ctx_scene,
            inspector: &insp_labels,
            left: &left_labels,
            clipview: &cv_labels,
            canvas_pick: menu::preset_index(canvas),
            view_flags: [
                self.view_black,
                self.editor.snap_grid,
                self.editor.smart_guides,
                self.cursor_choice == Some(0),
                self.cursor_choice == Some(1),
                self.half_res_play,
                self.fly.is_some(),
                self.floor,
            ],
            zoom: controls.zoom_pct,
            zoom_pct: self.canvas_view.pct(),
            audio_note: audio_note.as_deref(),
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
        if !menu_ui.is_empty() || !ctx_ui.is_empty() || !popup_ui.is_empty() {
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            ui_pass.draw_batches(
                &gpu.device,
                &gpu.queue,
                &mut encoder,
                &frame.view,
                &[(&menu_ui, None), (&ctx_ui, None), (&popup_ui, None)],
                gpu.size(),
                None,
            );
            chrome::menu_labels(text, scale, &scene, res);
            chrome::context_labels(text, &scene, res);
            chrome::draw_labels(text, &popup_labels, res);
            text.draw(&mut encoder, &frame.view, res);
            gpu.queue.submit([encoder.finish()]);
        }
        frame.present();
    }
}
