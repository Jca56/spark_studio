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

## Document model: Timeline + Comps

- A **Project** owns assets (audio track, later: meshes, images) and one master
  **Timeline** locked to the song.
- A **Comp** (composition) is an ordered stack of layers rendered
  back-to-front into one image, with its own parameter set, coordinate space,
  and duration. Rendering a comp is pure: comp × time → frame.
- The timeline holds **tracks** of **clips**; a clip instances a comp over a
  time range, mapping timeline time onto the comp's local time.
- Anything animatable is driven by **curves** (keyframes with easing,
  stamped deliberately from the canvas pose — no auto-key) and/or by audio
  analysis curves.
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
   regular polygon, line, path. Fill + stroke + glow, SDF-rendered so the
   neon look is native, not a filter. Select/move/rotate/scale with the
   mouse; duplicate + repeaters for instant symmetry.
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
that doesn't crop what it marks), or straddling. Because every effect derives
from one silhouette function, icon glyphs get borders, glows and gradients
for free, and `UiRect::line` finally draws a diagonal. Instance data moved
from vertex attributes to a **storage buffer** indexed by `instance_index`:
attributes cap at 16 slots / 60 inter-stage components, which the material
set would have hit immediately, so the ceiling is gone and new material
fields never touch the pipeline. Every parameter defaults to zero and zero
means off. Fake borders are banned — `.stroke()` is the only edge.

Theme: dark charcoal chrome — explicitly NOT the Lantern warm-brown; Spark
has its own identity. Logic-Pro-dark energy with colorful accents to come.
Big text and controls always.

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
gradient endpoint, or the draw color when nothing's selected), **layer
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

Gradient fills (first of the background-tools wave — starfield, symmetry,
grid arrays, and noise textures are queued): any shape can carry a
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
  spark_render    wgpu core: device/surface, offscreen HDR targets,
                  post-fx chain (bloom, tonemap, grade), frame capture
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
