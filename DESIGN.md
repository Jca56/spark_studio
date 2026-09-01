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
4. **Objects in the scene** — meshes, lights, the camera, then raymarched
   solids and particles. Not "3D layers": the comp *is* a 3D world, and a
   2D comp is one where nothing has left the canvas plane — see *The comp
   is a scene* (2026-08-30).
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

**The stage is cached, and drawn in two layers** (2026-08-22, after the
dice built a seventy-glow stress test in thirty seconds and the fans went
to full throttle with the song playing). Every shape's quad reached four
glow radii past its body, so a glowing shape was a near-canvas-sized quad
and seventy of them were seventy full-viewport fragment passes, each
sampling a smooth exponential at 4K, on every frame of playback — and a
redraw ran that straight into the swapchain on *every* event besides.
`Stage` in `spark_render` now renders the shape pass into its own texture
and composites that onto the frame, redrawing only when the pass's inputs
differ from what it holds — all of them, shapes, path pool, resolution,
view, time, clip, compared by value rather than by a dirty flag an edit
path could forget. That quiets idle redraws. Playback misses every frame,
as it must, so the frame itself got cheaper: **bodies** draw at full
resolution in quads that hug them, **halos** draw at half resolution in
the wide quads a halo needs and come up bilinearly — the light model
`core + halo·(1−core)·0.55` is separable, so the split is exact per shape.
A halo under six screen pixels stays with its body (cheap there, soft in
the halo layer), a star field's per-star light stays with the field, and a
glow-0 shape costs the halo pass nothing at all. The halo window went from
four radii to three (a 5% tail instead of 2%) while we were there. The
shape pass keeps one globals buffer *per layer*, because a
`queue.write_buffer` lands before the whole encoder runs and two passes
sharing one would both see the second's. **The picture changes in one
way, deliberately:** a halo now lies over every body, where it used to be
hidden by bodies in front of its own — bloom behaves this way, and a test
holds it on purpose. **View > Half-Res Playback** renders the stage at
half size while the song runs, full size the moment it stops; the paused
picture and export never see it. Readback tests hold a halo-free stack to
within one 8-bit count of a live frame, a wide halo to within 10% of its
light with the body pixel exact, and preview to the same fill colour.

Interaction-model direction (agreed 2026-08-16): the object model stays
(keyframeable persistent shapes — this is choreography, not pixels), and
the ceiling rises via three pillars: ① properties into the layer cards
(done), ② tool verbs with per-tool floating options (gradient drag tool,
scatter brush, warp), ③ a per-layer effects stack (grain, glitch, dash,
shadow, trails — keyframeable, audio-reactable) to end the neon-only
look, plus comp-level post FX later. Dark gritty dubstep = glow 0 +
grain + displacement keyed to bass.

## The comp is a scene (2026-08-30)

Spark was going to get "3D layers" the way After Effects has them: a
flag on a layer, and a rule that 3D layers share a space only while they
sit next to each other in the stack. Alva asked the better question
before a line of it existed — *are layers even still a thing in 3D?* —
and the answer is that a layer stack had been doing two jobs at once.
**The scene** — where things are relative to each other — is a question
geometry answers, with a camera and a depth buffer; nobody orders a scene
by a list. **The composite** — stacking pictures, transitions, post —
genuinely is an ordered stack. AE made the stack home and bolted the
scene on; the contiguity rule is the most hated thing in it, and exactly
the afterthought we said we wouldn't build.

So a comp is one 3D world: objects, a camera, lights. The list of layers
becomes the **outliner** — the list of objects, with hierarchy (a folder
already *is* a parent transform; it just didn't know it) — and the
compositing job moves to where the timeline always planned to do it:
tracks of clips instancing comps. Post-FX applies to the scene's render.
Nothing gets a second home.

What makes this not a rewrite: **a 2D comp is a 3D scene where nothing
has left the canvas plane.** The frame is the canvas — x right, y down —
with z added, running *toward* the camera: larger is nearer, the way a
higher layer is on top. (After Effects runs it the other way, and the
first cut of this did too; Alva's first hour with it said which was
right.) The stage camera (`Camera::stage`, 40° vertical, about a 50 mm
lens) looks straight at the canvas centre from far enough back that the
canvas plane projects to *exactly* the canvas rectangle, and `view_proj`
composes the CanvasView's fit, zoom and pan into it so the shader has one
matrix and one multiply. A point at `(x, y, 0)` lands on precisely the
window pixel the flat 2D map used to put it on — `camera::tests` holds
that to a thousandth of a pixel, and every readback test in
`pass/tests.rs` and `stage_tests.rs` now renders through the camera
unchanged, which is the proof. Perspective only shows on things that
leave the plane. The maths is ours (`math.rs`: `Vec3`, a column-major
`Mat4` laid out the way WGSL reads one), since `glam` was allowed but
never needed and a scene needs about two hundred lines of it.

**Shapes are flat.** A circle is a disc on a plane, a box a rectangle, a
line a ribbon, a path a flat polyline, a star field a flat sky — the same
as AE, Cavalry and Blender's grease pencil, and what you want from 2D
primitives: tilt a ring, stack twenty of them down z, fly the camera
through. The fragment stage did not change: its `world` coordinate is
now the object's *plane-local* position, interpolated perspective-correct
across the projected quad, so every distance field, gradient and glow is
evaluated on the plane rather than on the screen, and the `fwidth`
anti-aliasing is right on a turned plane for free. A per-instance
**model matrix** (a storage buffer indexed by `instance_index`) places
the plane: `clip = view_proj · model · (world, 0, 1)`. The 2D fields stay
what they are — position and spin *within* the plane; the matrix is
everything that moves the plane — so a shape's plane-local pose and its
place in the scene have one owner each. A square is not a cube: volumes
will be new kinds, raymarched 3D distance fields that write real depth
(Spark's SDF identity extended into space — glow in 3D, exact
silhouettes, smooth blends), and imported meshes are the other solid.

**Order comes from depth.** The stage sorts every shape back to front by
its centre's distance along the view before either layer draws — stably,
so shapes at one depth keep their list order, which is how a comp that
never left the plane still stacks the way it did, and strictly better
than the z-fighting a 3D tool hands coplanar things. Every target carries
a **depth attachment**: the opaque passes to come (meshes, then SDF
solids) write it; the shape pass tests against it and never writes, so a
shape behind a mesh is hidden by the mesh and a shape in front is not. The mesh pass writes it (below). One picture change to name: a halo
*behind* an opaque object is occluded by it. The 2D "halos over
everything" budget decision gives way to "a plane's light is on that
plane", and the spill bloom gives comes back from post, where it belongs.

The pass takes a **`Scene`** — shapes, models, path pool, camera, time —
as one struct rather than five loose parameters, so the next input
(lights) doesn't ripple through every call site. Models may be shorter
than shapes; anything without one is on the canvas plane, which is where
overlays (the grid, the ants) and every existing comp live. The camera
and the models join the stage cache key: a moved camera is a miss, a
hovered card still isn't. The studio passes no models and the stage
camera today — the scene under the picture, with nothing in it yet that
could tell. `scene_tests.rs` checks what only a scene can do: a turned
box narrows to cos θ of its width and a tilted one shortens, a box pushed
to twice the camera's distance is half the size and still centred, a
nearer shape is drawn over a farther one whichever was listed first, and
a glow on a turned plane stays beside its narrowed body.

**The GLB reader** (`spark_assets`, 2026-08-30): glTF 2.0 in, a `Model`
out — every mesh in the scene flattened through its node transforms into
Spark's frame, materials' factors and texture references, images as the
JPEG/PNG bytes they were stored as (decoding is FFmpeg's job, at the
renderer's convenience). GLB over OBJ because it is the one format on the
export menu that carries rigs and animation, which milestone 5 needs;
ours rather than Ember's because Ember's loader is the `gltf` crate under
a wrapper — a spec for what to cover, not code to take. It sits on our
own JSON parser: glTF is JSON wrapped around a binary blob and nothing
else in Spark speaks it, so a `Json` enum with linear-lookup objects and a
few typed accessors is the whole of it. `.glb` and `.gltf` both, with
buffers from the BIN chunk, a file beside the document, or an inlined
base64 data URI. glTF's frame is +y up and +z toward the viewer; Spark's
canvas is y down and z away — a half turn about x, a proper rotation, so
winding survives and a mesh drawn with an identity transform stands
upright facing the camera. Flat normals are computed where a file has
none, strips and fans are unrolled, points and lines are skipped, and
sparse accessors are refused rather than half-read. Not yet: skins,
animations, morph targets — the rig milestone's. Alva's logo — 68 MB of
Meshy output, 580k vertices, 1.1M triangles, three embedded JPEGs — reads
in about 130 ms, which is why the crate builds optimised even in dev, as
`spark_audio` does. `cargo run -p spark_assets --example inspect --
file.glb` says what the loader made of a file.

**Every object has a place in space** (2026-08-30). `Shape` gained a
sixth vec4, `space` — `[z, tilt, turn, unused]`, 30 floats on a line with
the 26-float era still reading — and with it every shape, mesh or not,
carries a full 3D transform: the 2D fields are its pose *on* its plane,
`space` is where the plane is. `Shape::model` builds the matrix (turn,
then tilt, about the shape's own centre, then z), the studio hands one per
display copy to the stage, and the selection ants copy `space` so a turned
shape is outlined where it is drawn. Z / Tilt / Turn are properties like
any other — keyable, scrubbable, typeable, in the stamp diff and the
lanes — and they sit on the card's settings block as a second strip of
three scrub fields rather than sliders, because Tilt and Turn count turns
the way Rotation does and a slider can't type 720. Picking asks each
shape where the click lands *on its plane*: the ray from the camera
through the canvas point, met with the plane, handed to the same 2D
distance every shape already answers (`Shape::unproject`). The handle rig
still draws flat on the canvas plane — named here so nobody thinks a
turned shape's corners are mis-drawn; dragging, scaling and rotating
through it still act on the right numbers.

**Images decode through FFmpeg** (`spark_assets::image`): any bytes a
file or a GLB holds, piped in, raw RGBA out, the way `spark_audio` takes
PCM. Raw video carries no size, so PNG and JPEG headers are read here and
anything else is asked of `ffprobe`. A decoded image builds its own mip
chain, box-filtered in *linear* light — averaging sRGB bytes darkens
every edge; black and white average to 188, not 128, and a test says so.

**The mesh pass** (`spark_render::pass::mesh`). Meshes are the first
thing in a comp with a real inside and outside, and they are drawn first:
into a 4× multisampled colour target with a multisampled depth buffer,
resolved to a plain texture the stage lays down under every shape. The
shapes are analytically anti-aliased by their distance fields; a
rasterised triangle next to them without multisampling would read as a
different kind of picture. Multisampled depth can't be resolved by the
GPU the way colour is, so a small pass does it — the nearest sample wins
— into the stage's single-sample attachment and again at half size into
the halo layer's, so a halo behind a mesh no longer glows through it
(`a_halo_behind_a_mesh_does_not_glow_through` holds it). Lighting is one
sun from the upper left in front of the canvas, ambient, and a Fresnel
rim — the default a comp gets until it has lights of its own — and
whichever side of a face looks at the camera is the side that is lit, so
a double-sided plaque and a mesh whose winding nobody checked both come
out right. Colour is the base texture times the material's factor times
the object's tint and brightness; opacity multiplies colour and alpha in
the resolved picture while the mesh still writes depth at full strength,
so a fading mesh hides what is behind it until it is gone — honest, and
the one thing a proper fade of solid geometry would need more than this.
One instance per primitive, matrices in a storage buffer by
`instance_index`; the stage cache keys on every instance's mesh id,
matrix and colour, so a moved mesh is a miss and a hovered card still
isn't. Pixel tests hold the sun-lit face to the byte it computes to
(188), the unlit tint exactly, a partial edge pixel under MSAA, a texture
on its texel centres, and what's-behind hidden with what's-in-front not.

**Mesh objects** (`Shape::mesh`, kind 6). A mesh is an object in the
outliner like any shape: centre, size, colour, opacity, place in space,
keyframes, a card with a cube glyph. `b` holds the half extents of its
*footprint* on the plane, fitted at import so the model's larger side
spans half the canvas's height (`MESH_FIT`), aspect kept — so `size()`,
scaling, the audio React on scale and the selection ants all work as they
do for a box, and the model is placed each frame by `meshes::placement`
(centred on the shape, scaled to its size) under `Shape::model`. The
model itself is an **asset**: `asset <id> mesh <path>` lines in the comp,
`extra[0]` on the shape naming which; the same path imported twice is one
asset, saved shapes carry the assets their meshes draw, and imported
shapes are repointed at this comp's ids. File > Import Mesh… reads the
file and decodes its textures on a worker thread — the logo is 68 MB and
its 4096² base colour takes FFmpeg most of the ~550 ms — and the shape
appears when the model arrives, on the thread that owns the device;
opening a comp reloads every asset it names. No fill/outline, no
width/height, no Additive on a mesh's card: a model is what it is. What a
mesh ignores today and will not tomorrow: the Glow effect (nothing to
glow), the metallic-roughness and normal textures (read, not yet drawn),
the material's emissive.

**Lights are objects** (2026-08-30). Alva's call, and the right one: a
light is a thing you place and aim, so it is a shape (kind 7) with a
card, keyframes, a colour from the colour home, and audio React — not a
setting in a panel. Three kinds on one card, switched by a Sun / Point /
Spot picker: a **sun** is only a direction; a **point** sits somewhere and
fades to nothing at its range; a **spot** is a point with a cone. The
shape's own numbers are the light's: `b` is the range, so `size()`,
scaling and the bass React all reach it; brightness is intensity, so the
slider (relabelled *Intensity*) and the mid/onset React are one multiply
into the colour; Tilt and Turn aim it — a light shines along its plane's
normal into the scene, so the two numbers that aim everything else aim
a light too. A spot adds a **Cone** slider (`Prop::Cone`, keyable). No
opacity, no Additive, no fill: a light has nothing to be see-through and
already is pure light. `Shape::as_light` is the whole handover to the
renderer, and `Shape::sun` builds a sun object aimed exactly where the
default sun points, so adding one changes nothing until it is moved.

They arrive from a new **Add** menu (File · Add · View): Sun, Point
Light, Spot Light. A sun lands in the upper left; a point or a spot at
the centre, 400 units in front of the canvas, so it lights what is
already there. On the canvas a light is a **gizmo** — editor overlays,
never part of the picture: a ring on the light's own plane (tilted with
it, so it reads as a disc facing where the light aims), a dot, and for a
sun or a spot a short line along the aim's on-canvas direction, all in
the light's colour as pure light. It is picked and outlined by the gizmo,
not by how far it shines.

In the mesh pass the lights are a uniform of up to eight, and the shader
loops: a sun is `n·-dir`; a point is **inverse square in its own
units** — `r² / (d² + r²/4)`: full intensity at its range, a quarter at
twice that, four times right at the light, and never nothing; a spot
multiplies that by a `smoothstep` across its cone's edge, softened by
`soft`. (The first cut faded as `(1 - (d/r)²)²` to *exactly* nothing at
the range, and Alva's verdict once the gizmo let lights fly was that
point and spot "barely produce any light at all even at full strength":
most placements landed past the cutoff, and inside it the window was
dim for most of its reach. A range is where a light is nominal now, not
where it dies — 2026-08-31.) A comp with no light from *somewhere* is
handed `Light::default_sun` — the sun every mesh was lit by before lights
existed, so nothing changed the day they arrived — and the stage cache
keys on the lights, so a moved or reacting light is a miss. Pixel tests
hold a point light at its range to the byte it computes to (188), twice
as far to a quarter of that (131), a spot lit on its axis and not 15°
off it, and a red sun tinting a grey face red. Lights light meshes only:
shapes are emissive and always were.

**Ambient is a light** (2026-08-31). The scene's base level (0.22) and
the Fresnel rim's strength (0.35) were constants in the mesh shader;
Alva wanted them as settings — keyable, reactive — and the honest home
for a scene setting in Spark is an object with a card. So a fourth
kind, **Ambient**, on the same picker as Sun / Point / Spot and on the
Add menu: light from everywhere at once, its Intensity and colour the
scene's level, a **Rim** slider (`Prop::Rim`, last in the prop order so
the keyed-bit mask of everything before it holds) the rim's strength.
A fresh one starts at the defaults, so adding it changes nothing until
it is turned. In the shader the first ambient replaces the default
level and any others add to it; and an ambient alone does not put the
sun out — the default sun is handed whenever no light comes from
*somewhere*, so a comp with only an ambient is the lit comp it was, at
the level it asked for. Its mark is the ring and dot every light gets,
placed upper right out of the way, since everywhere has no place.
Pixel tests hold ambient 0.5 with the sun to 211 and a black ambient
with the sun to 170.

**Shadows** (2026-08-31, Alva's pick over material response: sun and
spot, meshes only). Each casting light — a sun or a spot, the first
four in scene order — gets one 2048² layer of a depth array
(`pass/mesh/shadow.rs`), rendered from the light by a vertex-only
pipeline over the same instances the mesh pass draws, both faces, with
a slope-scaled bias. A sun looks through an orthographic box fitted to
the world bounds of every mesh in the scene (the sphere they fit in,
from twice its radius back); a spot through a perspective frustum a
little wider than its cone, from where it is to just past the far
corner of those bounds — a cone wider than a map can hold is capped,
and past the map's edge a point is simply lit. The mesh shader asks
each light's map, through a comparison sampler and a 3×3 tap, how lit
a point is, from a little off the surface along its normal (more at
grazing angles) so nothing shadows itself; the answer scales that
light's term and no other. The default sun casts too, so a head has a
shadow the day it is imported. Shapes are light and cast none; point
lights don't cast yet (six faces each). Every light knows its map by a
slot in its uniform, and the lights are resolved once — the default
sun added, the slots assigned — before either the maps or the shading
read them. Pixel tests hold a sun's shadow of one quad on another 22 px
to the right of where the caster sits, the caster itself lit cleanly,
and a spot's shadow from 45° up-left.

**Built-in meshes** (2026-08-31). Alva, on first seeing shadows: "an
alien head and two subwoofer meshes isn't a whole lot to really see
what's going on" — in Ember there was a ground and trees. Only meshes
take light or shadow (shapes are light; the floor grid is lines), and
Spark could import a surface but not make one. Now Add > Plane / Cube /
Sphere: unit-sized models generated in code (`primitives.rs` — a quad, a
box with four vertices a face so every face is flat, a 32×16 UV sphere
with its top up), each an asset under a `builtin:` path so it rides the
import rails unchanged: `meshes::load` makes it instead of reading it,
one asset however many times it is added, an `asset` line in the comp,
reloaded on open, fitted and placed like any model. A plane tilted a
quarter turn is a floor; the shape's size scales it. Tools, not
content: what the ground looks like is Alva's.

Alva built a room out of planes within the hour, and found the walls:
**S stopped at 900** — `props::fit` clamped every value to its slider's
range, and a floor is wider than any canvas — and **a plane could only
be square**, a mesh's card having no Width or Height because "a model
is what it is". Sizes now have a floor and no ceiling (the range's top
is where a slider ends, not where a value stops), and a mesh has a
width and a height — its footprint's — so `meshes::placement` scales
the model's x and y each to its side, depth following the thinner one:
a stretched plane is a floor, a stretched cube a slab. A footprint
fitted from the model keeps its aspect and scales uniformly, as before.

**Objects snap to each other** (2026-08-31, Alva's ask: "make a little
room like that easier"). View > Smart Guides gained its 3D half
(`align.rs`): every object has a world-space box — a mesh's model
through its placement, any other shape's footprint, spun, on its plane
— and a gizmo arrow drag locks the selection's box along the drag axis:
its low edge, centre or high edge to any other visible object's low
edge, centre or high edge, and to the canvas's, within 12 logical px on
screen at the pivot. The lock is always taken from the cursor's free
intent, never from where the last lock left the object, so leaving one
only takes moving past its reach — the 2D guides' rule. Where it locks,
the other object's box is drawn sliced at that value, a gold rectangle
of light (a line, for a plane's flat box), so the edge you snapped to is
the edge you see. Rings don't snap yet; angles are for another day.

**Sizes are sizes** (2026-08-31, the same room). Alva: "the Width and
Height of a plane is maxed at 1920×1080 … at 1920×1080 that's ~900
scale!? … Scale makes no sense to me whatsoever. The struggles of
duct-taping 3D on after the fact." Three things were true. **S was the
half-size** — `Shape::size` is a radius, a circle's own number, and
for a box half its longer side — so a plane at S 900 was 1800 wide,
which is nonsense beside a Width of 1800. **Width and Height were
sliders**, so the track was a ceiling and touching one re-clamped a
size that had been scaled past it. **A mesh had no third side.** Now:
the card's S is the shape's full size — the longer side, a circle's
diameter (`props::extent`; a light's S stays its range) — and
`set_prop(Scale)` takes one back, so the numbers on a card agree with
each other; Width and Height are **scrub fields** on a strip under Z /
Tilt / Turn, unbounded and typeable, for anything with a box; a mesh's
strip has **D** too, `Prop::Depth` (keyable, last in the prop order),
its model's z at the footprint's scale — fitted at import, filled in
for meshes from before it existed the moment their model arrives, and
scaling with the whole. Inside, a shape still keeps half extents and
its keys still count them: the change is what the card says and takes,
so no comp changed. Every size ceiling is gone: `fit`, `set_box_*` and
`scale_by` keep a floor and nothing else. Nothing here is a knob added;
it is the 2D skeleton's radius finally called what it is.

**z runs toward the camera** since the same day — larger is nearer, the
way a higher layer is on top. The first cut had After Effects' direction
and Alva's first hour with it said which was right; the flip touched the
camera basis (the frame is left-handed now, which only `Camera::view`
has to know), the glTF import (a flip of y rather than a half turn about
x), and every test that pushed something back.

**Moving things in 3D** (2026-08-30, the same long day). Alva, after an
hour with lights: *moving 3D objects on a 2D plane is incredibly
difficult, I hate positioning stuff by typing numbers in a box, and it's
so hard to tell if something points toward or away from the camera.*
Three causes, three fixes, all landed together.

*There was no handle for Z, Tilt or Turn at all.* The rig only knew the
canvas plane, so the only way into the third dimension was the number
boxes. Now a **transform gizmo** rides the primary selection: three
arrows along the world's axes — X right, Y down, Z toward the camera,
red, green, blue — that slide the selection along that axis, and three
rings that turn it. The rings are a **gimbal**: each sits on the axis
its angle actually rotates about — Turn on the world's Y, Tilt on the
turned X, Spin on the plane's own normal — so a drag round a ring is
exactly that angle changing, and an angle that has counted three turns
keeps its count (a world-axis gizmo would have needed an Euler
decomposition every frame, and would have lost the count). It is one
size on screen whatever its depth, built from ordinary shapes placed in
3D — an arrow is a segment and a billboarded dot, a ring is a circle on a
plane — and hit-tested in pixels through the camera the viewport is
looking through, so it works the same in either view. An axis pointing
straight at the camera has no direction on screen; dragging up brings
the selection toward you. The 2D rig keeps its corners and edges for
scaling and stretching in the comp viewer; in the fly view only the
gizmo is drawn.

*Toward and away were symmetric.* A spot's aim line was the on-canvas
projection of its direction, which throws away exactly the axis that
matters. Light gizmos are in true 3D now: a spot draws its **cone as a
wire** — the far ring grows when it points at the camera and shrinks
when it points away — a sun draws an arrow along its direction, a point
its reach as a ring facing the camera. And a **floor grid** (View > 3D
Floor; always on in the fly view) runs under the canvas and back into
the scene, so perspective has lines to draw depth with.

*You couldn't look from anywhere else.* The **fly view** (`Tab`, or
View > Fly View) looks through an editor-only camera you fly around the
scene. The first cut was an orbit camera with Blender's bindings —
middle-drag swung the eye about a target, Shift+middle panned, Ctrl+wheel
dollied — and Alva's verdict was that the controls "made no sense": the
hands in question learned Ember's editor, not Blender's, and an orbit
camera has a pivot the flyer never chose. So it is a fly camera now, on
Ember's scheme (2026-08-31): **drag empty space to look** around — the
eye stays put and the drag grabs the world, inverted on both axes at
Alva's request: the scene follows the cursor, so a drag right turns the
view left and a drag down looks up — **WASD flies** along and across
the look, **Q/E** straight down and up the world, **Shift** sprints
(×4), the **wheel**
steps forward and back a fixed 500 units a notch so it neither crawls up
close nor rockets far out, and a **right- or middle-drag pans**. The
keys are held keys, read by their physical position so the cluster is
under the left hand on any layout, and they count only while the fly
view is up and the cursor is over the viewport — in the comp viewer W is
still the brightness nudge — with the frame loop kept running while any
is down and the eye moved by the real time between frames, capped so a
stall doesn't leap. A left press on empty space is nothing until the
cursor has travelled four pixels; then it is a look, and a look never
drops the selection — a press that never travelled was a click, and that
deselects. Pressing on a shape or a gizmo part grabs it as it always
did; with a drawing tool chosen, a drag draws. The camera parks where it
was left, so `Tab` out and back lands on the same view.

The canvas is drawn as a gold frame in space
and the render camera as a purple frustum from its eye to the canvas's
corners — which for the stage camera lands exactly on the frame. Nothing
about the document knows which view is up: the stage renders through
whichever camera it is handed, keys its cache on it, and the **Framing**
that places the picture is either the canvas rectangle (aspect-fit,
zoomed and panned, clipped to its panel — the video as it will be) or a
free camera filling the viewport with its own aspect (the scene as it
is). Every click and drag passes through one conversion — the cursor is
where the mouse's ray meets the canvas plane, whatever the camera — so
drawing, moving and picking work inside the fly view unchanged, and
the gizmo picks its rings by meeting the ray with each ring's plane.

The frame's scene is assembled in `scene.rs` now rather than inline in
the renderer: document display copies place their own plane, overlays
arrive with the matrix that puts them wherever in the scene they belong,
and `overlay.rs` holds the vocabulary — a segment between any two points
(a line on a plane that contains it), a circle on a plane, a dot facing
the camera, the floor, the frame, the frustum.

**Handles you can see, from anywhere** (2026-08-31). Alva's screenshots
said the rest: a point light whose own ring swallowed the gizmo's 78 px
hairline rings, and the imported alien head with no gizmo at all — it
was inside the mesh, and the depth test hid it as honestly as it hides
everything else. Two fixes. The gizmo's sizes are **logical px,
multiplied by the UI scale**, and about twice what they were (arrows
160, rings 110, 4 px shafts), because a handle you can't see is a
handle you can't grab. And the scene gained marks drawn **over
everything**: `Scene::over` counts shapes off the tail of the list that
the shape pass draws last through a second pipeline whose depth compare
is `Always` — the same shader and instances, no question asked of the
depth buffer — sorted among themselves so they still layer sensibly,
and keyed in the stage cache. The transform gizmo is drawn that way. The
light gizmos, the floor, the canvas frame and the frustum stay in the
scene with depth: a spot's cone drawn through a wall would lie about
where the light is, and a light that has gone inside a mesh is still
found through its card, where the gizmo — on top — shows where it is. A
pixel test holds a red mark behind a grey quad: hidden in the scene,
on top once counted over.

Alva's first flight with that (same day): in the comp viewer the gizmo
is "essentially useless and that's just how it works" — head-on, two
rings are edge-on and the Z arrow is a dot, which is the geometry, not
a bug — and in the fly view it was good, with four asks. **Spin never
turned a mesh**: Rotation on the card and the Spin ring turned the
footprint and left the model where it was, because `meshes::placement`
composed centre, scale and bounds and never the shape's rotation. It
spins about the plane's normal now, x toward y, the same turn the 2D
field makes, so model and box turn together. **One half at a time**:
the gizmo is either the arrows (Move) or the rings (Rotate), `R`
flipping between them, so neither hides the other and a grab is never
ambiguous. **Bigger and louder**: arrows 200, rings 140, 5 px shafts,
saturated colour — and **opaque** rather than additive light, since a
handle has to read the same over a grey mesh as over black; the marks
in `overlay` stay light. And the **wheel no longer scales the
selection** — a stray notch resizing the thing under the cursor "has
been driving me crazy since the very beginning"; over the canvas a plain
wheel now does nothing, Ctrl+wheel zooms, and in the fly view it flies.

What comes next: the **camera as an object** with a card and keys (the
frustum is already drawn; it just can't be moved yet); a **work plane**
for drawing off the canvas; the SDF solids; the 2D rig on a turned
plane. Rotation is Spin / Tilt /
Turn, turns-counting Euler, because keys count turns and animators key
angles, not quaternions. The comp viewer shows the *render* camera's
frame — what the video will be — with zoom and pan a 2D view over it,
AE's comp viewer rather than Blender's orbit; the fly view is the
editor-only camera for placing things.

## The canvas has a size, and the first video (2026-08-31)

Alva wanted to render a first video today, for TikTok — which meant two
things Spark had never had: a canvas that isn't 1920×1080, and an export
at all.

**The canvas is the document's.** `CANVAS_W`/`CANVAS_H` had been
compile-time constants read in forty places — the camera, the fit, the
prop ranges, the floor, the frustum, smart guides, where a new light
lands, even the star shader's density (`1920.0` in the WGSL). Now the
`Editor` holds `canvas: [f32; 2]`, saved as a `canvas <w> <h>` line (old
files read as the default), undoable, and it rides through the
**camera**: `Camera::canvas` is the film gate — it sets the projection's
aspect, and `Camera::stage(canvas)` puts the eye far enough back that
the gate fills the frame exactly, so a portrait comp is looked at from
further away through the same 40° lens. `Framing::paint_rect` and
`frame_scale` read the camera; the shape pass hands the canvas width to
the shader in the globals' spare slot; the layout cuts the centre column
to the canvas's aspect, so a portrait comp gets a tall viewport and the
side panels take the rest. Shapes stay where they are in canvas units
when the size changes — the frame moves around them, the way a
comp-settings change does everywhere. The **Canvas** menu (between Add
and View) is the presets with names — Landscape 1920×1080, Portrait
1080×1920, Square 1080×1080, 4K both ways — the current one lit like a
View toggle; the format takes any even size. Sizes are forced even
because `yuv420p` halves the chroma plane.

**File > Export Video…** is `frame = render(project, t)` made literal.
The export (`export/`) owns a second `Stage` and `ShapePass` at the
canvas's size — its own so the editor's window-sized cache isn't
thrashed every frame; wgpu-core dedups identical bind-group layouts, so
the meshes the editor uploaded draw through it unchanged — and renders
`Quality::Full` (the halo layer at the stage's own resolution — the
same two-layer picture the viewport shows, without the one economy)
over **opaque black**: transparency is real in the editor and a video
has nowhere to put it. One frame per 1/60 s of comp time, posed by
`set_time` + `sync_to_time`, assembled with `marks: false` (no snap
grid, no light gizmos), read back through a row-padded buffer, and sent
down a bounded channel to a writer thread feeding **FFmpeg's stdin** —
the subprocess policy, as for audio decode. The song is muxed from the
comp's file, `-ss`/`-t` cut to the same range, AAC 320k. H.264 in an
MP4 because that is what every phone app takes; the encoder is probed —
`h264_nvenc` first (this machine's FFmpeg has no libx264, and the 3080
Ti encodes faster than the frames render), then libx264, then
libopenh264. The RGB→YUV matrix is said out loud (BT.709, TV range):
left unsaid, swscale reaches for 601 while the player assumes 709 and
every neon hue drifts. The range is the **loop region** if one is set,
otherwise the whole comp — a silent comp is two minutes, so set one.
While it runs the editor is read-only (a click that moved a shape
mid-render would land in the video), the status strip counts frames,
**Esc** cancels and removes the half-file, and the transport is stopped
first. Frame rate is a constant (`export::FPS = 60`) until it earns a
place in the document. The rendering interleaves with redraws in 40 ms
slices, so the window stays alive; the playhead is put back after every
slice.

Tested at every layer: the portrait camera lands its corners on its
rectangle; `Full` is the live picture to within 3% of its light with the
body pixel exact; the FFmpeg line carries the size, the rate and the
song; and one end-to-end test renders a 256×144 comp through the GPU,
encodes it with whatever FFmpeg is installed, decodes the MP4 back and
asserts the red rectangle is red, in its quadrant, on black — which is
the test that would catch BGRA read as RGBA, a padding slip, a flipped
axis or the wrong matrix, none of which a green build can.

## The arrangement: tracks of clips (2026-08-31, same day)

Alva hit the wall the moment the first video worked: a spinning logo
meant keyframing 0°, 360°, 720°… forever, because the only timeline in
the app was the *keyframe lanes* — the inside of one comp. "This whole
time I've had access to the keyframe timeline, which is not how a whole
video should be composed." Exactly right, and exactly what the scene
decision reserved: **tracks of clips instancing comps.** The `Arrange`
tab had been sitting in the toolbar as a stub since the tabs landed;
now it's the arrangement.

**Comps are separate files** (Alva's call, over one project file that
holds everything): `asset <id> comp <path>` lines name the placed
.spark files, `clip <track> <comp> <start> <len>` lines play them, and
an optional `duration <s>` declares a comp's own length. Editing
`logo-spin.spark` once updates every project that places it, at the
price of path fragility — a clip whose file can't be read stays on the
arrangement in red saying `! missing:` rather than vanishing. A comp's
**loop period** is its declared duration, else the time of its *last
keyframe* — the natural length of the motion, no dialog to fill in —
else a second for a static comp, where it never shows.

**Evaluation is `comps::pose`**, a pure poser that does to a parsed
`Doc` what the editor's frame does to the document — curves sampled,
effects resolved, folder transforms composed, hidden shapes kept home —
at `local_time = (t − clip.start) mod period`. A two-second spin placed
for a minute spins the minute out, and `frame = render(project, t)`
never notices. Two clocks on purpose: **keyframes read local looped
time, audio-react reads song time** — the loop replays its two seconds
forever and the wub still hits on the global beat, which is the whole
reason a music-video editor gets to have loops this cheap. Placed
comps' meshes load through the normal pipeline under ids parked at
`SUB_MESH_BASE` (1 << 20), far above anything a document hands out.

**Flattened, and said out loud** (v1): a placed comp's shapes, meshes
and *lights* join the host's one scene — the same premultiplied stack
the stage already draws — rather than rendering to their own texture
and compositing as a picture. Cross-comp blend isolation and per-comp
post-FX arrive with the real compositor; the seam is the same one
folder-fade already names. One level deep for now: a placed comp's own
clips don't play (load says so at the terminal), and the self-place
recursion guard sits at the door. Clip instances aren't clickable on
the canvas — they're instances; edit the comp, not the copy.

**The tab**: tracks down the sidebar, red clip bars on the axis with a
faint tick at every loop seam, so "how many times does this play" is
visible. Drag the body to move (and between tracks), either edge to
trim, `Delete` removes, double-click opens the comp itself (the project
re-reads its placed comps on next open — a breadcrumb back is the
obvious next ask). **File > Place Comp…** drops a one-period clip at
the playhead on the first free track. Snap rides the playhead-snap
toggle. Undo covers place, drag (one step per gesture) and delete.
Export needed zero changes — clips flow through the same `assemble`.

## One project, never left (2026-08-31, still the same day)

Alva's Ableton test found the workflow lie in v1: making a comp meant
leaving the project — close, new, draw, save, reopen, place. "That
sounds truly like the worst possible workflow." It was. The fix is the
DAW's shape: **you never leave the project**, and the separate comp
files become what Ableton's samples folder is — an implementation
detail beside the project that you only notice the day you want reuse.

**The comps live beside the project.** `comps/` next to the .spark, and
every path under the project file's directory is saved **relative** to
it (the song off in ~/Music stays absolute) — a project is a folder you
can move, back up, or git whole. **File > New Comp** writes a fresh
comp there, drops a one-bar clip at the playhead and steps straight in.
**Ctrl+Shift+C — Make Comp from Selection** — is the one that matters:
draw and animate *in the project*, then hive the selection off. Its
keys shift so the first is local zero, the comp's duration is the key
span, and the clip lands exactly where the motion was, so the picture
inside the span is untouched — and outside it the piece now *ends*,
which is Alva's own model: a thing exists where its clip is. One undo
step puts it all back (the file stays; files aren't undoable). A folder
travels only whole; a partial selection leaves the folder's transform
behind — the one way the picture can shift, named in `precompose.rs`.

**Entering a comp parks the project, whole.** Double-click a clip: the
project — Editor, GPU meshes, placed comps, canvas view, unsaved
changes and all — moves onto a breadcrumb stack in RAM; the title
becomes `project.spark > comp.spark` and clicking it is Back. The song
keeps playing where it is: a comp is edited against the *project's*
track and grid, like a clip inside a Live set. Back **auto-saves the
comp** (that is what the project re-reads) and re-loads it into every
clip that plays it, GPU meshes swapped cleanly.

**Unsaved work gets a say.** The document's serialization is compared
against its last save (session lines excluded, so scrubbing never
counts); the title stars while they differ. Quit, New and Open with
unsaved changes — the whole breadcrumb, not just the doc in hand — put
a line in the status strip and go through only when the same gesture
repeats within six seconds. No dialog machinery exists in this editor,
and a two-beat confirm in the strip is honest without it. Every save
routes through `save_project`, which also writes **where work left
off**: `loop`, `playhead` and `tab` lines that a reopened project lands
back on (applied after the track re-analyzes, whose arrival resets
them). And re-analysis itself is now once per track: `spark_audio`
bakes peaks, curves and the beat grid to `~/.cache/spark-studio` keyed
by (path, size, mtime) — hand-rolled little-endian binary, a corrupt
or stale file is a miss, never an error.

Two roadmap items from the same conversation, recorded so they don't
get lost: **object lifespans** — shapes get an in/out extent inside a
comp, drawn as bars, AE-style, ending opacity-hiding forever — and a
**clip loop toggle** (Ableton's), for a one-shot that plays once and
ends. Both are the honest completion of "a thing exists where its clip
is"; neither is built yet.

## The teardown (2026-08-31, same day still)

The first render proved the engine; the first real session with it
convicted the UI. The layer-cards-and-comps editor was designed for a 2D
workflow and duct-taped 3D on after the fact — Alva's verdict, and the
sizes fight above was the evidence. So the panels are stripped to shells
and the workflow UI is gone, ~6,200 lines of it: the layer cards, the
color home and its picker, the effects browser, the tool strip, the
materials playground, and the right-panel zoom bar — whose three buttons
survived, relocated to the transport toolbar's right end. The View menu
lost Materials; `spark_ui::Layout` lost the Tools and Zoom regions.

What this era is: **the keyboard and canvas carry everything** (the
banner in `help.rs` is the honest list), the timeline block stays whole
— tabs, lanes, React sliders, arrangement, transport — and the document
model is untouched, so every comp saved before the teardown opens and
exports bit-identically. The document APIs the panels consumed (rename,
folder ops, effect toggles, color routing, scrub-field text editing in
`textbox.rs`) are kept and marked `#[allow(dead_code)]`, because the
redesign re-consumes them; deleting tested document features to quiet a
transition lint would have been the wrong trade. Temporarily without UI:
layer rename/reorder/hide, effect add/remove, visual color picking (the
palette cycle `C`, the eyedropper and the dice's saved state survive),
folder headers, and per-shape sliders. The redesign is Alva's spec —
DAW-shaped, Ableton/Resolve energy — and lands panel by panel on these
empty shells.

## The object/clip model (locked 2026-08-31, Alva's spec)

**An object is an instrument. A clip is when it plays.** The Ableton
Arrangement model, applied to a scene.

- **Objects** — shapes, meshes, lights, comp instances — own their base
  state: geometry, color, glow, opacity, effects stack, audio-react
  amounts. That is what the **inspector (right panel)** edits. Folders
  are group tracks: parent transforms with collapse.
- **One track per object**, and the timeline's track sidebar *is* the
  outliner — name, kind glyph, eye, group collapse; click a header and
  the inspector shows that object. There is no separate scene list
  (Alva: "every object gets a track, that's the list in itself"). The
  left panel stays free (future Browser candidate).
- **Clips own when and motion**: start + length on the arrangement,
  keyframes in clip-local time, a loop toggle + loop length. Left-trim
  offsets content; a non-looping clip plays once and holds its last
  pose; **no clip under the playhead = the object does not exist**.
  An object cannot overlap itself. Two clocks stay law: keys read local
  looped time, audio-react reads song time.
- **Everything clips** — lights and camera included, one rule. "Always
  on" is an untrimmed clip.
- **One timeline.** The Wave/Arrange/Keys tabs die: audio is a track
  whose clip draws the waveform; double-clicking a clip turns the
  bottom panel into that clip's curve view (the piano-roll analog),
  breadcrumb/Esc back.
- **Drawing** births an object plus a **1-bar clip** at the (snapped)
  playhead.
- **The context menu** is the tool home: clicking a tool in the RCCM
  selects it and the panel shows that tool's draw defaults — what the
  shape will look like the moment it is drawn, configurable *before*
  drawing for the first time ever. Clicking the active tool again
  deselects back to a "home" panel — the selection's verbs, as it
  turned out (*The context menu*, below). Tool clicks never close the
  menu.
- **Format v2, no migration.** Objects carry persistent ids in the
  file (clips name them); v1 files are disposable test projects by
  Alva's own call. Dropped with v1: folder keyframes (group automation
  returns properly later) — folder transforms stay, static.

Build order: ① ids + clips in the document core → ② evaluation through
clips → ③ the one timeline with the track/outliner sidebar → ④ clip
curve view → ⑤ inspector → ⑥ RCCM tool defaults.

**①–③ landed the same day.** The core's shape: `Editor` now carries
`base` (the document truth, hand edits only) beside `shapes` (the
working copies the frame reads), and `sync_to_time` runs one
**absorb → restore → apply** cycle per object per frame — absorb folds
hand edits into `base` except values the active clip's curves were
driving (preview scratch, which only a stamp may commit), restore
rewinds the working copy, apply samples the clip covering the playhead
at clip-local time. The same fold runs at the gesture seams
(`absorb_pending` in record/undo/redo/end_gesture), or a drag ending
inside one frame would compare pre-absorb truth to itself and drop its
own undo step — a bug the tests caught before it shipped. `K` stamps
diffs into the **active clip** at local time; no clip under the
playhead means absent: not drawn, not picked, no gizmo, nothing to
stamp into, and `keys/tests.rs` holds every clause. Saves are
byte-identical at any playhead because curves never touch `base`.
Structural effect ops (add/toggle/remove) write both stacks; parameter
sliders go through the absorb path.

The timeline is `arrange.rs` grown into the whole thing: object rows
(kind glyph tinted the object's colour, name, eye, folder collapse,
dimmed when absent), clip bars in the object's colour with loop-seam
ticks, comp tracks as before, the song as an audio row drawing its
waveform, one scroll. Clips drag on their own track only and clamp
against their neighbours (an object can't overlap itself — Ableton
would eat the neighbour; refusing is the honest v1); left-trim eats
content via `offset`. `L` on a selected clip toggles its loop; `Ctrl+D`
duplicates it flush after itself. Dropped with v1, awaiting the clip
view (④): key retime/copy/paste/jump and the React sliders' UI (the
amounts persist; the inspector ⑤ re-homes them). Folder *keyframes*
were dropped entirely — folders are static group transforms until group
automation is built properly.

## The glow-up (2026-08-31, Lantern Mix's look)

The material knobs that sat wired-at-zero since surfaces landed are
dialled in, to Lantern Mix's treatment (`lantern-mix/lmx_ui`, itself
descended from the VST plugins): **everything is lit from above.**
Raised faces (cards, plates, the toolbar buttons, track rows) shade
downward with a thin highlight along the top edge and float on a drop
shadow; recesses (wells, the tempo field, slider tracks) catch an inset
shadow from above and a sliver of light on the bottom lip — which took
one new shader bit, `bevel.w`, flipping the rim light to come from
below; floating panels (menus, the RCCM) sit on the deepest shadow of
the set. The window regions carry only a gentle face gradient and a
touch of grain — the card-strength ramp would band across a
1500-px-tall panel. **Spark's palette stayed Spark's**: the grey
ladder, the gold/purple accents, the float's gold seam border (the
lntrn-menu look) — the physics came over, not Lantern Mix's neutral
accent. The material playground died with the teardown and stays dead:
the look is code now, receipted by `surface.rs`'s
`the_chrome_is_lit_from_above` test and two new pixel-readback tests.

**The dial came too** (`spark_ui/knob.rs`, ported from `lmx_ui/knob.rs`
— the evolved version of the VST original, graduation ticks already
removed by Alva): groove lit from above, value arc heating toward the
pointer over its own glow — *the knobs are the one place the UI glows*,
Alva's call carried over — a cap floating on a drop shadow with a
specular catch and rim highlight, and a chicken-head pointer that
retracts to the rim as a readout fades in. The pointer needed one new
silhouette: the wedge (kind 25), pixel-tested pointing both ways. The
shader speaks linear and radial gradients only, so the angular ones
(the lit groove, the heat sweep) are CPU-segmented arcs — a dozen short
arcs per knob. The RCCM's defaults pages placed the first knobs (*The
context menu*, below); the inspector is next.

## The context menu (2026-08-31, ⑥ out of turn — Alva's call)

The build order put the RCCM last; Alva wanted it next, and it is the
first panel of the redesign to land on the empty shells. Four decisions
were theirs: **Home is the selection's verbs** (over colour, or an Add
list); **one current colour, editable on every page** — a brush, not a
per-tool memory; pages are **lean** — what the keyboard already set
after the fact, and nothing a shape can't carry; and defaults are
**session state** for now, like the dice.

**What it is.** Right-click over the viewport or a side panel: the panel
opens at the cursor, pulled on-screen rail and all — six plates down its
left flank in `1`…`6` order. A tool click arms it and the body becomes
its **draw-defaults page**: a Fill|Outline switch (a form switch on a
star field), a row or two of **knobs** — Thickness, Glow, Brightness;
Sides on a polygon; Density, Size, Glow, Twinkle, Rate, Brightness on a
field — then the seven palette chips and an HSV picker filling whatever
height the knobs left. Clicking the armed tool again is Move, and the
body is **Home**: the primary's name and nine verbs with their shortcuts
— Duplicate, Delete, Hide/Show, Copy/Paste Style, Folder, Merge/Unmerge,
Convert to Path, Make Comp — lit only where they apply. A verb acts and
closes; everything else keeps the menu up. A right-click on a shape
selects it first, so Home opens on the thing you pointed at.

**The knobs are Lantern Mix's**, on its feel: drag up to turn up, 200
logical px for the full range, Shift a tenth, the wheel a fiftieth a
notch; a quarter-per-frame crossfade fades the readout in and retracts
the pointer while the cursor is on one, asking for frames only while it
moves. Purple at the cool end heating to gold at the pointer — the
slider's ramp on a dial. A knob that turns nothing right now (an
outline's thickness on a fill) sinks under a wash of the panel and
doesn't grab. The whole cell is the grab target, which is also what
keeps grabs from overlapping.

**Defaults are a struct per tool** (`defaults.rs`), born exactly as the
tools always drew — an outline at 4, brightness 1, no halo; a line at 3;
a star field asks the renderer's own fresh field for its numbers — so a
fresh session draws what it always drew, and `draw_shape` reads them
rather than its old literals. The polygon's `sides` moved in from the
editor: `[` / `]` turn the same number the knob does, and its ceiling
went from 12 to 24 to match the keys. A tool page's colour picks set the
*draw* colour only — `load_color`, no painting of the selection — since
the page says what the next shape is born as; `C` still paints.

**The bug it found.** `fx::resolve` writes a shape's glow and gradient
from its effect stack every frame, so anything that set them on the
shape's own fields was setting them nowhere: the dice's rolled glow and
gradient (`Roll::apply`), a pasted style's, and every star field's birth
glow of 14 — dead since the effects refactor, three ways. One road now,
`Editor::write_effects`: a glow above zero adds the Glow effect at that
radius (zero holds an existing one, never removes it), a far colour adds
the Gradient wearing it, and it writes both stacks and the stamp
baseline. Birth (defaults or roll) and Paste Style go through it;
`Roll::apply` stopped touching the fields. A test holds each path.

**Geometry is asserted, not eyeballed** (`context/tests.rs`): every
tool's page fits its panel at both output scales, no two knobs overlap,
the picker is never squeezed below its floor, what lights is what clicks
(a dimmed knob doesn't), Home's rows light exactly for the selection that
has them, and the picker round-trips the palette. The panel is 420×680
logical — sized for the star field's two knob rows; shorter pages give
the air to the colour square. The rail's plates grew with it.

Not yet: persistence of the defaults (a small user file, next), Opacity
and Additive on the pages, a live preview of the shape to be drawn.

**Alva's first look (same day)** rewrote Home and shrank the rest. *The
colour picker is gone* from the pages — a permanent colour home goes in
the right panel — so a page is its switch and its knobs, and the panel
is as tall as its page: a circle's is short, a star field's is two knob
rows. *The rail is a fixed 52-px column* (it had grown with the 680-px
panel: "way too big"), top-aligned, and the panel is never shorter than
it. *Home is context-aware*: the right press captures a `Target` — what
was under the cursor, kept for the menu's life — and Home is one table
of actions per target (`context/home.rs`): empty space offers nothing
yet; an object offers Copy, Paste, Duplicate and Delete, Delete in red.
A new right-clickable thing is a `Target` variant plus its table; a new
verb is a row plus a dispatch arm; whether a row is *lit* is the
editor's state, not the table's. Folder and Merge left the menu (they
belong elsewhere); so did Hide, Convert to Path and the style pair
(their keys still work); Make Comp left the menu *and* lost
`Ctrl+Shift+C` ("physically hurts"). *And there is a real clipboard*
now — there had only ever been a style one: `Ctrl+C` copies whole
objects from the document truth (geometry, look, effects, clips and
keys, names, merge groups, path vertices), `Ctrl+V` pastes them centred
on the cursor — the menu's Paste, where it was opened — with their
clips landing at the playhead: a thing exists where its clip is, and
you paste it where you are. Copies land loose; keyed X/Y move with the
paste. The style pair moved to `Ctrl+Shift+C` / `Ctrl+Shift+V`.

**Second look, minutes later.** "The menu keeps changing sizes!! Now
it's an ugly square" — the page-sized panel was wrong: a menu has one
shape. It is the fixed **420×680** rectangle again (the rail stays the
small fixed column; short pages leave air). And **the knobs are out**:
"replace the knobs with the sliders — knobs will be used elsewhere
later." A tool page is its switch and a stack of sliders, label and
live readout on one line, the track under them, the whole band the
grab; press or drag anywhere on it, the wheel steps, Shift fine. The
dial stays in `spark_ui::knob`, homeless again, for wherever Alva
places it. Next: the inspector (⑤).

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
  spark_render    wgpu core: device/surface, camera + scene math, the
                  shape SDF pass (kinds, star fields, picking), the mesh
                  pass with lights and shadows, the stage cache, readback
  spark_assets    what comes in from disk: glTF/GLB reader on our own
                  JSON parser; images via the FFmpeg pipe
  spark_audio     FFmpeg-pipe decode, our own FFT, analysis curves, the
                  beat grid, the bake cache, cpal playback with a
                  sample-accurate clock
  spark_text      the lntrn-text wrapper — the only crate that knows the
                  text backend
  spark_ui        the editor's chrome: materials (UiRect, Surface), the
                  layout solver, widgets, the dial, the pass with its
                  pixel-readback tests
  spark_studio    the app: the document (doc/, editor/, anim/, fx),
                  history, the timeline and arrangement, scene assembly,
                  the context menu, transport, export
```

(The planned `spark_project` / `spark_fx` split never happened: the
document and the effects live in `spark_studio`, and the visualizer zoo
is milestone 4's.)

## Milestones

1. **Editor core** — the canvas is the app: draw, select, move, scale,
   rotate, and style glowing shapes in the viewport; save/load the comp.
   Then SparkUI chrome: toolbar, inspector, layer list — big text.
2. **Timeline & audio** — import a track, waveform + analysis curves, cpal
   playback and scrubbing, auto-key choreography, audio-driven bindings.
3. **Export** — pipe frames to FFmpeg → a real .mp4 with the track muxed in,
   made entirely with the tools. (First video: 2026-08-31.)
4. **FX zoo** — repeaters, paths, generators (liquid neon, raymarched
   tunnels), lasers, lightning, particle storms, transitions.
5. **The scene: meshes, lights, camera & rigs** — the comp as a 3D world
   (foundations landed 2026-08-30), GLB import, lights and the camera as
   objects with cards, then skeletal animation. (Tracks of clips
   instancing comps — the arrangement — landed 2026-08-31; footage and
   image clips next.)
6. **Excision mode** — rigged monsters pointed directly at the camera. 💀
