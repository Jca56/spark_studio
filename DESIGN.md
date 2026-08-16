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
- Anything animatable is driven by **curves** (keyframes with easing, posed
  via auto-key) and/or by audio analysis curves.
- Transitions (cuts, crossfades, luma wipes) happen where clips meet/overlap.

Serialization is a hand-rolled human-readable text format (no serde). Projects
must diff cleanly in git.

## Comps & layers: canvas-first

Home base is direct manipulation: draw a shape, grab it, move it, pose it at
two moments and let auto-key fly it between them. Build-order rule: **tools
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
3. **3D layers** (later, additive) — real camera + instanced geometry +
   particles + depth buffer.
4. **Rigged mesh layers** (much later) — imported meshes, skeletons,
   character animation.

The look that sells all of it — bloom, glow, DOF, grade — is the shared post
chain, and it works identically on every layer kind.

## SparkUI

The engine draws its own editor. Build order: (1) flat rects mapping out the
layout, (2) container/grid layout framework, (3) reusable widget suite.
Text: external font rasterizer for now (fontdue-class); planned swap to
Alva's own lntrn-type once it matures.

Theme: dark charcoal chrome — explicitly NOT the Lantern warm-brown; Spark
has its own identity. Logic-Pro-dark energy with colorful accents to come.
Big text and controls always.

Title bar: our own (window decorations off) — controls at the far right,
drag zone everywhere else. **No double-click behaviors on the title bar,
ever.** Edge-resize handles for the borderless window: todo.

Text (adopted 2026-08-15): **lntrn-type**, Alva's own engine, at Phase 4
(parsing, rasterization, discovery, full layout API, gamma-correct AA) —
its first field test outside Lantern. All call sites go through the
`spark_text` wrapper crate so backend evolution never touches widget code.
UI face: bundled Atkinson Hyperlegible (OFL) — designed for low-vision
readability. Kerning/ligatures arrive free when lntrn-type reaches Phase 5+.

Layout: slim top toolbar; left all-purpose panel (comps / layers / assets);
right inspector; **full-width timeline** along the bottom (time deserves
every horizontal pixel); the remaining center is the viewport, canvas
aspect-fit. Rendering is event-driven — the app redraws only when state
changes (playback later drives continuous redraw only while playing).

## Dependency policy

We build our own everything, except where it's genuinely unreasonable:

| Allowed | Why |
|---|---|
| `wgpu` | The GPU API. Given. |
| FFmpeg (subprocess, not linked) | Video encode, audio file decode. Piped via stdin/stdout. |
| `winit` | Wayland/X11 windowing is protocol hell with zero creative payoff. |
| `cpal` | Audio *output* device access only. Decode is FFmpeg's job. |
| `lntrn-type` (path dep) | Text: Alva's own engine, adopted at Phase 4. Wrapped behind `spark_text` — the only crate that knows the backend. |
| `glam`, `bytemuck` | Math + GPU byte-casting. Buildable ourselves; not worth the early time. |

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
