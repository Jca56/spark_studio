//! Custom title bar: drag zone across the bar, logo block (app icon +
//! wordmark) at the right, then small round window controls — structurally
//! Lantern-Studio-shaped, styled entirely Spark.

use spark_render::Viewport;

use crate::rects::{ICON_IMAGE, ICON_MINUS, ICON_SQUARE, ICON_X, UiRect};
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
    icon: Viewport,
    wordmark_x: f32,
    scale: f32,
}

impl TitleBar {
    /// `wordmark_w` is the measured pixel width of the wordmark text, which
    /// the caller draws (text rendering lives above this crate).
    pub fn new(rect: Viewport, scale: f32, wordmark_w: f32) -> Self {
        let side = 32.0 * scale;
        let gap = 2.0 * scale;
        let right_pad = 10.0 * scale;
        let mut x = rect.x + rect.w - right_pad - (side * 3.0 + gap * 2.0);
        let y = rect.y + (rect.h - side) * 0.5;
        let mut slot = |action: TitleAction| {
            let v = Viewport {
                x,
                y,
                w: side,
                h: side,
            };
            x += side + gap;
            (action, v)
        };
        let buttons = [
            slot(TitleAction::Minimize),
            slot(TitleAction::Maximize),
            slot(TitleAction::Close),
        ];
        let wordmark_x = buttons[0].1.x - 18.0 * scale - wordmark_w;
        let icon_side = 30.0 * scale;
        let icon = Viewport {
            x: wordmark_x - 10.0 * scale - icon_side,
            y: rect.y + (rect.h - icon_side) * 0.5,
            w: icon_side,
            h: icon_side,
        };
        Self {
            rect,
            buttons,
            icon,
            wordmark_x,
            scale,
        }
    }

    /// Left edge of the wordmark text (caller vertically centers it).
    pub fn wordmark_x(&self) -> f32 {
        self.wordmark_x
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
                v.push(UiRect::region_rounded(r, bg, r.w * 0.5));
            }
            let fg = if hovered { t.icon_hover } else { t.icon };
            let kind = match action {
                TitleAction::Minimize => ICON_MINUS,
                TitleAction::Maximize => ICON_SQUARE,
                TitleAction::Close => ICON_X,
            };
            v.push(UiRect::icon(r, kind, 1.3 * self.scale, fg));
        }
        v.push(UiRect::icon(
            self.icon,
            ICON_IMAGE,
            0.0,
            [1.0, 1.0, 1.0, 1.0],
        ));
        v
    }
}
