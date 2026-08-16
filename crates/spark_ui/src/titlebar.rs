//! Custom title bar: window controls at the far right, drag zone everywhere
//! else. The first real SparkUI widgets — hover, press/release, icon glyphs.

use spark_render::Viewport;

use crate::rects::{ICON_MINUS, ICON_SQUARE, ICON_X, UiRect};
use crate::theme::theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TitleAction {
    Minimize,
    Maximize,
    Close,
}

pub struct TitleBar {
    pub rect: Viewport,
    buttons: [(TitleAction, Viewport); 3],
    scale: f32,
}

impl TitleBar {
    pub fn new(rect: Viewport, scale: f32) -> Self {
        let bw = 60.0 * scale;
        let mut x = rect.x + rect.w - bw * 3.0;
        let mut slot = |action: TitleAction| {
            let v = Viewport {
                x,
                y: rect.y,
                w: bw,
                h: rect.h,
            };
            x += bw;
            (action, v)
        };
        Self {
            rect,
            buttons: [
                slot(TitleAction::Minimize),
                slot(TitleAction::Maximize),
                slot(TitleAction::Close),
            ],
            scale,
        }
    }

    pub fn hit(&self, px: f32, py: f32) -> Option<TitleAction> {
        self.buttons
            .iter()
            .find(|(_, v)| v.contains(px, py))
            .map(|(a, _)| *a)
    }

    /// On the bar but not on a button — grab here to move the window.
    pub fn in_drag_zone(&self, px: f32, py: f32) -> bool {
        self.rect.contains(px, py) && self.hit(px, py).is_none()
    }

    pub fn rects(&self, hover: Option<TitleAction>) -> Vec<UiRect> {
        let t = theme();
        let mut v = vec![UiRect::region(self.rect, t.title)];
        for (action, r) in self.buttons {
            let hovered = hover == Some(action);
            if hovered {
                let bg = if action == TitleAction::Close {
                    t.close_hover
                } else {
                    t.button_hover
                };
                v.push(UiRect::region(r, bg));
            }
            let fg = if hovered { t.icon_hover } else { t.icon };
            let kind = match action {
                TitleAction::Minimize => ICON_MINUS,
                TitleAction::Maximize => ICON_SQUARE,
                TitleAction::Close => ICON_X,
            };
            v.push(UiRect::icon(r, kind, 1.4 * self.scale, fg));
        }
        v
    }
}
