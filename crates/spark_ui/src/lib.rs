//! SparkUI — the editor's own UI, drawn by the engine.
//!
//! `layout` is the container/grid framework; `rects` is the flat quad
//! renderer the chrome is painted with. Widgets come next.

use spark_render::{CANVAS_H, CANVAS_W, Viewport};

pub mod layout;
mod rects;
mod theme;
mod titlebar;
mod widgets;

pub use layout::{Dir, Node, Size};
pub use rects::{
    ICON_ARROW, ICON_CIRCLE, ICON_LINE, ICON_MINUS, ICON_NONE, ICON_PATH, ICON_PAUSE,
    ICON_PENTAGON, ICON_PLAY, ICON_SQUARE, ICON_X, UiPass, UiRect,
};
pub use theme::{srgb, theme};
pub use titlebar::{TitleAction, TitleBar};
pub use widgets::{IconBar, Menu, Segmented, Slider, Swatches, TextField};

/// Which editor region a layout leaf is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Region {
    Title,
    Toolbar,
    Left,
    Viewport,
    Right,
    Timeline,
}

/// The editor's panel regions, solved from the layout tree.
///
/// Slim top toolbar; left inspector; right all-purpose panel (layers, later
/// comps/assets); full-width timeline along the bottom (time deserves every
/// horizontal pixel); the remaining center is the viewport, canvas aspect-fit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub title: Viewport,
    pub top: Viewport,
    pub left: Viewport,
    pub right: Viewport,
    pub timeline: Viewport,
    pub viewport: Viewport,
}

impl Layout {
    pub fn compute(width: u32, height: u32, scale: f32) -> Self {
        // Side panels absorb the viewport's horizontal dead space: the canvas
        // is 16:9, so the center only ever needs the width that aspect-fits
        // its height — whatever's left over splits between the panels, which
        // never shrink below their minimums.
        const LEFT_MIN: f32 = 380.0;
        const RIGHT_MIN: f32 = 340.0;
        let center_h = height as f32 / scale - 44.0 - 64.0 - 360.0;
        let vp_w = center_h.max(1.0) * (CANVAS_W / CANVAS_H);
        let extra = (width as f32 / scale - vp_w - LEFT_MIN - RIGHT_MIN).max(0.0);

        let root = Node::col(Size::Flex(1.0))
            .child(Node::leaf(Size::Px(44.0), Region::Title))
            .child(Node::leaf(Size::Px(64.0), Region::Toolbar))
            .child(
                Node::row(Size::Flex(1.0))
                    .child(Node::leaf(Size::Px(LEFT_MIN + extra * 0.5), Region::Left))
                    .child(Node::leaf(Size::Flex(1.0), Region::Viewport))
                    .child(Node::leaf(Size::Px(RIGHT_MIN + extra * 0.5), Region::Right)),
            )
            .child(Node::leaf(Size::Px(360.0), Region::Timeline));

        let window = Viewport {
            x: 0.0,
            y: 0.0,
            w: width as f32,
            h: height as f32,
        };
        let mut rects = Vec::new();
        root.solve(window, scale, &mut rects);

        let find = |region: Region| {
            rects
                .iter()
                .find(|(r, _)| *r == region)
                .map(|(_, v)| *v)
                .unwrap_or(window)
        };
        Self {
            title: find(Region::Title),
            top: find(Region::Toolbar),
            left: find(Region::Left),
            right: find(Region::Right),
            timeline: find(Region::Timeline),
            viewport: find(Region::Viewport),
        }
    }

    /// The chrome as flat rects: panels plus seam lines between regions.
    pub fn panel_rects(&self, scale: f32) -> Vec<UiRect> {
        let t = theme();
        let seam = (3.0 * scale).max(1.0);
        vec![
            UiRect::region(self.top, t.toolbar),
            UiRect::region(self.left, t.panel),
            UiRect::region(self.right, t.panel),
            UiRect::region(self.timeline, t.timeline),
            // seams
            UiRect {
                pos: [self.top.x, self.top.y + self.top.h - seam],
                size: [self.top.w, seam],
                color: t.seam,
                icon: [0.0; 4],
                color2: [0.0; 4],
            },
            UiRect {
                pos: [self.timeline.x, self.timeline.y],
                size: [self.timeline.w, seam],
                color: t.seam,
                icon: [0.0; 4],
                color2: [0.0; 4],
            },
            UiRect {
                pos: [self.left.x + self.left.w - seam, self.left.y],
                size: [seam, self.left.h],
                color: t.seam,
                icon: [0.0; 4],
                color2: [0.0; 4],
            },
            UiRect {
                pos: [self.right.x, self.right.y],
                size: [seam, self.right.h],
                color: t.seam,
                icon: [0.0; 4],
                color2: [0.0; 4],
            },
        ]
    }
}
