//! The startup banner: every gesture the editor understands, printed to the
//! terminal until the in-app help panel lands. Split from `main` so the
//! event plumbing isn't buried under fifty lines of prose.

pub(crate) fn banner() {
    println!(
        "\nSpark Studio — the object/clip era: an object is an instrument,\n\
     a clip is when it plays. Side panels are shells awaiting the\n\
     inspector; keyboard + canvas + timeline carry everything.\n\
     \n\
     Tools:  1 select/move   2 circle   3 box   4 polygon   5 line   6 stars\n\
             7 lightning   8 vortex\n\
             or RIGHT-CLICK the viewport: the context menu, with the tool\n\
             rail down its left — click a tool to arm it (again for Move)\n\
     Menu:   an armed tool's page is its DRAW DEFAULTS — what the next\n\
             shape is born as: Fill|Outline, Thickness, Glow, Brightness,\n\
             Sides; a star field's Density, Size, Glow, Twinkle, Rate, form;\n\
             lightning's Jag, Forks, Strike (React on Onset = bolts on hits)\n\
             sliders: press or drag the band, wheel steps (Shift = fine)\n\
             with Move armed the page is HOME for what you right-clicked:\n\
             a shape — Copy, Paste, Duplicate, Delete; empty space — nothing yet\n\
     Copy:   Ctrl+C / Ctrl+V copy and paste whole objects — a paste lands on\n\
             the cursor, its clips at the playhead | Ctrl+Shift+C/V the look only\n\
     Panel:  the right panel is the INSPECTOR — the C O L O R section on top\n\
             (click its header to fold it; gold is the colour you start with):\n\
             foreground/background swatches (left-click opens the picker\n\
             popup: HSV + hex + R G B, typeable; right-click swaps them) and\n\
             the swatch grid (left-click = foreground, paints the selection;\n\
             right-click = background, paints a gradient's far end)\n\
             below the rule: the object's name in a box (click to rename,\n\
             Enter commits, empty = auto-label), then SECTIONS that fold under\n\
             their gold headers — TRANSFORM: scrub fields (drag up/down, Shift\n\
             = fine, click to type, Enter commits, Esc lets go; captions run\n\
             R G B); STYLE (LIGHT on a light): Fill|Outline / star form / light\n\
             kind, sliders — Sides, Opacity, Brightness, Thickness, Glow —\n\
             Additive; one section per added effect: Enabled, its settings,\n\
             Remove in red | wheel scrolls the body\n\
     Left:   the left panel's tabs — EFFECTS lists every effect you can add\n\
             (Gradient; Glow lives in Style): DRAG a row onto a shape on\n\
             the canvas, its row on the timeline, or the inspector (adds to\n\
             the selection — no aiming) to add it\n\
     React:  RIGHT-CLICK any field or slider in the inspector: React on,\n\
             pick the trigger (Bass Low Mid High Onset Loud), set Intensity,\n\
             close — a gold dot marks a reacting setting; per setting, any\n\
             setting, stacked on top of its keyframes; always song time\n\
     Draw:   click-drag in the viewport — the object is born with a 1-bar\n\
             clip at the playhead; it exists only where its clips are\n\
     Edit:   drag move | Q/E rotate | [ ] polygon sides | C cycles palette\n\
             T outline/fill | A/Z glow +/- | W/S brightness +/- | X or Del delete\n\
             Alt+click a shape or I eyedrops its color\n\
     Paths:  P make editable | drag points | = add point | - remove | O open/close\n\
     Tracks: every object is a track row — click its name to select it,\n\
             the eye hides it, folders collapse with their triangle\n\
             the song's row is pinned on top; object rows run in draw order:\n\
             first drawn first, a new object lands at the BOTTOM (and in\n\
             front); DRAG a row's head up/down to reorder — lower draws in front\n\
             Ctrl+Shift+N folders the selection | Ctrl+G merges | Ctrl+D duplicates\n\
     Clips:  CLICK anywhere on a clip to put the playhead there (the grid\n\
             scrubs everywhere, not just the ruler); drag the body to move\n\
             (its own track only), edges to trim (left trim eats content,\n\
             Ableton-style) | Del removes\n\
             L toggles the selected clip's loop | Ctrl+D duplicates it flush\n\
             loop seams tick inside the bar; clip bars wear the object's color\n\
             double-click a comp clip to edit its comp (status-bar name = back)\n\
             double-click an OBJECT clip: its CURVE VIEW takes the panel —\n\
             rows = the clip's keyed settings, plus any field or slider you\n\
             TOUCH IN THE INSPECTOR while the view is open (that's how you\n\
             pick what to keyframe; dim until keyed — double-click the graph\n\
             plants its first key); click a row to see its curve; drag a\n\
             diamond to move a key in time + value; the KEY STRIP under the\n\
             ruler retimes every key at a moment together; double-click the\n\
             graph adds a key on the line; Del removes the pick (or an\n\
             unkeyed row); right-click a key flips smooth/linear; drag the\n\
             gold LOOP BRACE's end on the ruler to set how much repeats (a\n\
             stretched clip keeps its whole-clip loop until you shorten it);\n\
             past the loop is washed dark — it never plays; the ruler scrubs\n\
             the song through the clip; Esc or the ‹ plate = back\n\
     Anim:   K or the diamond stamps what you changed into the ACTIVE CLIP\n\
             at clip-local time (on the arrangement: first K poses; K\n\
             unchanged holds still | in the CLIP VIEW: K keys the settings\n\
             you listed, moved or not, and volunteers nothing; with a KEY\n\
             PICKED, K updates that key to the value as it stands)\n\
             keys are LINEAR by default — right-click one to make it smooth\n\
             posing without stamping is a preview — it reverts on playhead move\n\
             keys loop with the clip; audio-react always reads song time\n\
             (retime, add and delete keys in the clip view — above)\n\
     Loop:   Shift+drag the ruler brackets bars | L toggles | right-click clears\n\
     View:   Ctrl+wheel zoom at cursor | Shift+wheel pan | wheel scrolls tracks\n\
     Canvas: Ctrl+wheel zoom at cursor | middle-drag pan | Ctrl+0 back to 100%\n\
             zoom cluster at the toolbar's right: - + steppers, 100% refit\n\
     Fly:    Tab toggles the fly view | drag empty space to look around\n\
             WASD fly, Q/E down/up, Shift sprints | wheel forward/back\n\
             right- or middle-drag pans | R flips the gizmo: Move / Rotate\n\
     Add:    Sun / Point / Spot / Ambient lights | Plane / Cube / Sphere meshes\n\
             all born with a 1-bar clip, like anything else\n\
     Undo:   Ctrl+Z undo | Ctrl+Shift+Z redo\n\
     Comp:   every session opens on a blank untitled comp — Ctrl+O opens one\n\
             File > New for a blank project | Ctrl+S save (format v2 —\n\
             pre-clip files open shapes-only, by design)\n\
     Layout: drag the toolbar's top edge to resize the bottom panel;\n\
             double-click resets | the red grid button snaps the playhead\n\
     Misc:   Esc closes the menu / deselects | Ctrl+Q quit\n"
    );
}
