//! On-canvas transform handles: corner squares scale, edge squares stretch
//! width/height (boxes and circles), the knob above rotates. A multi-shape
//! selection gets one axis-aligned group box that transforms everything
//! around the shared center. Pure geometry — drag state lives in main.

use spark_render::{Shape, ShapeKind, Viewport};
use spark_ui::{UiRect, theme};

use crate::editor::Editor;
use crate::view::CanvasMap;

#[derive(Clone, Copy, PartialEq)]
pub enum HandleHit {
    Corner,
    Width,
    Height,
    Rotate,
    /// A path vertex, by index.
    Vertex(usize),
}

pub struct Handles {
    /// Selection center in canvas units — the transform pivot.
    pub center: [f32; 2],
    corners: [Viewport; 4],
    width: Option<[Viewport; 2]>,
    height: Option<[Viewport; 2]>,
    rotate: Viewport,
    /// Editable vertices (single selected path only).
    verts: Vec<Viewport>,
}

fn half_extents(s: &Shape) -> [f32; 2] {
    match s.kind() {
        ShapeKind::Box | ShapeKind::Circle => {
            let d = s.box_size().unwrap_or([6.0, 6.0]);
            [d[0] * 0.5, d[1] * 0.5]
        }
        ShapeKind::Ngon | ShapeKind::Path => [s.size(), s.size()],
        ShapeKind::Line => [s.size(), s.thickness().unwrap_or(3.0)],
    }
}

pub fn build(editor: &Editor, map: CanvasMap, ui_scale: f32) -> Option<Handles> {
    let selection = editor.selection();
    let primary = editor.primary()?;
    if editor.is_hidden(primary) {
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
        let stretchy = matches!(s.kind(), ShapeKind::Box | ShapeKind::Circle);
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
    let corners = [
        handle_at(to_window([-half[0], -half[1]])),
        handle_at(to_window([half[0], -half[1]])),
        handle_at(to_window([half[0], half[1]])),
        handle_at(to_window([-half[0], half[1]])),
    ];
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
    })
}

impl Handles {
    pub fn hit(&self, px: f32, py: f32) -> Option<HandleHit> {
        if let Some(k) = self.verts.iter().position(|v| v.contains(px, py)) {
            return Some(HandleHit::Vertex(k));
        }
        if self.rotate.contains(px, py) {
            return Some(HandleHit::Rotate);
        }
        if self.corners.iter().any(|v| v.contains(px, py)) {
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
        for &c in &self.corners {
            square(c);
        }
        for pair in [&self.width, &self.height].into_iter().flatten() {
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
