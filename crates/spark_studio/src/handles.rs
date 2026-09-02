//! On-canvas transform handles: corner squares scale, edge squares stretch
//! width/height (boxes and circles), the knob above rotates. A line's rig
//! is its two ends — grab one and the other holds — with the knob still
//! spinning it about its middle. A multi-shape selection gets one
//! axis-aligned group box that transforms everything around the shared
//! center. Pure geometry — drag state lives in main.

use spark_render::{Shape, ShapeKind, Viewport};
use spark_ui::{UiRect, theme};

use crate::editor::Editor;
use crate::view::CanvasMap;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum HandleHit {
    Corner,
    Width,
    Height,
    Rotate,
    /// A path vertex, by index.
    Vertex(usize),
    /// One end of a line: 0 its start, 1 its end.
    End(usize),
}

pub struct Handles {
    /// Selection center in canvas units — the transform pivot.
    pub center: [f32; 2],
    /// The scale rig — every kind's but a line's, whose ends are its rig.
    corners: Option<[Viewport; 4]>,
    width: Option<[Viewport; 2]>,
    height: Option<[Viewport; 2]>,
    rotate: Viewport,
    /// Editable vertices (single selected path only).
    verts: Vec<Viewport>,
    /// A line's two ends (single selected line only).
    ends: Option<[Viewport; 2]>,
}

fn half_extents(s: &Shape) -> [f32; 2] {
    match s.kind() {
        // A star field's rig grips its region, the same as a box's.
        ShapeKind::Box | ShapeKind::Circle | ShapeKind::Stars => {
            let d = s.box_size().unwrap_or([6.0, 6.0]);
            [d[0] * 0.5, d[1] * 0.5]
        }
        ShapeKind::Ngon | ShapeKind::Path => [s.size(), s.size()],
        // A mesh's rig grips its fitted footprint; a light's, its gizmo.
        ShapeKind::Mesh => s.mesh_half().unwrap_or([6.0, 6.0]),
        ShapeKind::Light => [spark_render::LIGHT_PICK; 2],
        ShapeKind::Line => [s.size(), s.thickness().unwrap_or(3.0)],
    }
}

pub fn build(editor: &Editor, map: CanvasMap, ui_scale: f32) -> Option<Handles> {
    let selection = editor.selection();
    let primary = editor.primary()?;
    if editor.is_hidden(primary) || !editor.exists_now(primary) {
        // Hidden, or no clip under the playhead: nothing to rig.
        return None;
    }
    let (map_s, ox, oy) = map;
    let side = 17.0 * ui_scale;
    let handle_at = |w: [f32; 2]| Viewport {
        x: w[0] - side * 0.5,
        y: w[1] - side * 0.5,
        w: side,
        h: side,
    };

    let (center, half, rot, edges) = if selection.len() == 1 {
        let s = &editor.shapes()[primary];
        let pad = 14.0;
        let h = half_extents(s);
        let stretchy = matches!(
            s.kind(),
            ShapeKind::Box | ShapeKind::Circle | ShapeKind::Stars
        );
        (s.center(), [h[0] + pad, h[1] + pad], s.rotation(), stretchy)
    } else {
        // Group: axis-aligned bounds over rough per-shape extents.
        let mut min = [f32::MAX; 2];
        let mut max = [f32::MIN; 2];
        for &i in selection {
            let s = &editor.shapes()[i];
            let c = s.center();
            let r = s.size();
            for a in 0..2 {
                min[a] = min[a].min(c[a] - r);
                max[a] = max[a].max(c[a] + r);
            }
        }
        let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
        let half = [
            (max[0] - min[0]) * 0.5 + 14.0,
            (max[1] - min[1]) * 0.5 + 14.0,
        ];
        (center, half, 0.0, false)
    };

    let (sn, cs) = rot.sin_cos();
    let to_window = |local: [f32; 2]| {
        let x = center[0] + local[0] * cs - local[1] * sn;
        let y = center[1] + local[0] * sn + local[1] * cs;
        [ox + x * map_s, oy + y * map_s]
    };
    // A line's ends are its handles: a square on each, where the corner
    // rig would only crowd them (2026-09-01: "I can't move just one end
    // around while the other stays in one spot").
    let ends = (selection.len() == 1)
        .then(|| &editor.shapes()[primary])
        .filter(|s| s.is_line())
        .map(|s| {
            let (a, b) = s.line_ends();
            [a, b].map(|p| handle_at([ox + p[0] * map_s, oy + p[1] * map_s]))
        });
    let corners = ends.is_none().then(|| {
        [
            handle_at(to_window([-half[0], -half[1]])),
            handle_at(to_window([half[0], -half[1]])),
            handle_at(to_window([half[0], half[1]])),
            handle_at(to_window([-half[0], half[1]])),
        ]
    });
    let width = edges.then(|| {
        [
            handle_at(to_window([-half[0], 0.0])),
            handle_at(to_window([half[0], 0.0])),
        ]
    });
    let height = edges.then(|| {
        [
            handle_at(to_window([0.0, -half[1]])),
            handle_at(to_window([0.0, half[1]])),
        ]
    });
    // The rotate knob floats a fixed screen distance above the top edge.
    let top = to_window([0.0, -half[1]]);
    let up = [sn * 34.0 * ui_scale, -cs * 34.0 * ui_scale];
    let rotate = handle_at([top[0] + up[0], top[1] + up[1]]);

    let vside = 13.0 * ui_scale;
    let verts = if selection.len() == 1 {
        let s = &editor.shapes()[primary];
        match s.path_meta() {
            Some((id, count, _)) => editor
                .path(id)
                .iter()
                .take(count)
                .map(|&v| {
                    let w = to_window(v);
                    Viewport {
                        x: w[0] - vside * 0.5,
                        y: w[1] - vside * 0.5,
                        w: vside,
                        h: vside,
                    }
                })
                .collect(),
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };

    Some(Handles {
        center,
        corners,
        width,
        height,
        rotate,
        verts,
        ends,
    })
}

impl Handles {
    pub fn hit(&self, px: f32, py: f32) -> Option<HandleHit> {
        if let Some(k) = self.verts.iter().position(|v| v.contains(px, py)) {
            return Some(HandleHit::Vertex(k));
        }
        if let Some(k) = self
            .ends
            .and_then(|e| e.iter().position(|v| v.contains(px, py)))
        {
            return Some(HandleHit::End(k));
        }
        if self.rotate.contains(px, py) {
            return Some(HandleHit::Rotate);
        }
        if self
            .corners
            .is_some_and(|c| c.iter().any(|v| v.contains(px, py)))
        {
            return Some(HandleHit::Corner);
        }
        if let Some(w) = &self.width
            && w.iter().any(|v| v.contains(px, py))
        {
            return Some(HandleHit::Width);
        }
        if let Some(h) = &self.height
            && h.iter().any(|v| v.contains(px, py))
        {
            return Some(HandleHit::Height);
        }
        None
    }

    pub fn rects(&self, scale: f32) -> Vec<UiRect> {
        let t = theme();
        let mut out = Vec::new();
        let mut square = |v: Viewport| {
            out.push(UiRect::region_rounded(v, t.accent, 5.0 * scale));
            let inset = 2.5 * scale;
            out.push(UiRect::region_rounded(
                Viewport {
                    x: v.x + inset,
                    y: v.y + inset,
                    w: v.w - inset * 2.0,
                    h: v.h - inset * 2.0,
                },
                t.panel,
                3.5 * scale,
            ));
        };
        for c in self.corners.iter().flatten() {
            square(*c);
        }
        // A line's ends wear the rig's own gold: they *are* its rig.
        for pair in [&self.ends, &self.width, &self.height].into_iter().flatten() {
            for &v in pair {
                square(v);
            }
        }
        // Path vertices: accent purple, so they read apart from the gold rig.
        for &v in &self.verts {
            out.push(UiRect::region_rounded(v, t.accent_alt, 4.0 * scale));
        }
        // The rotate knob: solid gold, round.
        out.push(UiRect::region_rounded(
            self.rotate,
            t.accent,
            self.rotate.w * 0.5,
        ));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::Tool;

    fn draw(e: &mut Editor, tool: Tool, a: [f32; 2], b: [f32; 2]) -> usize {
        e.set_time(0.0);
        e.sync_to_time();
        e.choose_tool(tool);
        e.set_cursor_canvas(a);
        e.mouse_down(false);
        e.set_cursor_canvas(b);
        e.mouse_up();
        e.choose_tool(Tool::Select);
        e.primary().expect("drawn")
    }

    /// A selected line's rig is a square on each end and the rotate
    /// knob — no corner rig to crowd them — and a press on an end says
    /// which end; a circle keeps its corners and has no ends.
    #[test]
    fn a_lines_rig_is_its_ends() {
        let mut e = Editor::empty();
        draw(&mut e, Tool::Line, [100.0, 100.0], [500.0, 300.0]);
        // The identity map: canvas units are window px.
        let h = build(&e, (1.0, 0.0, 0.0), 1.0).expect("a rig");
        assert!(h.corners.is_none() && h.ends.is_some());
        assert_eq!(h.hit(100.0, 100.0), Some(HandleHit::End(0)));
        assert_eq!(h.hit(500.0, 300.0), Some(HandleHit::End(1)));
        assert_eq!(h.hit(300.0, 200.0), None, "the middle is the line, not a handle");
        assert!(h.rects(1.0).len() >= 5, "two ends, two-part each, and the knob");
        draw(&mut e, Tool::Circle, [800.0, 500.0], [860.0, 500.0]);
        let h = build(&e, (1.0, 0.0, 0.0), 1.0).expect("a rig");
        assert!(h.corners.is_some() && h.ends.is_none());
        assert!(matches!(h.hit(800.0 - 74.0, 500.0 - 74.0), Some(HandleHit::Corner)));
    }
}
