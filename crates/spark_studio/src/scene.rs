//! The frame's scene, assembled: every shape the stage draws this frame
//! — the document's display copies posed at the playhead and riding the
//! track, the editor's overlays — with a matrix each, the path pool, the
//! mesh instances, and the lights.
//!
//! Split from `render` so frame assembly reads on its own, apart from the
//! passes that draw it. Document shapes place themselves by their own
//! `space`; overlays arrive with the matrix that puts them wherever in
//! the scene they belong.

use std::collections::HashMap;

use spark_render::{Camera, Light, Mat4, MeshInstance, Shape};

use crate::comps::{self, PlacedComp};
use crate::editor::Editor;
use crate::meshes::MeshAssetGpu;
use crate::overlay::Overlay;

pub struct Assembled<'a> {
    pub shapes: Vec<Shape>,
    pub models: Vec<Mat4>,
    /// Each shape's own clock (`Scene::clocks`): a document shape's
    /// clip-local time, a placed comp's shape's own clip time inside its
    /// comp, the playhead for everything else.
    pub clocks: Vec<f32>,
    pub paths: Vec<[f32; 2]>,
    pub meshes: Vec<MeshInstance<'a>>,
    pub lights: Vec<Light>,
    /// How many shapes at the end draw over everything (`Scene::over`).
    pub over: usize,
}

/// Build the frame. `levels` is the song's react curves at this
/// moment, if the song is playing then. `extra` are this frame's editor overlays beyond the
/// ones the document implies (light gizmos) that sit *in* the scene with
/// depth — the floor, the frame, the frustum; `over` are the ones drawn
/// over everything — the transform gizmo, which has to be there to grab
/// even from inside a mesh. `marks` is whether the editor's own marks —
/// the snap grid, the light gizmos — are drawn at all: the viewport
/// wants them, an export frame is the document and nothing else.
#[allow(clippy::too_many_arguments)]
pub fn assemble<'a>(
    editor: &Editor,
    levels: Option<crate::fx::Levels>,
    meshes: &'a HashMap<u32, MeshAssetGpu>,
    subcomps: &HashMap<u32, PlacedComp>,
    camera: &Camera,
    extra: Vec<Overlay>,
    over: Vec<Overlay>,
    marks: bool,
) -> Assembled<'a> {
    let mut shapes = Vec::new();
    let mut clocks: Vec<f32> = Vec::new();
    let t = editor.time();
    let mut overlay_n = 0;
    let [cw, ch] = editor.canvas();
    if editor.snap_grid && marks {
        // Faint 60-unit grid, drawn as light under the document shapes.
        for gx in 1..(cw / 60.0) as usize {
            let x = gx as f32 * 60.0;
            let mut l = Shape::line([x, 0.0], [x, ch], 0.75)
                .color(1.0, 1.0, 1.0)
                .intensity(0.05)
                .glow(2.0);
            l.set_additive(true);
            shapes.push(l);
        }
        for gy in 1..(ch / 60.0) as usize {
            let y = gy as f32 * 60.0;
            let mut l = Shape::line([0.0, y], [cw, y], 0.75)
                .color(1.0, 1.0, 1.0)
                .intensity(0.05)
                .glow(2.0);
            l.set_additive(true);
            shapes.push(l);
        }
        overlay_n = shapes.len();
    }
    // Render-time audio reaction: the document never changes, the copies
    // drawn this frame just ride the analysis curves — each setting with
    // a reaction on it, by its own trigger and intensity (`fx::react`).
    //
    // `levels` is the song at the playhead, read through the song's
    // clip by the studio (`Studio::levels_at`) — none where the song
    // isn't playing. Sampled at the playhead, not at a running player's
    // clock: a paused frame reads the same as the same frame in motion,
    // which `frame = render(project, t)` says it must.
    // The grid lines above run on the playhead; the document's shapes
    // each on their clip's clock; the ants and guides after them on the
    // playhead again.
    clocks.resize(overlay_n, t);
    clocks.extend(editor.clocks());
    shapes.extend(editor.display_shapes(levels));
    let n_doc = (overlay_n + editor.shapes().len()).min(shapes.len());
    clocks.resize(shapes.len(), t);
    // Flatten path vertex lists into this frame's pool, repointing each
    // display copy at its slice. The bound ratio carries any render-time
    // scaling (wub) onto the vertices themselves.
    let mut paths: Vec<[f32; 2]> = Vec::new();
    let flatten = |s: &mut Shape, vs: &[[f32; 2]], pool: &mut Vec<[f32; 2]>| {
        let vb = vs
            .iter()
            .map(|v| (v[0] * v[0] + v[1] * v[1]).sqrt())
            .fold(1.0f32, f32::max);
        let f = s.size() / vb.max(0.001);
        let start = pool.len();
        pool.extend(vs.iter().map(|v| [v[0] * f, v[1] * f]));
        s.set_path_start(start);
    };
    for s in &mut shapes {
        if let Some((id, _, _)) = s.path_meta() {
            flatten(s, editor.path(id), &mut paths);
        }
    }
    // Clips: every placed comp playing at this moment joins the frame,
    // posed at its own looped local time (see `comps`). Track order
    // stacks low to high, so a higher track draws over — and the audio
    // reaction samples *global* time: the loop replays its two seconds
    // forever, the wub still hits on the song's beat. Inserted before
    // the selection ants and guides so the editor's marks stay on top.
    let mut playing: Vec<&crate::doc::Clip> = editor
        .comp_clips()
        .iter()
        .filter(|c| t >= c.start && t < c.start + c.len)
        .collect();
    playing.sort_by_key(|c| c.track);
    let mut clip_shapes: Vec<Shape> = Vec::new();
    let mut clip_clocks: Vec<f32> = Vec::new();
    for clip in playing {
        let Some(pc) = subcomps.get(&clip.comp) else {
            continue;
        };
        if pc.missing {
            continue;
        }
        let lt = comps::local_time(t, clip, pc.period);
        for (mut s, clock) in comps::pose_clocked(pc, lt, levels, editor.canvas()) {
            if let Some((id, _, _)) = s.path_meta() {
                let vs = pc.doc.paths.get(id).map(Vec::as_slice).unwrap_or(&[]);
                flatten(&mut s, vs, &mut paths);
            }
            clip_shapes.push(s);
            clip_clocks.push(clock);
        }
    }
    let n_clips = clip_shapes.len();
    shapes.splice(n_doc..n_doc, clip_shapes);
    clocks.splice(n_doc..n_doc, clip_clocks);
    // Mesh objects: one instance per primitive of every visible mesh
    // shape among the document's display copies — placed comps' included,
    // so a spinning logo mesh spins on the arrangement. What the meshes
    // are lit by, from the same copies (a placed comp's lights join the
    // one scene — flattening's semantics, see `comps`); the editor's
    // light gizmos mark only the host's own lights.
    let n_world = n_doc + n_clips;
    let mesh_instances = crate::meshes::instances(meshes, &shapes, overlay_n..n_world);
    let lights = crate::lights::scene_lights(&shapes[overlay_n..n_world]);
    let gizmos = if marks {
        crate::lights::gizmos(&shapes[overlay_n..n_doc], camera)
    } else {
        Vec::new()
    };
    // Document shapes and the 2D overlays place their own plane; the 3D
    // overlays bring a matrix each.
    let mut models: Vec<Mat4> = shapes.iter().map(Shape::model).collect();
    let over_n = over.len();
    for (s, m) in gizmos.into_iter().chain(extra).chain(over) {
        shapes.push(s);
        models.push(m);
        clocks.push(t);
    }
    Assembled {
        shapes,
        models,
        clocks,
        paths,
        meshes: mesh_instances,
        lights,
        over: over_n,
    }
}
