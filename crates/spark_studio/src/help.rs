//! The startup banner: every gesture the editor understands, printed to the
//! terminal until the in-app help panel lands. Split from `main` so the
//! event plumbing isn't buried under fifty lines of prose.

pub(crate) fn banner() {
    println!(
        "\nSpark Studio — comp editor v0 (status prints here until in-app UI lands)\n\
     \n\
     Tools:  1 select/move   2 circle   3 box   4 polygon   5 line\n\
     Draw:   click-drag in the viewport\n\
     Edit:   drag move | scroll scale | Shift+scroll or Q/E rotate\n\
             [ ] polygon sides | C color | T outline/fill\n\
             A/Z glow +/- | W/S brightness +/- | X or Del delete\n\
     Paths:  P make editable | drag points | = add point | - remove | O open/close\n\
     Layers: click a row to select | Shift+click a range | Ctrl+click toggles one\n\
             drag rows to reorder the stack | Ctrl+D duplicate\n\
     Folder: Ctrl+Shift+N puts the selected layers in a folder | +/- collapses\n\
             X/Y/R/S on the header moves everything inside, about its own center\n\
             drag the header to reorder the whole run | K keys the folder too\n\
             click the header to select its contents | double-click renames\n\
             the folder eye hides everything inside | right-click dissolves it\n\
             drag a card onto a header to file it; onto a loose card to pull it out\n\
     Merge:  Ctrl+G merges the selection into one layer (colors + keys kept)\n\
             Ctrl+Shift+G unmerges | File > Save/Import Shape... reuses selections\n\
     Anim:   the timeline is always there — a comp keeps its own clock (120 BPM,\n\
             2 min) until a track is imported, so you can choreograph first;\n\
             space/play runs it on wall time until a song takes over the clock\n\
             K or the diamond button keys what you changed since the last stamp\n\
             (first K on a shape poses it; K with nothing changed holds it still)\n\
             the terminal says which properties each stamp landed on\n\
             posing without stamping is a preview — it reverts when the playhead moves\n\
             folders key too — their lane sits above its members in Keys\n\
             drag keys to retime (16th grid) | Alt+drag copies | right-click deletes\n\
             Ctrl+drag empty lane space box-selects keys | Shift+click adds/removes\n\
             Ctrl+C copies selected keys | Ctrl+V pastes at playhead\n\
             Ctrl+Shift+V repeat-pastes bar-aligned (to loop end, else x4)\n\
             arrows jump playhead between keys | , . nudge selected keys a 16th\n\
             Ctrl+click a key: smooth (diamond) <-> linear (square)\n\
     Loop:   Shift+drag the ruler brackets bars | L toggles | right-click clears\n\
     View:   Ctrl+wheel zoom at cursor | Shift+wheel pan | wheel scrolls lanes\n\
     Canvas: Ctrl+wheel zoom at cursor | middle-drag pan | Ctrl+0 back to 100%\n\
             zoom bar bottom-right: - + steppers, 100% refit, live readout\n\
     Cards:  each layer card owns its shape: drag X/Y/R/S up/down to scrub,\n\
             click one to type the value (Enter commits, Esc cancels)\n\
             eye toggles visibility | cogwheel expands full settings\n\
     Color:  the color home is the *current color* — swatches, picker, hex\n\
             selecting a layer never changes it; Alt+click a shape or I eyedrops\n\
             with a selection, editing the color paints it too | C cycles palette\n\
     React:  a lane's cog opens sliders for how hard that shape rides the track\n\
             reaction is evaluated at the playhead, parked or playing\n\
     Undo:   Ctrl+Z undo | Ctrl+Shift+Z redo\n\
     Comp:   every session opens on a blank untitled comp — Ctrl+O opens one\n\
             File > New for a blank project | Ctrl+S save\n\
     Layout: drag the toolbar's top edge to resize the bottom panel; double-click resets\n\
             three square tab buttons: wave (teal), arrange (red), keys (gold)\n\
             the red grid button snaps the playhead to quarter-bars\n\
             Keys tab: hero Keyframe button in the sidebar; a lane's cog opens its\n\
             React sliders right there in the row\n\
     Misc:   Esc deselect | Ctrl+Q quit\n"
    );
}
