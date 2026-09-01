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
             (keyboard only until the context-menu tools land)\n\
     Draw:   click-drag in the viewport — the object is born with a 1-bar\n\
             clip at the playhead; it exists only where its clips are\n\
     Edit:   drag move | Q/E rotate | [ ] polygon sides | C cycles palette\n\
             T outline/fill | A/Z glow +/- | W/S brightness +/- | X or Del delete\n\
             Alt+click a shape or I eyedrops its color\n\
     Paths:  P make editable | drag points | = add point | - remove | O open/close\n\
     Tracks: every object is a track row — click its name to select it,\n\
             the eye hides it, folders collapse with their triangle\n\
             Ctrl+Shift+N folders the selection | Ctrl+G merges | Ctrl+D duplicates\n\
     Clips:  drag the body to move (its own track only), edges to trim\n\
             (left trim eats content, Ableton-style) | Del removes\n\
             L toggles the selected clip's loop | Ctrl+D duplicates it flush\n\
             loop seams tick inside the bar; clip bars wear the object's color\n\
             double-click a comp clip to edit its comp (status-bar name = back)\n\
     Anim:   K or the diamond stamps what you changed into the ACTIVE CLIP\n\
             at clip-local time (first K poses; K unchanged holds still)\n\
             posing without stamping is a preview — it reverts on playhead move\n\
             keys loop with the clip; audio-react always reads song time\n\
             (key retime/copy arrives with the clip view — stamp over to redo)\n\
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
     Misc:   right-click the viewport/panels: context menu — click a tool\n\
             to arm it (menu stays open; click it again for Move + home)\n\
             Esc deselect | Ctrl+Q quit\n"
    );
}
