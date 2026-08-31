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

use spark_render::{CANVAS_H, CANVAS_W, Camera, Light, Mat4, MeshInstance, Shape};

use crate::editor::Editor;
use crate::meshes::MeshAssetGpu;
use crate::overlay::Overlay;

pub struct Assembled<'a> {
    pub shapes: Vec<Shape>,
    pub models: Vec<Mat4>,
    pub paths: Vec<[f32; 2]>,
    pub meshes: Vec<MeshInstance<'a>>,
    pub lights: Vec<Light>,
}

/// Build the frame. `extra` are this frame's editor overlays beyond the
/// ones the document implies (light gizmos): the transform gizmo, the
/// floor, the frustum.
pub fn assemble<'a>(
    editor: &Editor,
    audio: Option<&spark_audio::Track>,
    meshes: &'a HashMap<u32, MeshAssetGpu>,
    camera: &Camera,
    extra: Vec<Overlay>,
) -> Assembled<'a> {
    let mut shapes = Vec::new();
    let mut overlay_n = 0;
    if editor.snap_grid {
        // Faint 60-unit grid, drawn as light under the document shapes.
        for gx in 1..(CANVAS_W / 60.0) as usize {
            let x = gx as f32 * 60.0;
            let mut l = Shape::line([x, 0.0], [x, CANVAS_H], 0.75)
                .color(1.0, 1.0, 1.0)
                .intensity(0.05)
                .glow(2.0);
            l.set_additive(true);
            shapes.push(l);
        }
        for gy in 1..(CANVAS_H / 60.0) as usize {
            let y = gy as f32 * 60.0;
            let mut l = Shape::line([0.0, y], [CANVAS_W, y], 0.75)
                .color(1.0, 1.0, 1.0)
                .intensity(0.05)
                .glow(2.0);
            l.set_additive(true);
            shapes.push(l);
        }
        overlay_n = shapes.len();
    }
    shapes.extend(editor.display_shapes());
    let n_doc = (overlay_n + editor.shapes().len()).min(shapes.len());
    if let Some(track) = audio {
        // Render-time audio reaction: the document never changes, the
        // copies drawn this frame just ride the analysis curves.
        //
        // Sampled at the playhead, not at a running player's clock. It
        // used to be gated on `is_playing()`, so parking on the drop to
        // tune a React amount showed you a shape with no reaction on it
        // — and a paused frame differed from the same frame in motion,
        // which `frame = render(project, t)` says can never happen.
        let t = editor.time();
        let c = &track.curves;
        let bass = spark_audio::Curves::sample(&c.bass, c.rate, t);
        let mid = spark_audio::Curves::sample(&c.mid, c.rate, t);
        let onset = spark_audio::Curves::sample(&c.onset, c.rate, t);
        // Skip the stage background and grid overlay. Bass moves size
        // and glow (kick/sub weight); mids carry the wobble into
        // brightness; onsets snap — each scaled by the shape's own
        // React amounts, so shapes ride the track as hard as they like.
        for (k, s) in shapes[overlay_n..n_doc].iter_mut().enumerate() {
            let r = editor.react(k);
            s.add_glow(bass * 40.0 * r[1]);
            s.add_intensity((bass * 0.3 + mid * 0.45 + onset * 0.25) * r[2]);
            s.scale_by(1.0 + bass * 0.05 * r[0]);
        }
    }
    // Flatten path vertex lists into this frame's pool, repointing each
    // display copy at its slice. The bound ratio carries any render-time
    // scaling (wub) onto the vertices themselves.
    let mut paths: Vec<[f32; 2]> = Vec::new();
    for s in &mut shapes {
        if let Some((id, _, _)) = s.path_meta() {
            let vs = editor.path(id);
            let vb = vs
                .iter()
                .map(|v| (v[0] * v[0] + v[1] * v[1]).sqrt())
                .fold(1.0f32, f32::max);
            let f = s.size() / vb.max(0.001);
            let start = paths.len();
            paths.extend(vs.iter().map(|v| [v[0] * f, v[1] * f]));
            s.set_path_start(start);
        }
    }
    // Mesh objects: one instance per primitive of every visible mesh
    // shape among the document's display copies. What the meshes are lit
    // by, from the same copies, so a keyed or reacting light shines from
    // where it is drawn.
    let mesh_instances = crate::meshes::instances(meshes, &shapes[overlay_n..n_doc]);
    let lights = crate::lights::scene_lights(&shapes[overlay_n..n_doc]);
    let gizmos = crate::lights::gizmos(&shapes[overlay_n..n_doc], camera);
    // Document shapes and the 2D overlays place their own plane; the 3D
    // overlays bring a matrix each.
    let mut models: Vec<Mat4> = shapes.iter().map(Shape::model).collect();
    for (s, m) in gizmos.into_iter().chain(extra) {
        shapes.push(s);
        models.push(m);
    }
    Assembled {
        shapes,
        models,
        paths,
        meshes: mesh_instances,
        lights,
    }
}
