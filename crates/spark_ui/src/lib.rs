//! SparkUI — the editor's own UI, drawn by the engine.
//!
//! `layout` is the container/grid framework; `rects` is the flat quad
//! renderer the chrome is painted with. Widgets come next.

use spark_render::Viewport;

pub mod layout;
mod rects;
mod theme;
mod titlebar;
mod widgets;

pub use layout::{Dir, Node, Size};
pub use rects::{
    ICON_ARROW, ICON_CIRCLE, ICON_LINE, ICON_MINUS, ICON_NONE, ICON_PENTAGON, ICON_SQUARE,
    ICON_X, UiPass, UiRect,
};
pub use theme::{srgb, theme};
pub use titlebar::{TitleAction, TitleBar};
pub use widgets::IconBar;

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
/// Slim top toolbar; left all-purpose panel; right inspector; full-width
/// timeline along the bottom (time deserves every horizontal pixel); the
/// remaining center is the viewport, canvas aspect-fit.
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
        let root = Node::col(Size::Flex(1.0))
            .child(Node::leaf(Size::Px(44.0), Region::Title))
            .child(Node::leaf(Size::Px(64.0), Region::Toolbar))
            .child(
                Node::row(Size::Flex(1.0))
                    .child(Node::leaf(Size::Px(340.0), Region::Left))
                    .child(Node::leaf(Size::Flex(1.0), Region::Viewport))
                    .child(Node::leaf(Size::Px(380.0), Region::Right)),
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
        let seam = (2.0 * scale).max(1.0);
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
            },
            UiRect {
                pos: [self.timeline.x, self.timeline.y],
                size: [self.timeline.w, seam],
                color: t.seam,
                icon: [0.0; 4],
            },
            UiRect {
                pos: [self.left.x + self.left.w - seam, self.left.y],
                size: [seam, self.left.h],
                color: t.seam,
                icon: [0.0; 4],
            },
            UiRect {
                pos: [self.right.x, self.right.y],
                size: [seam, self.right.h],
                color: t.seam,
                icon: [0.0; 4],
            },
        ]
    }
}
