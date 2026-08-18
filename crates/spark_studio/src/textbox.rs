//! A real text field: a buffer with a caret and a selection.
//!
//! The scrub fields used to be a bare `String` you could only append to or
//! backspace from. Clicking one gave no feedback beyond a border, and
//! replacing a value meant deleting every digit first. This is the model
//! behind them now — caret movement, selection, insert-over-selection —
//! so a field behaves the way a text field is expected to.
//!
//! Indices are **byte offsets** into the buffer and always land on a `char`
//! boundary; every mover steps whole characters, so slicing at a caret is
//! always safe.

/// An editable buffer with a caret and an optional selection anchor.
#[derive(Clone, PartialEq, Debug)]
pub struct TextBox {
    text: String,
    /// Byte offset of the caret.
    caret: usize,
    /// Where a selection started, if one is active. The selection is
    /// everything between this and the caret, in either direction.
    anchor: Option<usize>,
}

impl TextBox {
    /// Open on `text` with all of it selected — clicking a value field and
    /// typing should replace the number, not append to it.
    pub fn selecting_all(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            caret: text.len(),
            anchor: Some(0),
            text,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    /// The selected range as `(start, end)` byte offsets, if any covers
    /// more than nothing.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let a = self.anchor?;
        let (lo, hi) = (a.min(self.caret), a.max(self.caret));
        (lo < hi).then_some((lo, hi))
    }

    /// Drop the selection, leaving the caret where it is.
    fn deselect(&mut self) {
        self.anchor = None;
    }

    /// Remove the selection's contents, if any. Returns whether it removed
    /// anything — insert and backspace both lead with this.
    fn delete_selection(&mut self) -> bool {
        let Some((lo, hi)) = self.selection() else {
            self.deselect();
            return false;
        };
        self.text.replace_range(lo..hi, "");
        self.caret = lo;
        self.deselect();
        true
    }

    /// Type a character. Whatever is selected is replaced by it.
    pub fn insert(&mut self, c: char) {
        self.delete_selection();
        self.text.insert(self.caret, c);
        self.caret += c.len_utf8();
    }

    /// Backspace: the selection if there is one, otherwise the character
    /// before the caret.
    pub fn backspace(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        let Some(prev) = self.prev_boundary(self.caret) else {
            return false;
        };
        self.text.replace_range(prev..self.caret, "");
        self.caret = prev;
        true
    }

    /// Delete forward.
    pub fn delete(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        let Some(next) = self.next_boundary(self.caret) else {
            return false;
        };
        self.text.replace_range(self.caret..next, "");
        true
    }

    /// Move the caret one character. With `select`, the selection extends
    /// rather than collapsing — Shift+arrow.
    pub fn step(&mut self, forward: bool, select: bool) {
        let to = if forward {
            self.next_boundary(self.caret)
        } else {
            self.prev_boundary(self.caret)
        };
        // Without Shift, an arrow over a selection collapses to its edge
        // rather than moving from the caret — what every text field does.
        if !select && let Some((lo, hi)) = self.selection() {
            self.caret = if forward { hi } else { lo };
            self.deselect();
            return;
        }
        self.set_caret(to.unwrap_or(self.caret), select);
    }

    pub fn home(&mut self, select: bool) {
        self.set_caret(0, select);
    }

    pub fn end(&mut self, select: bool) {
        self.set_caret(self.text.len(), select);
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.text.len();
    }

    fn set_caret(&mut self, to: usize, select: bool) {
        if select {
            // Start anchoring from where the caret was, so the first
            // Shift+arrow selects the character it just crossed.
            self.anchor.get_or_insert(self.caret);
        } else {
            self.deselect();
        }
        self.caret = to;
    }

    fn prev_boundary(&self, from: usize) -> Option<usize> {
        (from > 0).then(|| {
            let mut i = from - 1;
            while !self.text.is_char_boundary(i) {
                i -= 1;
            }
            i
        })
    }

    fn next_boundary(&self, from: usize) -> Option<usize> {
        (from < self.text.len()).then(|| {
            let mut i = from + 1;
            while !self.text.is_char_boundary(i) {
                i += 1;
            }
            i
        })
    }
}

/// The selection wash and the caret bar for a field being typed into.
///
/// Both need to know how wide the text before them is, so this takes a
/// measuring closure rather than a font — the caller owns the text engine.
/// Drawn in the UI pass, under the glyphs the text pass lays on top.
pub fn caret_rects(
    box_rect: spark_render::Viewport,
    tb: &TextBox,
    pad: f32,
    line_h: f32,
    mut measure: impl FnMut(&str) -> f32,
) -> Vec<spark_ui::UiRect> {
    let th = spark_ui::theme();
    let x0 = box_rect.x + pad;
    let y = box_rect.y + (box_rect.h - line_h) * 0.5;
    let mut at = |i: usize| x0 + measure(&tb.text()[..i]);
    let mut out = Vec::new();
    if let Some((lo, hi)) = tb.selection() {
        let (a, b) = (at(lo), at(hi));
        out.push(spark_ui::UiRect::region(
            spark_render::Viewport {
                x: a,
                y,
                w: (b - a).max(1.0),
                h: line_h,
            },
            th.accent_alt_bg,
        ));
    }
    // The caret is always drawn, selection or not — it's where the next
    // character lands, which is the thing a selection alone doesn't say.
    out.push(spark_ui::UiRect::region(
        spark_render::Viewport {
            x: at(tb.caret()),
            y,
            w: (2.0 * (line_h / 18.0)).max(1.5),
            h: line_h,
        },
        th.accent,
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(s: &str) -> TextBox {
        let mut b = TextBox::selecting_all(s);
        b.end(false);
        b
    }

    /// The complaint, in a test: click a field and type, and the number is
    /// replaced. It used to append, so setting X to 5 on a field reading
    /// 960 gave you 9605 unless you backspaced three times first.
    #[test]
    fn typing_replaces_the_whole_value() {
        let mut b = TextBox::selecting_all("960");
        assert_eq!(b.selection(), Some((0, 3)), "opens with everything picked");
        b.insert('5');
        assert_eq!(b.text(), "5");
        assert_eq!(b.caret(), 1);
        assert_eq!(b.selection(), None, "the selection is spent");
    }

    /// Backspace over a selection removes the selection, not one character
    /// before it.
    #[test]
    fn backspace_eats_the_selection_first() {
        let mut b = TextBox::selecting_all("960");
        assert!(b.backspace());
        assert_eq!(b.text(), "");
        // ...and then behaves normally.
        let mut b = typed("960");
        assert!(b.backspace());
        assert_eq!(b.text(), "96");
        assert_eq!(b.caret(), 2);
    }

    /// Backspace at the start has nothing to take, and says so rather than
    /// reporting a change that would force a pointless redraw.
    #[test]
    fn backspace_at_the_start_does_nothing() {
        let mut b = typed("9");
        b.home(false);
        assert!(!b.backspace());
        assert_eq!(b.text(), "9");
    }

    /// The caret moves, and stops at both ends.
    #[test]
    fn the_caret_walks_the_text_and_stops_at_the_ends() {
        let mut b = typed("123");
        assert_eq!(b.caret(), 3);
        b.step(false, false);
        assert_eq!(b.caret(), 2);
        b.home(false);
        assert_eq!(b.caret(), 0);
        b.step(false, false);
        assert_eq!(b.caret(), 0, "walked off the front");
        b.end(false);
        b.step(true, false);
        assert_eq!(b.caret(), 3, "walked off the back");
    }

    /// Typing lands at the caret, not at the end.
    #[test]
    fn typing_inserts_where_the_caret_is() {
        let mut b = typed("13");
        b.step(false, false);
        b.insert('2');
        assert_eq!(b.text(), "123");
        assert_eq!(b.caret(), 2);
    }

    /// Shift+arrow grows a selection from where the caret was; a plain
    /// arrow over one collapses to its edge instead of moving.
    #[test]
    fn shift_selects_and_a_plain_arrow_collapses() {
        let mut b = typed("1234");
        b.step(false, true);
        b.step(false, true);
        assert_eq!(b.selection(), Some((2, 4)));
        b.step(false, false);
        assert_eq!(b.caret(), 2, "collapsed to the selection's left edge");
        assert_eq!(b.selection(), None);
        b.end(true);
        assert_eq!(b.selection(), Some((2, 4)));
        b.step(true, false);
        assert_eq!(b.caret(), 4, "collapsed to the right edge");
    }

    /// Every caret position is a char boundary, so slicing the buffer to
    /// measure the text before the caret can never panic.
    #[test]
    fn the_caret_never_splits_a_character() {
        let mut b = TextBox::selecting_all("−4°ø");
        b.home(false);
        for _ in 0..10 {
            assert!(b.text().is_char_boundary(b.caret()), "split a char");
            b.step(true, false);
        }
        for _ in 0..10 {
            assert!(b.text().is_char_boundary(b.caret()), "split a char");
            b.step(false, false);
        }
        // And deleting through multi-byte text stays valid.
        b.end(false);
        while b.backspace() {
            assert!(b.text().is_char_boundary(b.caret()));
        }
        assert_eq!(b.text(), "");
    }

    #[test]
    fn select_all_then_delete_clears_it() {
        let mut b = typed("1234");
        b.select_all();
        assert!(b.delete());
        assert_eq!(b.text(), "");
    }
}
