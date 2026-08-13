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
- The timeline holds **tracks** of **clips**. A clip is an instance of a
  **Comp** — a self-contained visualizer scene (own camera, own effect stack,
  own parameters).
- Comp parameters are animated by **curves** (keyframes with easing) and/or
  driven by audio analysis curves.
- Transitions (cuts, crossfades, luma wipes) happen where clips meet/overlap.
- Later, 3D scenes with rigged meshes are just another kind of comp.

Serialization is a hand-rolled human-readable text format (no serde). Projects
must diff cleanly in git.

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

1. **Vertical slice** — window → wgpu → load a real track → offline analysis →
   one bloomy audio-reactive comp → cpal playback with scrub → pipe frames to
   FFmpeg → a real .mp4 with the track muxed in. Thin cut through the entire
   pipeline; everything after this just widens it.
2. **Editor shell** — SparkUI: panels, viewport, timeline UI with waveform,
   inspector, keyframing. The app renders its own chrome.
3. **FX zoo** — lasers, lightning, particle storms, tunnels, camera moves,
   transitions. Enough vocabulary to make full DJ-visual style videos.
4. **Meshes & rigs** — glTF import, skeletal animation, 3D comps.
5. **Excision mode** — rigged monsters pointed directly at the camera. 💀
