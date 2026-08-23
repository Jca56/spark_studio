# Spark Studio — Design

An animation video editor for creating animated music videos (EDM / Dubstep / Riddim).
Rust + WGPU. The editor is drawn by the engine itself — every pixel of UI comes
from our own renderer.

## The core idea: everything is a function of time

The song is fixed. The timeline is fixed. Nothing changes at render time — so
every rendered frame is a pure function:

```
frame = render(project, t)
```

This single decision buys us:

- **Perfect scrubbing** — jump to any `t`, get exactly that frame. No catch-up,
  no state drift.
- **Perfect audio sync** — the drop hits on the exact frame, every render.
- **Offline rendering** — final export renders at any resolution/framerate,
  slower or faster than realtime, and is bit-identical every time.
- **Simple undo/redo** — the document is data; rendering never mutates it.

Anything stateful-looking (particles, trails) must be seeded + deterministic so
that `render(project, t)` stays pure. (Particle systems simulate from their clip
start on seek, or use closed-form/stateless formulations where possible.)

## Audio: analyzed offline, baked, scrubbable

Live visualizers guess at the music as it happens. We don't have to — we have
the whole track up front. On import, the track is decoded (FFmpeg subprocess →
raw PCM) and analyzed once into **analysis curves**, cached on disk:

- Band energies (bass / low-mid / mid / high — tuned for kick, sub, snare, leads)
- Onset / transient strength (riddim wobble chops want this)
- Beat grid + BPM estimate (user-correctable)
- RMS / loudness envelope
- Waveform peaks (for timeline drawing)

These curves are first-class citizens: any parameter of any effect can be driven
by a keyframe curve, an analysis curve, or an expression combining them.
"Scale = 1.0 + bass × 0.5" is the hello-world of this app.

Curves are read at the **centre** of their FFT window, not its start
(`CURVE_LAG`): sample `h` describes 2048 samples beginning at `h·HOP`, and
labelling it with the start made every curve read ~21 ms early. The first
frame's spectral flux is forced to zero — with no previous frame its "rise"
is the whole spectrum arriving out of silence, which flashed every reactive
shape at `t=0`.

The beat grid answers two separate questions (2026-08-17, after the grid
came out a beat off Alva's own track). **How fast** is autocorrelation of
the onset curve, mean-subtracted so the periodic ripple isn't buried under
the square of the mean, with a parabola fitted through the winning lag and
its neighbours — whole-hop lags can only express ~3 BPM steps around 140,
and half a percent of tempo error walks the grid a full beat off across two
minutes. The result snaps to a whole BPM when it lands within 0.4 of one:
these tracks were typed into a DAW, so 140.6 is evidence of a 140 BPM song
measured through a finite window, not of a 140.6 BPM song. **Where bar one
starts** cannot come from the same search, because the loudest recurring
transient in EDM is the snare — a single comb for "the biggest repeating
hit" lands confidently on beat two, which is a quarter bar out and was
exactly the bug. So the beat phase is locked from the onsets (any beat will
do, they're evenly spaced) and then the downbeat is chosen among the four by
combing the **bass** band, where a kick dominates and a snare doesn't.
Finally the phase walks back to the earliest bar line, since a track that
opens on the downbeat was otherwise losing its whole first bar. Synthetic
click-track tests hold it: a 140 BPM pattern with a *louder* snare on two
and four must still put bar one on the kick.

**A comp keeps time before it has a song** (2026-08-17). The Keys tab,
the ruler, the lanes and the playhead all used to live inside `if let
Some(track) = &self.audio` — no track meant no timeline, so nothing could
be animated until a file had been imported and analyzed. The clock is now
`Studio::grid` / `Studio::duration`: a loaded track owns it, and without
one the comp runs at 120 BPM for two minutes. Choreography can start on a
blank comp and the track can arrive afterwards. The waveform is the one
tab that genuinely needs a song.

That clock has to *run*, not just exist. With audio the transport clock
**is** the audio callback's cursor, so picture and sound can't drift;
with no audio there's no cursor to read, so playback runs on wall time
(`SilentClock`: where the playhead was, plus how long since play was
pressed). Every seek re-anchors it, or scrubbing mid-playback would snap
back on the next frame. A cycling loop wraps by its own *length* rather
than snapping to its start, so a long frame carries its overshoot across
instead of nudging the choreography off the grid — the decision half is
`transport::tick`, kept pure precisely so it can be tested without a
window.

**Spark opens on a blank page.** It used to reload `comp.spark` from the
working directory at startup, which quietly made whichever comp sat there
the home project — and made an unsaved session one Ctrl+S away from
overwriting it. Every session now starts on an untitled comp; File > Open
picks the one you meant.

None of which beats being told. The transport carries a **tempo field**
left of the play button: click it, type the number, Enter. It overrides
detection, rides the comp file as a `bpm` line so the correction is made
once rather than every session, and leaves the downbeat alone — the phase
came from the audio and retyping the tempo is no reason to discard it.
Detection stays the opening guess; the person who made the track is the
authority.

## Document model: Timeline + Comps

- A **Project** owns assets (audio track, later: meshes, images) and one master
  **Timeline** locked to the song.
- A **Comp** (composition) is an ordered stack of layers rendered
  back-to-front into one image, with its own parameter set, coordinate space,
  and duration. Rendering a comp is pure: comp × time → frame.
- The timeline holds **tracks** of **clips**; a clip instances a comp over a
  time range, mapping timeline time onto the comp's local time.
- **A stamp keys what changed, not the whole shape** (2026-08-17). `K`
  used to write every applicable property at once, so one keyframe froze
  a shape forever: from that moment the curves drove its glow, sides,
  thickness and everything else too, and posing by hand could only ever
  preview. Rotation could never animate alone. Now `K` diffs the live
  pose against the baseline the curves posed the shape at — captured in
  `sync_to_time`, frozen the moment a preview pose begins — and keys only
  the difference. Three cases: nothing keyed yet lays down a *pose*
  (X/Y/Rotation/Scale); something moved keys exactly that; nothing moved
  **holds**, re-stamping what is already animated at its current value,
  because pressing `K` twice without touching anything is how you ask for
  stillness. Folder transforms follow the identical rule.

  A property earning its **first** key gets a **backfill**: a holding key
  at the owner's previous key time carrying the pre-edit value. Without
  it, turning the glow up at bar 5 and pressing `K` produces a single key
  — a flat line — and nothing ramps. After Effects gets this for free
  because its stopwatch stamps the starting key *before* you change the
  value; the backfill is our equivalent. With no earlier key there is
  nothing to ramp from and none is invented.

  Width/Height stay out of the first pose deliberately. `Scale` reads
  `b[0].max(b[1])`, the same extents Width and Height write, so
  stretching the longer axis moves Scale too and the diff catches the
  pair — which it must, or a flat Scale curve would squash the stretch
  back on playback.
- Anything animatable is driven by **curves** (keyframes with easing,
  stamped deliberately from the canvas pose — no auto-key) and/or by audio
  analysis curves. A smooth key is a cubic Hermite whose end slopes come
  from the keys on either side, so a run of keys reads as one continuous
  move; it used to smoothstep each segment on its own, which zeroes the
  velocity at both ends and made the shape brake to a stop at every key it
  passed. The first and last key keep flat tangents, so a plain two-key
  move still eases in and out, and a key the curve turns around at is
  flattened too — carrying speed through it would sail past the value that
  was stamped, and a stamped key is a promise about where the shape is.
- **Shapes have identity, not just a stack position.** Anything that
  outlives a frame and refers to a shape — a keyframe lane, the key
  selection, the expanded lane, the keyframe clipboard — names it by a
  stable id. `Owner::Shape` used to carry a stack index, so dragging a
  layer past another silently repointed a selected key at whatever shape
  had slid into that slot, and every reorder threw the key clipboard away
  because it could no longer be trusted. An id that no longer resolves is
  simply gone, which every key operation already handled by skipping it.
  Ids are session-local: the file format stores draw order, not identity,
  so a load hands out fresh ones.
- **Audio reaction is evaluated at the playhead**, not at a running
  player's clock. It was gated on `is_playing()`, so parking on the drop
  to tune a React amount showed a shape with no reaction on it at all —
  and a paused frame differed from the same frame in motion, which
  `frame = render(project, t)` says can never happen.
- **Rotation counts turns.** It is not an angle in (-π, π]: folding it made
  a continuous spin impossible, because the key past half a turn came back
  negative and the shape unwound counter-clockwise to reach it. Two turns
  is 720 and means it. (A *line*'s rotation is still derived from its
  endpoints, so it can't be keyed past a full turn — press `P` to convert
  it to a path first.)
- Transitions (cuts, crossfades, luma wipes) happen where clips meet/overlap.

Serialization is a hand-rolled human-readable text format (no serde). Projects
must diff cleanly in git.

## Comps & layers: canvas-first

Home base is direct manipulation: draw a shape, grab it, move it, pose it at
two moments, stamp a keyframe at each (the diamond button / `K`), and it
flies between them. Posing a keyed shape without stamping is a preview that
reverts when the playhead moves — keys never appear on their own. Build-order rule: **tools
before output** — every feature is proven by Alva using it in the editor,
never by hardcoded demo content. The engine core (curves, timeline, post
chain, export) never cares what a layer draws. Layer kinds arrive in this
order:

1. **Shape layers** (first) — hand-drawn glowing primitives: circle, box,
   regular polygon, line, path, star field. Fill + stroke + glow,
   SDF-rendered so the neon look is native, not a filter. Select/move/
   rotate/scale with the mouse; duplicate + repeaters for instant symmetry.
   Star fields blur the line into the next tier on purpose: one instance
   that draws hundreds of things is a generator wearing a shape layer's
   clothes, and it proved the shape pipeline can carry one.
2. **Generator layers** — procedural backdrops (liquid neon, plasma, glow
   fields) and raymarched flythroughs (SDF tunnels — 3D on screen, zero mesh
   code). Knobs, not brushes; seasoning behind the hand-made foreground.
3. **Footage layers** — imported video clips and images: Alva's own
   footage plus downloaded VFX assets and overlay packs. FFmpeg-decoded
   with a frame cache for scrubbing (keyframe seek + roll-forward); the
   post chain treats them like any other layer. This is also the seam that
   grows Spark toward general video editing.
4. **3D layers** (later, additive) — real camera + instanced geometry +
   particles + depth buffer.
5. **Rigged mesh layers** (much later) — imported meshes, skeletons,
   character animation.

The look that sells all of it — bloom, glow, DOF, grade — is the shared post
chain, and it works identically on every layer kind.

Glow is optional (2026-08-17). It wasn't: `set_glow` floored at 2, so no
shape could stop emitting, and worse, the halo was added *on top of* the
body rather than only outside it — the exponential is at full strength
across a shape's whole interior (`max(d, 0)` is zero everywhere inside), so
a fill rendered at **1.55× its own colour**, 2.17× at the old default
brightness. Saturated fills clipped their bright channels first and came
back pastel, which is why the only way to see the colour you picked was to
crush the brightness to nearly nothing, and why the usable part of that
slider was a sliver of its length. On a hairline stroke the overdrive read
as neon, so it hid there through every previous session. Now the halo
lights only what the body doesn't cover, `glow_at` returns nothing at radius
zero (an almost-zero radius still lit the fragments exactly on the boundary
— a bright rim on an edge meant to be hard), and **a shape at brightness 1.0
is exactly the colour you picked**. New shapes are born plain: no glow,
brightness 1.0. Glow is a thing you add, not a thing you spend the session
subtracting. Existing comps keep their saved glow radii but lose the
overdrive, so they read darker and *more* saturated than before. Pixel-
readback tests hold the line: a fill must come back as its own colour, glow
zero must leave nothing outside the silhouette, and glow turned up must
still spill.

## SparkUI

The engine draws its own editor. Build order: (1) flat rects mapping out the
layout, (2) container/grid layout framework, (3) reusable widget suite.
Text: external font rasterizer for now (fontdue-class); planned swap to
Alva's own lntrn-text once it matures.

Materials (rebuilt 2026-08-17): the chrome renderer used to draw exactly one
thing — a flat rounded rect in a single color — so every restyle could only
ever be "pick a different grey", and every border was faked by drawing a
bigger rect behind a smaller one (which is why they never hugged the corner
radius). One `UiRect` is now one **complete** piece of chrome, composited by
`ui.wgsl` in a single quad: drop shadow, fill, linear/radial gradient at any
angle, inner shadow, bevel, grain, and a **real** inset stroke that rides the
shape's own signed distance field — exactly N px thick around every corner.
Alignment picks whether a stroke sits inside the edge, outside it (a halo
that doesn't crop what it marks), or straddling, and `.dash(on, off, phase)`
breaks it into ticks that walk the real outline — slide the phase per frame
for marching ants. Shapes: per-corner radii (`.top_round()`, `.pill()`),
`.rotate(turns)`, `UiRect::line` for diagonals, and `UiRect::arc` / `::ring`
— ring segments swept clockwise from twelve o'clock, which is what knobs,
radial meters and circular progress are made of. On a line or an arc the
dashes break the *shape*, since those have no interior. Because every effect
derives from one silhouette function, icon glyphs get borders, glows,
gradients and rotation for free.

Instance data moved from vertex attributes to a **storage buffer** indexed by
`instance_index`: attributes cap at 16 slots / 60 inter-stage components,
which the material set would have hit immediately, so the ceiling is gone and
new material fields never touch the pipeline. Every parameter defaults to
zero and zero means off. The silhouette function branches per kind and the
shadow / inner-shadow / dash / image-sample work is guarded, so a panel fill
pays for a rounded box and nothing else — the branches are uniform across a
wave because one instance is one kind. `sdf.wgsl` (the distance-field
vocabulary) is concatenated ahead of `ui.wgsl` at build time, wgpu having no
`#include`. The pass is covered by offscreen render + pixel-readback tests:
if a border is meant to be 4px inside the edge, a test reads the pixels and
checks. Fake borders are banned — `.stroke()` is the only edge.

Palette (Alva's ladder, 2026-08-17): every chrome surface is drawn from one
eight-rung grey ladder — `0F0F19 · 151515 · 1B1B18 · 2A2A2A · 414141 ·
504E4E · 555555 · 888888` — with `151515` as the base the side panels sit at.
Things set deeper step down it (wells, the lane-name box), raised things step
up (cards, hover, borders). Two rungs lean very slightly blue and olive on
purpose: dead neutral grey all the way up reads as a rendering fault rather
than a decision. The viewport gutter keeps its deep purple, the one large area
deliberately outside the ladder. Text and icons keep their own contrast
rather than joining it, which tops out at `888888` — fine for a dimmed label, nowhere
near enough for one that has to be read across a room.

Colors are named for **the job they do**.
The chrome had three golds — `seam`, `playhead` and `grad_gold`, the last two
the same value under two names — with `playhead` standing in for "selected"
in eighteen places that had nothing to do with the playhead; there is now one
`accent` (gold, primary), one `accent_alt` (purple, secondary), and
`playhead` means the playhead. Every shade the editor draws lives in
`theme.rs`: not one `srgb(0x…)` literal survives anywhere else in the
workspace, text weights (`text` / `text_dim` / `text_off`) included — they
had been hardcoded three modules deep, unreachable by any theme swap. A
**`Surface`** is one complete material recipe (fill, gradient, radius,
border, bevel, shadow, inner shadow, grain) in *logical* px, and `Surfaces`
names the seven the editor is built from: card, header, plate, well, float,
field, hover. Call sites ask for a material instead of re-deriving one, so a
restyle is one function. The recipes are deliberately still flat — adopting
them changed zero pixels, and a test asserts each one against the longhand it
replaced — with every depth knob wired and at zero, waiting to be dialled in
rather than guessed at. `theme()` and `surfaces()` read a cached skin
(sRGB→linear conversion runs once, not per call inside per-layer loops) and
`set_theme` / `set_surfaces` swap it live: the hook the material editor needs.

**The material playground** (`View > Materials`, 2026-08-17) is that editor.
Spark's look kept being restyled by the one participant who can't see the
screen, so every attempt cost a build-look-describe-revert round trip; twice
it ended in a revert. This hands the controls over. It lives in the
**bottom panel** — full window width, already user-resizable by dragging its
top edge. (v1 tried the left panel: nowhere near enough room, sliders ran off
the edge, half the controls needed scrolling.) Two tabs. **Colors** is the
important one: every shade the editor draws with, grouped and named for the
thing you see (`Side panels`, `Layer card`, `Around the canvas`) rather than
the struct field, each a swatch plus the hex code it reads as. Click one and
type a code — it applies the moment the buffer parses, so the editor changes
on the sixth character rather than on Enter. **Depth** carries the
per-material knobs, relabelled for the effect rather than the code (`Corner
rounding`, `Highlight along top`, not `Bevel light`). Recolouring rederives
every material so a palette change reaches the borders, and the depth is
carried across so the two tabs never undo each other. **Print** writes both
halves — colours as hex, materials as palette *expressions* — beside the comp
file. Rules it lives by: it never styles *itself* from the values it edits,
so a colour dialled into oblivion can't take the panel that would undo it
down too; and its geometry is unit-tested for overlaps, escapes and narrow
panels, since nobody who can run the tests can look at it.

**The playground could only pick greys** (2026-08-18). Three things were
missing under it, and each one was a ceiling rather than a knob.

*The largest surfaces had no material at all.* The side panels, the tool
and transport bars, the timeline and the status strip were painted as bare
fills — `UiRect::region(v, t.panel)` — so the four biggest areas on screen
could be recoloured and nothing else: never shaded, textured, or lit. They
are `Surfaces::panel` / `bar` / `timeline` / `status` now, painted through
the same recipes as everything else and flat by default, which is why
adopting them changed zero pixels the day it landed. Their radius stays
zero by definition: a window region meets its neighbours, and a corner
radius there cuts a hole in the layout.

*A gradient could only run straight down, corner to corner.* The shader had
always taken any angle and a radial; no recipe could ask for either, and
none could confine the blend, so "a wash across the left quarter" was
unaskable. `Surface` gained `.toward(turns)`, `.radial(on)` and
`.span(start, end)` — before `start` the surface is its fill, after `end` it
is the far colour — and a direction alone still paints flat, because zero
means off here as everywhere.

*And the blend was in the wrong space.* `mix()` ran in linear light, which
is not the ramp anyone means by "gradient": at 3% of the way across, linear
0.03 encodes to sRGB 0.20, so a fifth of the brightness had already
happened in the first thirtieth of the surface, and the far colour appeared
to take ~98% of the run. It now converts to display space, mixes, and
converts back, so the halfway point looks halfway. Alpha stays a straight
lerp — it is coverage, not light.

Chrome colours also carry **transparency** now (`srgba`, alongside `srgb`),
which the palette had no way to express: `darken` used to force alpha to 1,
so shading a translucent surface quietly made it solid. And rather than
grow a second colour picker, the playground **borrows the right panel's**:
while it is open, the colour home paints the picked chrome colour instead
of the selection — captioned with what it has hold of, chips swapped to
Alva's grey ladder, and an alpha slider that appears only where alpha means
something. Closing the playground hands the picker back with nothing else
having to remember to let go. The square and the hue bar say nothing about
transparency, so they carry the existing alpha through rather than resetting
it. One last thing the playground had been getting away with: the timeline
went on drawing underneath it — bar shading behind the swatches, ruler
numbers behind the Print button — because the grid paints controls, not a
background. A panel you can see two screens through is not a panel.

Theme: dark charcoal chrome — explicitly NOT the Lantern warm-brown; Spark
has its own identity. Logic-Pro-dark energy with colorful accents to come.
Big text and controls always.

**Status strip** (2026-08-17): a title-bar-height bar across the very
bottom of the window. It exists first to *close the layout* — the
timeline's shaded axis is framed gold above it and gold down its left,
and used to run out into black at the window edge. A bare seam ruled
along that edge was tried and read as a stray artifact: an edge needs
something on the other side of it to be an edge. It reports what's
selected on the left and the playhead on the right (`Bar 5.3 · 0:08.42`
— bars and beats count from one, the way the ruler does and the way a
musician does). The action log currently going to the terminal belongs
here next.

Title bar: our own (window decorations off) — File menu at the far left,
logo block and controls at the far right, drag zone everywhere else. **No double-click behaviors on the title bar,
ever.** Edge-resize handles for the borderless window: todo.

Text (adopted 2026-08-15): **lntrn-text**, Alva's own engine, at Phase 4
(parsing, rasterization, discovery, full layout API, gamma-correct AA) —
its first field test outside Lantern. All call sites go through the
`spark_text` wrapper crate so backend evolution never touches widget code.
UI face: bundled Space Mono (OFL) — Alva's pick. Kerning/ligatures arrive
free when lntrn-text reaches Phase 5+. No panel header labels — Alva knows
what the panels are.

Layout (reworked 2026-08-16 — "the layer card owns everything about the
shape it represents"): shape tools in a strip pinned to the top of the
left panel; the rest of the left panel is reserved (tool options when
verb-tools land, then a file browser / asset library). The right panel is
the shape's world: the **color home** pinned on top (palette, current-
color bar, HSV picker — always visible; paints the selection, its armed
gradient endpoint, or the draw color when nothing's selected; the
**dice** at the bar's right end arm random style for new shapes — colour,
glow, brightness, thickness, fill/outline, gradient, sides, star density
are rolled once at mouse-down and held through the drag, geometry never
is, and the tool colour is left alone; session state like the snap
toggles, and our own xorshift rather than a crate, 2026-08-22), **layer
cards** below (identity row: kind glyph tinted the shape's color — a
stand-in until layer thumbnails — name, visibility eye, cogwheel; an
X/Y/Rotation/Scale field strip on every card: drag up/down to scrub, a
clean click opens the field for typing, Enter commits; the cogwheel
expands one card at a time into the full settings — sliders,
Style/Blend/Gradient toggles, gradient endpoint chips), and the zoom bar
pinned at the bottom. Selection reads as a gold card border; gold is the
primary accent for active state everywhere (purple is secondary — a
proper contrast palette pass is queued). Hidden shapes stay in the
document (`hide` lines in the format), draw as nothing, and can't be
picked on canvas. React sliders live in the Keys tab's
sidebar, with the keyframes and the track they ride. Floating chevron
popups (Lantern's Brush-Advanced pattern) are reserved for per-tool
options. The side panels flex wider to absorb the viewport's horizontal
dead space, so the 16:9 canvas aspect-fits the center snugly. The
**canvas view**: the stage maps through one CanvasView transform — 100% =
exact aspect-fit (the resting default), Ctrl+wheel zooms at the cursor
(25%–800%), middle-drag pans, Ctrl+0 returns to 100% — the same
zoomable-view pattern as the timeline's TimeView. A **zoom bar** sits in
a gold-seamed strip at the bottom of the right panel (mirroring the tool
strip atop the left): −/+ steppers, a 100% refit button, and a live
readout that goes gold whenever the view is off 100%. The **document has
no background** — true transparency, so future export can render straight
to alpha (VFX clips re-importable over other comps): the viewport gutter
paints deep purple (View > Black flips it), the stage sits on an
editor-only transparency checkerboard, and the shape pass scissors to
stage ∩ viewport so nothing bleeds over the chrome. Selection ants are
two-coat black + gold (dashed gold light riding a solid black stroke),
readable over any shape color. A
**transport toolbar** runs between the viewport row and the timeline: a
square Wave-tab button and a rectangular Arrange/Keys toggle on the left
(from Wave the toggle returns to whichever it last showed; on its own tab
it flips Arrange <-> Keys), then the active tab's own tools — each tab
shows exactly the tools it needs (the keyframe stamp only exists in Keys)
— and a big green play button centered. The **full-width bottom panel**
(toolbar + timeline, one block) is user-resizable — drag the toolbar's top
edge; double-click it to snap back to the default height. The timeline's
left sidebar is the lane-name box; the time axis owns the rest: bars/beats
ruler on top, tab content directly beneath (Wave: teal min/max waveform;
Keys: keyframe lanes), alternating light/dark bar shading (quarter-note
lines fade in as you zoom, phrase seams every 4 bars), all mapped through
one zoomable time view starting at the first bar (Ctrl+wheel zoom at
cursor, Shift+wheel pan). Wave is the default tab — the waveform strip is
back, and it rides the zoomable axis now. Rendering is event-driven — the
app redraws only when state changes (playback later drives continuous
redraw only while playing).

Star fields (2026-08-17, second of the background-tools wave — symmetry,
grid arrays and noise textures are still queued): a sixth shape kind, and
the first generator to arrive inside the shape system rather than beside
it. Drag a region with the Stars tool (`6`) and it fills with scattered
stars — but the document holds *one* instance, not five hundred: the
fragment shader divides the region into cells, hashes one star into each,
and a fragment only ever visits its own 3×3 neighbourhood, so density is
free and the whole field is one quad. Nothing is stored per star, which is
what keeps `frame = render(project, t)` true of a sky nobody placed: same
seed, same stars, every render, forever. Knobs: **density** (stars across
the *canvas*, not across the field — spacing belongs to the sky, so a small
patch is fewer stars rather than the same count crammed in, and stretching
a field reveals more sky instead of magnifying it), star size, glow,
**twinkle** amount and speed (each star pulses on its own hashed phase off
the playhead — so scrubbing back lands on the same frame), and a **form**
picker: dot, four-point sparkle, or diffraction cross. Size variance is
baked in from the hash, biased small, which is what reads as depth rather
than as polka dots. A field is a layer like any other — move, rotate,
stretch, keyframe (density, twinkle and speed all animate), fold into a
folder, ride the audio React amounts. It borrows the existing gradient
toggle to tint across the region, and offers no Fill/Outline, since the
number that would flip is the star size.

Two things grew to make room. `Shape` gained a fourth vec4, `extra` — 22
floats on a line now, with the 14- and 18-float eras still reading — and
the shape shader's globals gained the **playhead**, the one clock the
fragment stage gets. Time is a view input, not document state: the field
says how fast it twinkles, `t` says when we are. Every generator after this
one will want both. The shape pass now has its own offscreen
render-and-read-back tests (mirroring SparkUI's): they caught the globals
buffer still being vertex-only, which would have shipped as a black
viewport, and caught density meaning the wrong thing.

Gradient fills (first of the background-tools wave): any shape can carry a
two-color gradient (`color2` on Shape, 18-float lines in the format; old
14-float files still read). Mode follows the kind — radial for circles,
along the segment for lines, along local Y (riding the shape's rotation)
for boxes/ngons/paths. Inspector: a Gradient Off/On toggle, then an Edit
A/B row that routes the palette/swatches/picker at either endpoint; the
color section previews whichever end is being edited. Style copy/paste
carries gradients. A canvas-sized box + gradient = a background wash.

Merge groups (Ctrl+G / Ctrl+Shift+G): a merged selection becomes one
layer row ("name xN") and one object — click any member and the whole
group selects, moves, scales, and rotates around its shared center. Every
member keeps its own color, style, geometry, and keyframes (non-
destructive; group id per shape in the comp format as `group <id>`
lines). Vertex editing needs a single selected path — unmerge first.
Shape library: File > Save Shape... writes the selection (pose baked at
t=0, grouping kept, no audio/keys) to a `.sparkshape` file — the same
text format as comps — and File > Import Shape... appends one to the
current comp and selects it. A proper in-app shape browser comes later.

**Effects** (2026-08-17): a shape carries only what it *is* — where it
sits, how big, what colour. Everything you might optionally want it to do
is an effect you add, so a layer's settings list stays as short as the
choices you actually made. Glow is the case that proved it: it used to be
a permanent field floored above zero, so no shape could stop emitting and
"everything is neon" became structural rather than chosen. A setting that
is always present is a decision already made for you.

An effect is a **kind** plus a flat list of parameter values, with kinds
declaring their parameters in a static table — adding an effect type is a
table entry, not a new field on every layer. Two so far: Glow and
Gradient. Effects carry **stable ids**, not stack positions, because
curves address them and reordering must not repoint a curve at a different
effect. Turning one off keeps its settings; re-adding a kind you already
have turns it back on rather than stacking a silent twin. `resolve` paints
the stack onto the *display copy* of a shape each frame, so the document
is never mutated and an absent effect actively clears what it controls —
a look you didn't ask for cannot leak in from a field nobody can see.

**One thing, one place** (2026-08-18). Three controls on the layer card
were dead, in two different ways, and the rule that sorts them out is that
a value has exactly one owner — the same rule the Glow effect was built
on, applied to the ones that had drifted.

*Two owners, and the effect wins.* `resolve` writes the display copy from
the stack every frame, so anything the stack controls that also exists as
a shape field has a **dead** control on the card: whatever you set there
is overwritten before it reaches the screen. Additive's `Normal | Additive`
pair and Gradient's `Off | On` pair (and its endpoint chips) were both in
that state — visible, clickable, and doing nothing at all.

*No owner at all.* Brightness was the mirror image: it listed in the
browser, it added to a stack, it had a parameter, and `resolve` never read
it. The shape's own brightness slider did the work. A control that changes
nothing is worse than a missing one, because you spend the session
wondering what you did wrong.

So: **Brightness is a shape setting only** and its effect is gone.
**Additive is a shape setting only** — a checkbox, not a segmented pair,
since `Normal` was never a choice, only the absence of the other one, and
it cost a whole row of the card to say so. A comp that saved an Additive
effect has its pure light migrated onto the shape's own field on load,
because there the effect *was* the truth. And **Gradient is an effect
only**: its Off/On pair is gone, and its endpoint chips moved onto the
effect's own card, where clicking one routes the colour home at the
effect's colour parameters. A colour is three parameters only because a
parameter list is flat floats; the card draws it as a chip you click and
hides the three channel sliders, since nobody picks a colour by dragging
its channels apart. Adding the effect seeds a deep-dimmed copy of the
shape's own colour, so the wash reads immediately rather than being a fade
to black — the seeding the old Off/On toggle used to do, moved to where
turning a gradient on now happens.

The **checkbox** is a new SparkUI widget: a square with a tick made of two
capsules from the material renderer rather than a new shader glyph, so it
inherits colour and scale like everything else. The box is the small part
and the whole row is the target — a 30px square asks to be missed.
Effect cards also stopped borrowing the layer card's grey (the lightest
surface in the panel, which made a list of effects the loudest thing on
the card) for a material of their own at `151515`, a rung *below* the
block they sit on, so an effect reads as sunk into the settings. Like
every other material it is live in the playground.

Keyframes therefore stopped naming properties and started naming
**targets**: `Target::Shape(Prop)` or `Target::Effect { id, param }`. The
whole curve system — sampling, the stamp diff, the backfill, the
clipboard, the lanes — works in targets, so every effect parameter is
keyable the moment its effect exists, without the keyframe machinery
knowing what an effect is. A parameter's spec carries an `absent` value
alongside its default: what the resolver draws *without* the effect. That
is the value a stamp treats as its history, so adding glow at bar 5 and
pressing `K` backfills a holding key of zero at bar 1 and the glow ramps
up from nothing instead of appearing flat.

Still on the shape and deliberately so: transform, size, colour, opacity,
fill vs outline, and the intrinsic per-kind numbers (a polygon's sides, a
star field's density). Brightness is still queued to move out onto the
stack. Repeaters, symmetry and grid arrays are queued as effects rather
than as the drawing tools they were originally planned to be: they
multiply instances, which needs no shader work, and as stack entries they
compose, keyframe and ride the audio for free.

**Opacity** (2026-08-18) is the one that stayed. `Shape.color` was
`[r, g, b, intensity]` with no alpha channel at all, so until now nothing
in Spark could fade — the most ordinary thing an animation does was the
one thing the renderer had no room to express. It is a shape property
rather than an effect on purpose: glow is a look you add, but 100% opaque
is not a decision you made, it is the absence of one, the same as X=0. A
shape can be faded the moment it exists, with no setup, and `K` keys it
like anything else.

`Shape` gained a fifth vec4 — 26 floats on a line, with the 14-, 18- and
22-float eras still reading — and it is the **one field where zero is not
"off"**, since off for opacity is invisible. Reading a missing tail as
zero the way every other field is read would open every comp written
before today completely blank, so the rule ("nothing had been faded when
nothing could fade") lives on `Shape::from_short_array`, next to the
fields it is a fact about, rather than in the document parser.

The renderer already blended **premultiplied** — the fragment stage emits
light already scaled by coverage, and alpha *is* the coverage — so a fade
is one multiply on the whole result, and it comes out right for free:
a body stops occluding at exactly the rate it stops emitting (a shape
faded out that still punched a hole in what was behind it would be a black
shape, not an absent one), a glow halo fades with the shape it comes off,
and an additive shape fades without ever starting to occlude. `fs_main`
became a two-line wrapper around `shade` for this: a star field composites
itself and returns early, and a second exit is a second thing to forget.
Pixel-readback tests hold all of it — half opacity is half the light, a
faded shape blends against what is behind it by exactly the amount asked
for, and a faded-out sky has no stars in it.

**Folders fade too**, from a slider on their own row under the X/Y/R/S
strip — not a fifth box in it, because five boxes across that panel are
five boxes too narrow to read, and the strip matching a layer card's four
is what makes the two rows read as the same kind of object. It multiplies
into each member (a shape at 40% inside a folder at 50% is at 20%, not
back up at 50%), which is an honest limitation rather than the real
thing: members composite one at a time, so overlapping ones show through
each other halfway down a fade. Doing it properly means rendering the
folder to its own texture — the same seam comp-level post FX will need.
Opacity joins the folder's animatable axes but stays out of its *first*
pose, which is the same four numbers a shape's is: fading is a change you
make, not part of standing still. It rides its own `folderfade` line in
the format rather than a ninth column on `folderdef`, where the name runs
to end of line — a folder actually named `1` would otherwise be read as an
opacity and lose its name.

Interaction-model direction (agreed 2026-08-16): the object model stays
(keyframeable persistent shapes — this is choreography, not pixels), and
the ceiling rises via three pillars: ① properties into the layer cards
(done), ② tool verbs with per-tool floating options (gradient drag tool,
scatter brush, warp), ③ a per-layer effects stack (grain, glitch, dash,
shadow, trails — keyframeable, audio-reactable) to end the neon-only
look, plus comp-level post FX later. Dark gritty dubstep = glow 0 +
grain + displacement keyed to bass.

## Dependency policy

We build our own everything, except where it's genuinely unreasonable:

| Allowed | Why |
|---|---|
| `wgpu` | The GPU API. Given. |
| FFmpeg (subprocess, not linked) | Video encode, audio file decode. Piped via stdin/stdout. |
| `winit` | Wayland/X11 windowing is protocol hell with zero creative payoff. |
| `cpal` | Audio *output* device access only. Decode is FFmpeg's job. |
| `lntrn-text` (path dep) | Text: Alva's own engine, adopted at Phase 4. Wrapped behind `spark_text` — the only crate that knows the backend. |
| `glam`, `bytemuck` | Math + GPU byte-casting. Buildable ourselves; not worth the early time. |
| `lntrn-file-manager` (subprocess) | Open/save dialogs: Alva's own file manager in `--pick` / `--pick-save` mode; chosen path arrives on stdout, cancel exits 1. |

Everything else — UI framework, FFT, beat detection, timeline, curves, undo,
serialization, particles, post-fx — is ours.

## Crates

```
crates/
  spark_render    wgpu core: device/surface, shape SDF pass (kinds, star
                  fields, hit testing), post-fx chain, frame capture
  spark_audio     FFmpeg-pipe decode, our own FFT, analysis curves,
                  peaks cache, cpal playback with a sample-accurate clock
  spark_project   the document: timeline, tracks, clips, comps, params,
                  keyframe curves, serialization, undo
  spark_fx        the visualizer zoo: shapes, particles, lasers, lightning,
                  tunnels, ribbons — each comp type lives here
  spark_ui        immediate-mode editor UI drawn by spark_render
  spark_studio    the app: viewport, timeline panel, inspector, export
```

## Milestones

1. **Editor core** — the canvas is the app: draw, select, move, scale,
   rotate, and style glowing shapes in the viewport; save/load the comp.
   Then SparkUI chrome: toolbar, inspector, layer list — big text.
2. **Timeline & audio** — import a track, waveform + analysis curves, cpal
   playback and scrubbing, auto-key choreography, audio-driven bindings.
3. **Export** — pipe frames to FFmpeg → a real .mp4 with the track muxed in,
   made entirely with the tools.
4. **FX zoo** — repeaters, paths, generators (liquid neon, raymarched
   tunnels), lasers, lightning, particle storms, transitions.
5. **3D layers, meshes & rigs** — real camera + instanced geometry layers,
   then glTF import and skeletal animation.
6. **Excision mode** — rigged monsters pointed directly at the camera. 💀
