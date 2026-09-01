//! The studio's half of the arrangement: the paired scene builder and
//! the press dispatch — sidebar rows select objects, eyes toggle,
//! folders collapse, clip bars grab.

use super::{ArrHit, ArrangeScene, ClipDrag, ClipRef, RowKind, build, hit};
use crate::timeline::Panel;

impl crate::Studio {
    /// The arrangement's layout, for hit-testing and drawing alike — the
    /// paired builder every panel needs.
    pub(crate) fn arrange_scene(
        &self,
        panel: &crate::timeline::Panel,
        scale: f32,
    ) -> ArrangeScene {
        build(
            panel,
            &self.time_view,
            scale,
            &self.editor,
            &self.subcomps,
            self.selected_clip,
            self.lanes_scroll,
            self.audio_name().as_deref(),
        )
    }

    /// The song's row label: the loaded track's file name.
    pub(crate) fn audio_name(&self) -> Option<String> {
        self.audio_file.as_ref().map(|p| {
            std::path::Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.clone())
        })
    }

    /// A press on the arrangement: sidebar rows select objects (their
    /// track is the outliner), eyes toggle, folders collapse; clip bars
    /// grab (body moves, edges trim); double-click opens a comp clip's
    /// comp. Returns whether the press was consumed — empty air falls
    /// through to the scrub.
    pub(crate) fn arrange_press(&mut self, panel: &Panel, scale: f32, cx: f32, cy: f32) -> bool {
        let over_lanes = panel.lanes.contains(cx, cy);
        let over_names = panel.names_box.contains(cx, cy);
        if !over_lanes && !over_names {
            return false;
        }
        let sc = self.arrange_scene(panel, scale);
        let Some(hit) = hit(&sc, cx, cy, scale) else {
            if self.selected_clip.take().is_some() {
                self.request_redraw();
            }
            // Empty arrangement air scrubs — the caller's fallthrough.
            return false;
        };
        match hit {
            ArrHit::Disclose(id) => {
                if self.editor.toggle_folder_collapsed(id) {
                    self.request_redraw();
                }
            }
            ArrHit::Eye(RowKind::Object(i)) => {
                if self.editor.toggle_hidden(i) {
                    self.request_redraw();
                }
            }
            ArrHit::Eye(RowKind::Folder(id)) => {
                if self.editor.toggle_folder_hidden(id) {
                    self.request_redraw();
                }
            }
            ArrHit::Eye(_) => {}
            ArrHit::Head(RowKind::Object(i)) => {
                if self.editor.select(Some(i)) {
                    self.request_redraw();
                }
            }
            ArrHit::Head(RowKind::Folder(id)) => {
                if self.editor.select_folder(id) {
                    self.request_redraw();
                }
            }
            ArrHit::Head(_) => {}
            ArrHit::Clip(r, zone) => {
                // A second click on the same comp clip opens its comp;
                // an object clip's detail view is on the roadmap.
                let now = std::time::Instant::now();
                let double = self
                    .last_clip_click
                    .take()
                    .is_some_and(|(pr, t0)| pr == r && now.duration_since(t0).as_millis() < 400);
                if double {
                    match r {
                        ClipRef::Comp(i) => {
                            self.open_clip_comp(i);
                            return true;
                        }
                        ClipRef::Obj { .. } => {
                            println!("clip view coming — keys still stamp with K at the playhead");
                        }
                    }
                }
                self.last_clip_click = Some((r, now));
                self.selected_clip = Some(r);
                let t = self.time_view.t_at(cx, panel.axis);
                let start = match r {
                    ClipRef::Obj { obj, c } => self
                        .editor
                        .index_of(obj)
                        .and_then(|i| self.editor.obj_clips(i).get(c))
                        .map(|cl| cl.start),
                    ClipRef::Comp(i) => self.editor.comp_clips().get(i).map(|c| c.start),
                };
                if let Some(s) = start {
                    // Selecting a clip selects its object too — the track,
                    // the canvas ants and the inspector agree on the thing.
                    if let ClipRef::Obj { obj, .. } = r
                        && let Some(i) = self.editor.index_of(obj)
                        && !self.editor.selection().contains(&i)
                    {
                        self.editor.select(Some(i));
                    }
                    self.clip_drag = Some(ClipDrag {
                        r,
                        zone,
                        grab_dt: t - s,
                    });
                }
                self.request_redraw();
            }
        }
        true
    }
}

