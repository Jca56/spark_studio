//! The Arrange tab: tracks of clips across the time axis — the
//! arrangement the video is composed on, as the timeline always planned
//! ("tracks of clips instancing comps"). Pure layout and hit-testing;
//! clicks live in input, evaluation in `comps.rs`.
//!
//! A clip bar shows its comp's name and a faint tick at every loop
//! seam, so "how many times does this play" is something you can see.
//! Dragging the body moves a clip (and can carry it to another track);
//! dragging either edge trims it; the comp inside keeps looping
//! whatever the length. A clip whose file can't be read stays on the
//! arrangement and says so — a broken path is a thing to fix, not to
//! hide.

use std::collections::HashMap;

use spark_render::Viewport;
use spark_ui::{UiRect, theme};

use crate::comps::PlacedComp;
use crate::editor::Editor;
use crate::timeline::{Panel, TimeView};

/// Track label / clip label size, logical px (matches the lanes).
pub const TRACK_TEXT: f32 = 20.0;

/// Row pitch and height, logical px — a touch taller than the key lanes:
/// clips carry a name and loop ticks, and they're the main event here.
pub const ROW_STEP: f32 = 60.0;
const ROW_H: f32 = 52.0;
/// How close to a clip's edge (logical px) a press becomes a trim.
const EDGE: f32 = 12.0;

pub struct TrackRow {
    pub cell: Viewport,
    pub label: String,
    pub label_pos: [f32; 2],
    pub label_max_w: f32,
}

pub struct ClipRow {
    /// Index into the editor's clip list.
    pub index: usize,
    pub bar: Viewport,
    pub label: String,
    pub label_pos: [f32; 2],
    pub label_max_w: f32,
    pub selected: bool,
    pub missing: bool,
    /// x of every loop seam inside the bar.
    pub loop_xs: Vec<f32>,
}

/// Everything the Arrange tab draws this frame.
pub struct ArrangeScene {
    pub tracks: Vec<TrackRow>,
    pub clips: Vec<ClipRow>,
    /// Where the empty-state hint goes, when there is nothing else.
    pub hint: Option<[f32; 2]>,
}

/// Which part of a clip a press grabs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Zone {
    Move,
    Left,
    Right,
}

/// A clip drag in progress: which clip, which grip, and how far into the
/// clip the cursor grabbed it (so a move doesn't jump to the cursor).
#[derive(Clone, Copy)]
pub struct ClipDrag {
    pub index: usize,
    pub zone: Zone,
    pub grab_dt: f32,
}

/// Tracks shown: every one a clip lives on, plus an empty one to drop
/// onto — never fewer than three, so the tab reads as a place.
pub fn track_count(ed: &Editor) -> u32 {
    let top = ed.clips().iter().map(|c| c.track + 1).max().unwrap_or(0);
    (top + 1).max(3)
}

/// Content height for scroll clamping.
pub fn content_height(ed: &Editor, scale: f32) -> f32 {
    track_count(ed) as f32 * ROW_STEP * scale
}

/// The track row under a window y, for carrying a dragged clip.
pub fn track_at(panel: &Panel, scale: f32, scroll: f32, y: f32) -> u32 {
    let row = ((y - panel.lanes.y + scroll) / (ROW_STEP * scale)).floor();
    row.max(0.0) as u32
}

pub fn build(
    panel: &Panel,
    view: &TimeView,
    scale: f32,
    ed: &Editor,
    subcomps: &HashMap<u32, PlacedComp>,
    selected: Option<usize>,
    scroll: f32,
) -> ArrangeScene {
    let mut tracks = Vec::new();
    for ti in 0..track_count(ed) {
        let y = panel.lanes.y - scroll + ti as f32 * ROW_STEP * scale;
        let cell = Viewport {
            x: panel.names_box.x + 6.0 * scale,
            y: y + 2.0 * scale,
            w: panel.names_box.w - 12.0 * scale,
            h: ROW_H * scale - 4.0 * scale,
        };
        let label = format!("Track {}", ti + 1);
        tracks.push(TrackRow {
            cell,
            label_pos: [
                cell.x + 12.0 * scale,
                cell.y + (cell.h - spark_text::Text::line_height(TRACK_TEXT * scale)) * 0.5,
            ],
            label_max_w: cell.w - 24.0 * scale,
            label,
        });
    }
    let (ax, aw) = panel.axis;
    let mut clips = Vec::new();
    for (i, c) in ed.clips().iter().enumerate() {
        let x0 = view.x_of(c.start, panel.axis);
        let x1 = view.x_of(c.start + c.len, panel.axis);
        if x1 < ax || x0 > ax + aw {
            continue;
        }
        let y = panel.lanes.y - scroll + c.track as f32 * ROW_STEP * scale;
        let bar = Viewport {
            x: x0,
            y: y + 4.0 * scale,
            w: (x1 - x0).max(2.0),
            h: (ROW_H - 8.0) * scale,
        };
        let (name, period, missing) = match subcomps.get(&c.comp) {
            Some(pc) if pc.missing => (format!("! missing: {}", pc.name()), pc.period, true),
            Some(pc) => (pc.name(), pc.period, false),
            None => ("loading...".to_string(), f32::MAX, false),
        };
        // A tick at every loop seam inside the bar.
        let mut loop_xs = Vec::new();
        let mut k = 1;
        while c.start + k as f32 * period < c.start + c.len && k < 512 {
            let x = view.x_of(c.start + k as f32 * period, panel.axis);
            if x > ax && x < ax + aw {
                loop_xs.push(x);
            }
            k += 1;
        }
        let lx = (bar.x + 10.0 * scale).max(ax + 6.0 * scale);
        clips.push(ClipRow {
            index: i,
            bar,
            label_pos: [
                lx,
                bar.y + (bar.h - spark_text::Text::line_height(TRACK_TEXT * scale)) * 0.5,
            ],
            label_max_w: (bar.x + bar.w - lx - 8.0 * scale).max(1.0),
            label: name,
            selected: selected == Some(i),
            missing,
            loop_xs,
        });
    }
    let hint = ed
        .clips()
        .is_empty()
        .then_some([ax + aw * 0.5 - 300.0 * scale, panel.lanes.y + 40.0 * scale]);
    ArrangeScene {
        tracks,
        clips,
        hint,
    }
}

/// The tab's rects: track cells for the sidebar batch, clip bars for the
/// axis batch (which clips them to the time axis).
pub fn rects(sc: &ArrangeScene, scale: f32) -> (Vec<UiRect>, Vec<UiRect>) {
    let t = theme();
    let lanes_ui: Vec<UiRect> = sc
        .tracks
        .iter()
        .map(|tr| UiRect::region_rounded(tr.cell, t.card, 8.0 * scale))
        .collect();
    let mut axis_ui = Vec::new();
    for c in &sc.clips {
        let r = 8.0 * scale;
        let fill = if c.missing {
            [t.red[0] * 0.4, t.red[1] * 0.15, t.red[2] * 0.15, 0.9]
        } else {
            [t.red[0] * 0.55, t.red[1] * 0.35, t.red[2] * 0.35, 0.55]
        };
        let bar = UiRect::region_rounded(c.bar, fill, r);
        axis_ui.push(if c.selected {
            bar.stroke_outer(2.5 * scale, t.accent)
        } else {
            bar.stroke_outer(1.5 * scale, [t.red[0], t.red[1], t.red[2], 0.8])
        });
        for &x in &c.loop_xs {
            axis_ui.push(UiRect::region(
                Viewport {
                    x: x - 0.75 * scale,
                    y: c.bar.y + 3.0 * scale,
                    w: 1.5 * scale,
                    h: c.bar.h - 6.0 * scale,
                },
                [1.0, 1.0, 1.0, 0.30],
            ));
        }
    }
    (lanes_ui, axis_ui)
}

/// The clip and grip under a point — later clips win, since they draw
/// over. Returns the clip's editor index.
pub fn hit(sc: &ArrangeScene, x: f32, y: f32, scale: f32) -> Option<(usize, Zone)> {
    for c in sc.clips.iter().rev() {
        if !c.bar.contains(x, y) {
            continue;
        }
        let m = (EDGE * scale).min(c.bar.w * 0.33);
        let zone = if x < c.bar.x + m {
            Zone::Left
        } else if x > c.bar.x + c.bar.w - m {
            Zone::Right
        } else {
            Zone::Move
        };
        return Some((c.index, zone));
    }
    None
}

impl crate::Studio {
    /// The Arrange tab's layout, for hit-testing and drawing alike.
    pub(crate) fn arrange_scene(
        &self,
        panel: &crate::timeline::Panel,
        scale: f32,
    ) -> crate::arrange::ArrangeScene {
        crate::arrange::build(
            panel,
            &self.time_view,
            scale,
            &self.editor,
            &self.subcomps,
            self.selected_clip,
            self.lanes_scroll,
        )
    }

    /// A press on the Arrange tab: grab a clip (body moves, edges trim),
    /// double-click opens its comp, empty air deselects and falls through
    /// to the scrub. Returns whether the press was consumed. Lives here
    /// with the layout it hit-tests, the way the transport's presses live
    /// with the transport.
    pub(crate) fn arrange_press(&mut self, panel: &Panel, scale: f32, cx: f32, cy: f32) -> bool {
        if self.timeline_tab != crate::timeline::Tab::Arrange || !panel.lanes.contains(cx, cy) {
            return false;
        }
        let sc = self.arrange_scene(panel, scale);
        let Some((idx, zone)) = hit(&sc, cx, cy, scale) else {
            if self.selected_clip.take().is_some() {
                self.request_redraw();
            }
            // Empty arrangement air scrubs — the caller's fallthrough.
            return false;
        };
        // A second click on the same clip opens its comp.
        let now = std::time::Instant::now();
        if self
            .last_clip_click
            .take()
            .is_some_and(|(pi, t0)| pi == idx && now.duration_since(t0).as_millis() < 400)
        {
            self.open_clip_comp(idx);
            return true;
        }
        self.last_clip_click = Some((idx, now));
        self.selected_clip = Some(idx);
        if let Some(c) = self.editor.clips().get(idx) {
            let t = self.time_view.t_at(cx, panel.axis);
            self.clip_drag = Some(ClipDrag {
                index: idx,
                zone,
                grab_dt: t - c.start,
            });
        }
        self.request_redraw();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline;

    fn fixture() -> (Panel, TimeView, Editor) {
        let panel = timeline::panel(
            Viewport {
                x: 0.0,
                y: 500.0,
                w: 3000.0,
                h: 400.0,
            },
            1.0,
        );
        let view = TimeView::new(0.0, 60.0);
        let mut ed = Editor::empty();
        let id = ed.add_comp_asset("/x/spin.spark".into());
        ed.place_clip(id, 0, 10.0, 20.0);
        (panel, view, ed)
    }

    /// A clip bar sits where its times say, on its track's row, and the
    /// loop seams land every period inside it.
    #[test]
    fn a_clip_bar_maps_time_and_track_and_marks_its_loops() {
        let (panel, view, ed) = fixture();
        let mut subs = HashMap::new();
        let doc = crate::doc::Doc {
            duration: Some(2.0),
            ..Default::default()
        };
        subs.insert(1, PlacedComp::new("/x/spin.spark".into(), doc, Vec::new()));
        let sc = build(&panel, &view, 1.0, &ed, &subs, Some(0), 0.0);
        assert_eq!(sc.clips.len(), 1);
        let c = &sc.clips[0];
        assert!((c.bar.x - view.x_of(10.0, panel.axis)).abs() < 0.5);
        assert!((c.bar.x + c.bar.w - view.x_of(30.0, panel.axis)).abs() < 0.5);
        // 20 s of a 2 s comp: nine interior seams.
        assert_eq!(c.loop_xs.len(), 9);
        assert!(c.selected && !c.missing);
        assert_eq!(c.label, "spin");
        // Three tracks even with one clip; the hint only on an empty tab.
        assert_eq!(sc.tracks.len(), 3);
        assert!(sc.hint.is_none());
        assert!(
            build(&panel, &view, 1.0, &Editor::empty(), &subs, None, 0.0)
                .hint
                .is_some()
        );
    }

    /// Edges trim, the middle moves, empty air is nothing.
    #[test]
    fn the_grips_are_edges_then_body() {
        let (panel, view, ed) = fixture();
        let subs = HashMap::new();
        let sc = build(&panel, &view, 1.0, &ed, &subs, None, 0.0);
        let c = &sc.clips[0];
        let y = c.bar.y + c.bar.h * 0.5;
        assert_eq!(hit(&sc, c.bar.x + 3.0, y, 1.0), Some((0, Zone::Left)));
        assert_eq!(
            hit(&sc, c.bar.x + c.bar.w - 3.0, y, 1.0),
            Some((0, Zone::Right))
        );
        assert_eq!(
            hit(&sc, c.bar.x + c.bar.w * 0.5, y, 1.0),
            Some((0, Zone::Move))
        );
        assert_eq!(hit(&sc, c.bar.x - 30.0, y, 1.0), None);
        // The row under a y, scroll included.
        assert_eq!(track_at(&panel, 1.0, 0.0, panel.lanes.y + 5.0), 0);
        assert_eq!(track_at(&panel, 1.0, 0.0, panel.lanes.y + ROW_STEP + 5.0), 1);
        assert_eq!(track_at(&panel, 1.0, ROW_STEP, panel.lanes.y + 5.0), 1);
    }
}

