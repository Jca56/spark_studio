//! Driving the playground: what a click lands on, what a drag moves, and
//! what a typed hex code does.
//!
//! Colors apply the moment the buffer parses, so a code takes effect on the
//! sixth character rather than on Enter — the whole point is watching the
//! editor change while you type.

use spark_ui::{Slider, Surfaces, from_hex, hex_of, set_surfaces, set_theme, theme};

use super::{Edit, KNOBS, MATERIALS, State, Tab, build, get, nth, nth_mut, recipe, set};
use crate::Studio;

/// A track is thin; its grab box is fattened so it's reachable.
fn on_track(track: spark_render::Viewport, cx: f32, cy: f32, scale: f32) -> bool {
    let grow = 10.0 * scale;
    cx >= track.x
        && cx <= track.x + track.w
        && cy >= track.y - grow
        && cy <= track.y + track.h + grow
}

impl Studio {
    pub(crate) fn material_state(&self) -> State {
        State {
            tab: self.material_tab,
            pick: self.material_pick,
            editing: self.material_edit.clone(),
        }
    }

    pub(crate) fn press_materials(&mut self, cx: f32, cy: f32) {
        let Some(layout) = self.layout() else { return };
        let scale = self.scale();
        let panel = build(layout.timeline, scale, &self.material_state());

        // Any click outside the field being typed into commits it, the same
        // way a scrub field behaves.
        self.material_edit = None;

        if let Some(i) = panel.tabs.iter().position(|v| v.contains(cx, cy)) {
            self.material_tab = super::TABS[i].0;
            self.request_redraw();
            return;
        }
        if panel.print.contains(cx, cy) {
            self.print_recipe();
            return;
        }
        if panel.reset.contains(cx, cy) {
            set_theme(spark_ui::default_theme());
            self.request_redraw();
            return;
        }
        // Colour fields exist on both tabs now, so this comes before the
        // tab-specific controls rather than inside one of them.
        if let Some(c) = panel.cells.iter().find(|c| c.rect.contains(cx, cy)) {
            // Start from the code it already reads as, so a tweak is an
            // edit rather than a retype.
            self.material_edit = Some((c.edit, hex_of(c.color)));
            // And hand this colour to the right panel's picker, opened on
            // it. Typing a code is a fine way to reach an exact shade and a
            // useless way to *find* one; nobody knows codes by heart.
            self.material_target = Some(c.edit);
            self.picker_hsv = Some(crate::input::hsv_of_linear([
                c.color[0], c.color[1], c.color[2],
            ]));
            self.request_redraw();
            return;
        }
        match panel.tab {
            Tab::Colors => {}
            Tab::Depth => {
                if let Some(i) = panel.picks.iter().position(|v| v.contains(cx, cy)) {
                    self.material_pick = i;
                    self.request_redraw();
                    return;
                }
                if let Some(row) = panel.rows.iter().find(|r| on_track(r.track, cx, cy, scale)) {
                    self.material_drag = Some(row.knob);
                    self.drag_material(cx);
                    self.request_redraw();
                }
            }
        }
    }

    /// Move the knob under the cursor. Returns whether anything changed.
    pub(crate) fn drag_material(&mut self, cx: f32) -> bool {
        let Some(knob) = self.material_drag else {
            return false;
        };
        let Some(layout) = self.layout() else {
            return false;
        };
        let panel = build(layout.timeline, self.scale(), &self.material_state());
        let Some(row) = panel.rows.iter().find(|r| r.knob == knob) else {
            self.material_drag = None;
            return false;
        };
        let Some(&(_, _, _, max)) = KNOBS.iter().find(|(k, ..)| *k == knob) else {
            return false;
        };
        let mut live = surfaces_now();
        set(
            nth_mut(&mut live, self.material_pick),
            knob,
            Slider::t_at(row.track, cx) * max,
        );
        set_surfaces(live);
        true
    }

    /// Keyboard while a hex code is being typed. Returns whether the frame
    /// needs redrawing.
    pub(crate) fn material_key(&mut self, key: &winit::keyboard::Key) -> bool {
        use winit::keyboard::{Key, NamedKey};
        let Some((edit, buf)) = &mut self.material_edit else {
            return false;
        };
        match key {
            Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Enter) => {
                self.material_edit = None;
                true
            }
            Key::Named(NamedKey::Backspace) => {
                let changed = buf.pop().is_some();
                let (edit, buf) = (*edit, buf.clone());
                self.apply_hex(edit, &buf);
                changed
            }
            Key::Character(s) => {
                let mut dirty = false;
                for c in s.chars() {
                    // Eight, not six: the last two digits are the alpha.
                    if buf.len() < 8 && c.is_ascii_hexdigit() {
                        buf.push(c.to_ascii_uppercase());
                        dirty = true;
                    }
                }
                let (edit, buf) = (*edit, buf.clone());
                self.apply_hex(edit, &buf);
                dirty
            }
            _ => false,
        }
    }

    /// Push a typed code into the palette if it parses. A half-typed code
    /// simply doesn't apply yet — nothing flashes, nothing resets.
    fn apply_hex(&mut self, edit: Edit, buf: &str) {
        let Some(color) = from_hex(buf) else { return };
        super::set_color(edit, self.material_pick, color);
        // The picker is showing this colour; a typed code has to move it or
        // the square would drift away from the value it claims to be on.
        self.sync_picker();
    }

    /// Write the recipe beside the comp file, and echo it. The file is the
    /// reliable half — launched from a desktop entry there may be no
    /// terminal to print to.
    fn print_recipe(&mut self) {
        let body = recipe(&theme(), &surfaces_now());
        let path = std::path::Path::new(&self.current_file)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_path_buf())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default()
            .join("spark_materials.txt");
        match std::fs::write(&path, &body) {
            Ok(()) => println!("materials -> {}\n{body}", path.display()),
            Err(e) => println!(
                "materials: could not write {} ({e})\n{body}",
                path.display()
            ),
        }
        self.request_redraw();
    }
}

fn surfaces_now() -> Surfaces {
    spark_ui::surfaces()
}

/// Copy every knob from `old` onto the freshly rederived materials, so a
/// recolor keeps the depth and a depth pass keeps the colors.
pub(super) fn carry_depth(old: &Surfaces) {
    let mut fresh = surfaces_now();
    for i in 0..MATERIALS.len() {
        let was = nth(old, i);
        let now = nth_mut(&mut fresh, i);
        for (knob, ..) in KNOBS {
            set(now, knob, get(&was, knob));
        }
        // Not a knob: the gradient's far end is a colour somebody chose, so
        // a recolour of the *fill* carries it across untouched rather than
        // recomputing it into something nobody asked for.
        now.fill_to = was.fill_to;
    }
    set_surfaces(fresh);
}
