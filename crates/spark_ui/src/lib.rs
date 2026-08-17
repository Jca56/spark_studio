//! SparkUI — the editor's own UI, drawn by the engine.
//!
//! `layout` is the container/grid framework; `rects` is the flat quad
//! renderer the chrome is painted with. Widgets come next.

use spark_render::{CANVAS_H, CANVAS_W, Viewport};

pub mod layout;
pub mod picker;
mod rects;
mod theme;
mod titlebar;
mod widgets;

pub use layout::{Dir, Node, Size};
pub use picker::ColorPicker;
pub use rects::{
    ICON_ARROW, ICON_CIRCLE, ICON_EYE, ICON_EYE_OFF, ICON_GEAR, ICON_HSV, ICON_HUE, ICON_KEY,
    ICON_LINE, ICON_MINUS, ICON_NONE, ICON_PATH, ICON_PAUSE, ICON_PENTAGON, ICON_PLAY, ICON_SQUARE,
    ICON_X, UiPass, UiRect,
};
pub use theme::{srgb, theme};
pub use titlebar::{TitleAction, TitleBar};
pub use widgets::{IconBar, Menu, Segmented, Slider, Swatches, TextField};

/// Which editor region a layout leaf is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Region {
    Title,
    Tools,
    Toolbar,
    Left,
    Viewport,
    Right,
    /// The zoom bar pinned to the bottom of the right panel.
    Zoom,
    Timeline,
}

/// The editor's panel regions, solved from the layout tree.
///
/// Shape tools live in a strip pinned to the top of the left panel; the
/// transport toolbar runs between the viewport row and the timeline; the
/// timeline's height is user-resizable (drag its top border); the remaining
/// center is the viewport, canvas aspect-fit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub title: Viewport,
    /// The shape-tool strip atop the left panel — always visible.
    pub tools: Viewport,
    /// The transport toolbar between the viewport row and the timeline.
    pub toolbar: Viewport,
    pub left: Viewport,
    pub right: Viewport,
    /// The zoom bar strip at the bottom of the right panel.
    pub zoom: Viewport,
    pub timeline: Viewport,
    pub viewport: Viewport,
}

impl Layout {
    /// Smallest useful timeline height / center row height, logical px.
    pub const TIMELINE_MIN: f32 = 180.0;
    const CENTER_MIN: f32 = 220.0;

    /// Clamp a requested timeline height to what the window allows.
    pub fn clamp_timeline_h(height: u32, scale: f32, timeline_h: f32) -> f32 {
        let max = (height as f32 / scale - 44.0 - 64.0 - Self::CENTER_MIN).max(Self::TIMELINE_MIN);
        timeline_h.clamp(Self::TIMELINE_MIN, max)
    }

    pub fn compute(width: u32, height: u32, scale: f32, timeline_h: f32) -> Self {
        // Side panels absorb the viewport's horizontal dead space: the canvas
        // is 16:9, so the center only ever needs the width that aspect-fits
        // its height — whatever's left over splits between the panels, which
        // never shrink below their minimums.
        const LEFT_MIN: f32 = 380.0;
        // The right panel carries the layer cards' X/Y/R/S strip — four
        // numeric fields across one row — so it needs the wider floor.
        const RIGHT_MIN: f32 = 440.0;
        let tl_h = Self::clamp_timeline_h(height, scale, timeline_h);
        let center_h = height as f32 / scale - 44.0 - 64.0 - tl_h;
        let vp_w = center_h.max(1.0) * (CANVAS_W / CANVAS_H);
        let extra = (width as f32 / scale - vp_w - LEFT_MIN - RIGHT_MIN).max(0.0);

        let root = Node::col(Size::Flex(1.0))
            .child(Node::leaf(Size::Px(44.0), Region::Title))
            .child(
                Node::row(Size::Flex(1.0))
                    .child(
                        Node::col(Size::Px(LEFT_MIN + extra * 0.5))
                            .child(Node::leaf(Size::Px(64.0), Region::Tools))
                            .child(Node::leaf(Size::Flex(1.0), Region::Left)),
                    )
                    .child(Node::leaf(Size::Flex(1.0), Region::Viewport))
                    .child(
                        Node::col(Size::Px(RIGHT_MIN + extra * 0.5))
                            .child(Node::leaf(Size::Flex(1.0), Region::Right))
                            .child(Node::leaf(Size::Px(64.0), Region::Zoom)),
                    ),
            )
            .child(Node::leaf(Size::Px(64.0), Region::Toolbar))
            .child(Node::leaf(Size::Px(tl_h), Region::Timeline));

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
            tools: find(Region::Tools),
            toolbar: find(Region::Toolbar),
            left: find(Region::Left),
            right: find(Region::Right),
            zoom: find(Region::Zoom),
            timeline: find(Region::Timeline),
            viewport: find(Region::Viewport),
        }
    }

    /// The chrome as flat rects: panels plus seam lines between regions.
    pub fn panel_rects(&self, scale: f32) -> Vec<UiRect> {
        let t = theme();
        let seam = (3.0 * scale).max(1.0);
        let line = |pos: [f32; 2], size: [f32; 2]| UiRect {
            pos,
            size,
            color: t.seam,
            icon: [0.0; 4],
            color2: [0.0; 4],
        };
        vec![
            UiRect::region(self.tools, t.toolbar),
            UiRect::region(self.toolbar, t.toolbar),
            UiRect::region(self.left, t.panel),
            UiRect::region(self.right, t.panel),
            UiRect::region(self.zoom, t.toolbar),
            UiRect::region(self.timeline, t.timeline),
            // seams
            line(
                [self.tools.x, self.tools.y + self.tools.h - seam],
                [self.tools.w, seam],
            ),
            line([self.toolbar.x, self.toolbar.y], [self.toolbar.w, seam]),
            line([self.zoom.x, self.zoom.y], [self.zoom.w, seam]),
            line([self.timeline.x, self.timeline.y], [self.timeline.w, seam]),
            line(
                [self.left.x + self.left.w - seam, self.tools.y],
                [seam, self.tools.h + self.left.h],
            ),
            line(
                [self.right.x, self.right.y],
                [seam, self.right.h + self.zoom.h],
            ),
        ]
    }
}
