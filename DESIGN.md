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

## Document model: Timeline + Scenes

- A **Project** owns assets (audio track, later: meshes, images) and one master
  **Timeline** locked to the song.
- The timeline holds **tracks** of **clips**. A clip is an instance of a
  **Scene** — one stage of the video, holding layers back-to-front.
- Anything animatable is driven by **curves** (keyframes with easing, posed
  via auto-key) and/or by audio analysis curves.
- Transitions (cuts, crossfades, luma wipes) happen where clips meet/overlap.

Serialization is a hand-rolled human-readable text format (no serde). Projects
must diff cleanly in git.

## Scenes & layers: canvas-first

Home base is direct manipulation: draw a shape, grab it, move it, pose it at
two moments and let auto-key fly it between them. The engine core (curves,
timeline, post chain, export) never cares what a layer draws. Layer kinds
arrive in this order:

1. **Shape layers** (first) — hand-drawn glowing primitives: circle, box,
   regular polygon, line, path. Fill + stroke + glow, SDF-rendered so the
   neon look is native, not a filter. Select/move/rotate/scale with the
   mouse; duplicate + repeaters for instant symmetry.
2. **Generator layers** — procedural backdrops (liquid neon, plasma, glow
   fields) and raymarched flythroughs (SDF tunnels — 3D on screen, zero mesh
   code). Knobs, not brushes; seasoning behind the hand-made foreground.
3. **Scene-3D layers** (later, additive) — real camera + instanced geometry +
   particles + depth buffer.
4. **Rigged mesh layers** (much later) — imported meshes, skeletons,
   character animation.

The look that sells all of it — bloom, glow, DOF, grade — is the shared post
chain, and it works identically on every layer kind.

## Dependency policy

We build our own everything, except where it's genuinely unreasonable:

| Allowed | Why |
|---|---|
| `wgpu` | The GPU API. Given. |
| FFmpeg (subprocess, not linked) | Video encode, audio file decode. Piped via stdin/stdout. |
| `winit` | Wayland/X11 windowing is protocol hell with zero creative payoff. |
| `cpal` | Audio *output* device access only. Decode is FFmpeg's job. |
| `fontdue` (or ttf-parser) | TTF rasterization for editor text. |
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

1. **Canvas slice** — draw glowing shapes on the canvas, move/scale/rotate
   them with the mouse, auto-key two poses, load a real track (analysis +
   cpal playback), bind glow to bass, pipe frames to FFmpeg → a real .mp4 of
   shapes Alva drew dancing to Alva's track. Thin cut through the entire
   pipeline; everything after this just widens it.
2. **Editor shell** — SparkUI: panels, timeline UI with waveform, inspector,
   layer list, curve editor. The app renders its own chrome.
3. **FX zoo** — repeaters, paths, generators (liquid neon, raymarched
   tunnels), lasers, lightning, particle storms, transitions.
4. **Scene-3D, meshes & rigs** — real camera + instanced geometry layers,
   then glTF import and skeletal animation.
5. **Excision mode** — rigged monsters pointed directly at the camera. 💀
