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

    /// Put the caret at a byte offset, dropping any selection — a click.
    pub fn place(&mut self, byte: usize) {
        self.caret = self.clamp_boundary(byte);
        self.deselect();
    }

    /// Extend the selection to a byte offset — a drag. Anchors where the
    /// caret was when the drag began.
    pub fn drag_to(&mut self, byte: usize) {
        let to = self.clamp_boundary(byte);
        self.anchor.get_or_insert(self.caret);
        self.caret = to;
    }

    /// Snap an arbitrary offset onto a char boundary inside the buffer, so
    /// a position derived from a pixel can never split a character.
    fn clamp_boundary(&self, byte: usize) -> usize {
        let mut i = byte.min(self.text.len());
        while !self.text.is_char_boundary(i) {
            i -= 1;
        }
        i
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

/// The byte offset whose x is nearest `x` — a pixel turned into a caret.
/// `xs` is the boundary table [`crate::Studio::field_caret_xs`] caches.
pub fn index_at(xs: &[(usize, f32)], x: f32) -> usize {
    xs.iter()
        .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
        .map(|&(i, _)| i)
        .unwrap_or(0)
}

/// Every char boundary's x, for a field drawn at `x0` in `size`.
pub fn boundaries(text: &str, x0: f32, mut measure: impl FnMut(&str) -> f32) -> Vec<(usize, f32)> {
    let mut out = vec![(0, x0)];
    for (i, _) in text.char_indices().skip(1) {
        out.push((i, x0 + measure(&text[..i])));
    }
    // The end is a caret position too, and on an empty buffer it *is* the
    // start — pushing it unconditionally would duplicate the only entry.
    if !text.is_empty() {
        out.push((text.len(), x0 + measure(text)));
    }
    out
}

/// The selection wash and the caret bar for a field being typed into.
///
/// Both need to know how wide the text before them is, so this takes a
/// measuring closure rather than a font — the caller owns the text engine.
/// Drawn in the UI pass, under the glyphs the text pass lays on top.
pub fn caret_rects(
    xs: &[(usize, f32)],
    box_rect: spark_render::Viewport,
    tb: &TextBox,
    line_h: f32,
) -> Vec<spark_ui::UiRect> {
    let th = spark_ui::theme();
    let y = box_rect.y + (box_rect.h - line_h) * 0.5;
    let at = |i: usize| {
        xs.iter()
            .find(|&&(b, _)| b == i)
            .map(|&(_, x)| x)
            .unwrap_or(box_rect.x)
    };
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
            th.accent_bg,
        ));
    }
    // The caret draws whether or not anything is selected — it's where the
    // next character lands, which a selection alone doesn't say.
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

    /// A click places the caret; a drag from there selects. Both come from
    /// pixel positions, so both have to land on char boundaries.
    #[test]
    fn clicking_places_and_dragging_selects() {
        let mut b = typed("1234");
        b.place(2);
        assert_eq!(b.caret(), 2);
        assert_eq!(b.selection(), None, "a click clears the selection");
        b.drag_to(4);
        assert_eq!(b.selection(), Some((2, 4)));
        b.drag_to(0);
        assert_eq!(b.selection(), Some((0, 2)), "dragging back past the anchor");
    }

    /// A position derived from a pixel can land anywhere; it must never
    /// split a character or run off the end.
    #[test]
    fn a_pixel_position_is_snapped_into_the_buffer() {
        let mut b = TextBox::selecting_all("−4°");
        for byte in 0..20 {
            b.place(byte);
            assert!(b.text().is_char_boundary(b.caret()), "split at {byte}");
            assert!(b.caret() <= b.text().len());
        }
    }

    /// A pixel maps to the nearest boundary, and never off either end.
    #[test]
    fn a_pixel_maps_to_the_nearest_boundary() {
        // Three characters, ten px apart.
        let xs = [(0usize, 100.0f32), (1, 110.0), (2, 120.0), (3, 130.0)];
        assert_eq!(index_at(&xs, 0.0), 0, "before the start");
        assert_eq!(index_at(&xs, 104.0), 0, "rounds to the nearer edge");
        assert_eq!(index_at(&xs, 106.0), 1);
        assert_eq!(index_at(&xs, 999.0), 3, "past the end");
    }

    /// The boundary table covers every caret position, start and end
    /// included — a click past the last glyph has to land somewhere.
    #[test]
    fn the_boundary_table_covers_both_ends() {
        // One unit per byte, so offsets are readable.
        let xs = boundaries("abc", 0.0, |s| s.len() as f32);
        assert_eq!(xs, vec![(0, 0.0), (1, 1.0), (2, 2.0), (3, 3.0)]);
        assert_eq!(boundaries("", 5.0, |_| 0.0), vec![(0, 5.0)]);
    }

    #[test]
    fn select_all_then_delete_clears_it() {
        let mut b = typed("1234");
        b.select_all();
        assert!(b.delete());
        assert_eq!(b.text(), "");
    }
}
