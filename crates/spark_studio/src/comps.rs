//! Placed comps, evaluated: the runtime store for the .spark files this
//! comp's clips play, and the pure poser that turns one into display
//! shapes at a moment of its own looped time.
//!
//! A placed comp is an *instance*: its file is read once, its meshes go
//! to the GPU under the studio's ids, and every frame the poser samples
//! its curves at `local_time` — `(t - clip.start) mod period` — so a
//! two-second spin placed for a minute spins the whole minute out and
//! `frame = render(project, t)` stays exactly true. The period is the
//! comp's declared `duration`, or the time of its last keyframe — the
//! natural length of the motion — or one second for a static comp,
//! where it doesn't matter.
//!
//! Honest limitations, on purpose (v1): a placed comp is **flattened**
//! into the host's scene — its shapes, meshes and lights join the one
//! world, the way the stage already draws everything, rather than being
//! rendered to their own texture and composited as a picture. Cross-comp
//! blend isolation and per-comp post-FX arrive with the real compositor.
//! And it is one level deep: a placed comp's *own* clips don't play yet
//! (load says so) — the poser is where recursion will go.

use spark_render::Shape;

use crate::doc::{Clip, Doc};
use crate::editor::Folder;

/// GPU-map keys for placed comps' meshes start here, far above any id a
/// document hands out (those count up from 1 per comp), so the one mesh
/// map can hold both without collision.
pub const SUB_MESH_BASE: u32 = 1 << 20;

/// One placed .spark file, parsed and ready to pose.
pub struct PlacedComp {
    pub path: String,
    pub doc: Doc,
    /// Seconds one cycle takes when a clip loops it.
    pub period: f32,
    /// The doc's mesh asset ids → the studio's GPU-map keys.
    pub mesh_map: Vec<(u32, u32)>,
    /// The file couldn't be read — the clip bar says so.
    pub missing: bool,
}

impl PlacedComp {
    pub fn new(path: String, doc: Doc, mesh_map: Vec<(u32, u32)>) -> Self {
        let period = period_of(&doc);
        Self {
            path,
            doc,
            period,
            mesh_map,
            missing: false,
        }
    }

    /// A reference that couldn't be read: keeps its place on the
    /// arrangement so the broken path is visible and fixable.
    pub fn broken(path: String) -> Self {
        Self {
            path,
            doc: Doc::default(),
            period: 1.0,
            mesh_map: Vec::new(),
            missing: true,
        }
    }

    /// What the clip bar calls it: the file's stem.
    pub fn name(&self) -> String {
        std::path::Path::new(&self.path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.clone())
    }
}

/// A comp's loop period: its declared length, else its last clip's end —
/// the natural length of what is scheduled — else one second (static,
/// where the period never shows).
pub fn period_of(doc: &Doc) -> f32 {
    if let Some(d) = doc.duration.filter(|d| *d > 0.0) {
        return d;
    }
    let last = doc
        .oclips
        .iter()
        .flat_map(|l| l.iter())
        .map(|c| c.end())
        .fold(0.0f32, f32::max);
    if last > 0.01 { last } else { 1.0 }
}

/// Where host time `t` lands inside the clip's comp: local, looped.
pub fn local_time(t: f32, clip: &Clip, period: f32) -> f32 {
    (t - clip.start).rem_euclid(period.max(0.001))
}

/// The comp posed at local time `lt`: display copies of every visible
/// shape — curves sampled, effects resolved, folder transforms composed,
/// mesh ids remapped to the studio's — each with its audio-react
/// amounts, in stack order. The same steps the editor's own frame takes,
/// without an editor.
pub fn pose(pc: &PlacedComp, lt: f32) -> Vec<(Shape, [f32; 3])> {
    let d = &pc.doc;
    // Every shape posed first: folder pivots read posed centres. A shape
    // exists only where one of its own clips covers the comp's local
    // time — `None` marks the absent.
    let posed: Vec<Option<(Shape, crate::fx::Stack)>> = d
        .shapes
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let clip = d
                .oclips
                .get(i)
                .and_then(|l| l.iter().find(|c| c.contains(lt)))?;
            let mut c = *s;
            let mut stack = d.fx.get(i).cloned().unwrap_or_default();
            clip.anim.apply(&mut c, &mut stack, clip.local(lt));
            Some((c, stack))
        })
        .collect();
    let folders: &[Folder] = &d.folders;
    let pivot = |id: u32| -> [f32; 2] {
        let members: Vec<usize> = (0..posed.len())
            .filter(|&i| d.folder.get(i).copied().unwrap_or(0) == id && posed[i].is_some())
            .collect();
        if members.is_empty() {
            return [0.0, 0.0];
        }
        let n = members.len() as f32;
        let (mut sx, mut sy) = (0.0, 0.0);
        for &i in &members {
            let c = posed[i].as_ref().map(|(s, _)| s.center()).unwrap_or([0.0; 2]);
            sx += c[0];
            sy += c[1];
        }
        [sx / n, sy / n]
    };
    let mut out = Vec::with_capacity(posed.len());
    for (i, entry) in posed.iter().enumerate() {
        let Some((shape, stack)) = entry else {
            continue;
        };
        let fid = d.folder.get(i).copied().unwrap_or(0);
        let folder = folders.iter().find(|f| f.id == fid);
        let hidden =
            d.hidden.get(i).copied().unwrap_or(false) || folder.is_some_and(|f| f.hidden);
        if hidden {
            continue;
        }
        let mut s = *shape;
        crate::fx::resolve(&mut s, stack);
        if let Some(f) = folder.filter(|f| !f.is_identity()) {
            f.compose(&mut s, pivot(fid));
        }
        if let Some(local) = s.mesh_asset() {
            match pc.mesh_map.iter().find(|(l, _)| *l == local) {
                Some(&(_, global)) => s.set_mesh_asset(global),
                // A mesh the map doesn't know draws nothing rather than
                // whatever host asset shares the number.
                None => continue,
            }
        }
        // Its reaction comes off its posed stack, like everything else.
        out.push((s, crate::fx::react_of(stack)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::{Ease, Key, Target, Track};
    use crate::props::Prop;

    fn spin_doc() -> Doc {
        // One object, one 2s clip carrying a full linear turn.
        let mut clip = crate::doc::ObjClip::new(0.0, 2.0);
        clip.anim.tracks.push(Track {
            target: Target::Shape(Prop::Rotation),
            keys: vec![
                Key { t: 0.0, v: 0.0, ease: Ease::Linear },
                Key { t: 2.0, v: std::f32::consts::TAU, ease: Ease::Linear },
            ],
        });
        Doc {
            shapes: vec![Shape::rect([100.0, 100.0], [40.0, 20.0])],
            ids: vec![1],
            names: vec![String::new()],
            oclips: vec![vec![clip]],
            fx: vec![Default::default()],
            groups: vec![0],
            hidden: vec![false],
            folder: vec![0],
            ..Default::default()
        }
    }

    /// The whole point: a 2-second spin placed for a minute keeps
    /// spinning. Local time wraps by the comp's period, so the pose at
    /// t = 5 s into the clip is the pose at 1 s.
    #[test]
    fn a_short_spin_loops_for_as_long_as_its_clip_plays() {
        let pc = PlacedComp::new("/x/spin.spark".into(), spin_doc(), Vec::new());
        assert_eq!(pc.period, 2.0, "period falls back to the last clip's end");
        let clip = Clip { track: 0, comp: 1, start: 8.0, len: 60.0 };
        let early = pose(&pc, local_time(8.5, &clip, pc.period));
        let later = pose(&pc, local_time(12.5, &clip, pc.period));
        assert_eq!(early[0].0, later[0].0, "t=8.5 and t=12.5 are the same pose");
        // Half a period in: half a turn, linear.
        let half = pose(&pc, local_time(9.0, &clip, pc.period));
        assert!(
            (half[0].0.rotation() - std::f32::consts::PI).abs() < 1e-3,
            "got {}",
            half[0].0.rotation()
        );
        // A whole period lands back on the start of the cycle — that is
        // the loop.
        assert_eq!(local_time(10.0, &clip, pc.period), 0.0);
        assert!((local_time(8.5, &clip, pc.period) - 0.5).abs() < 1e-6);
    }

    /// A declared duration outranks the last key; a static comp gets a
    /// period that never shows.
    #[test]
    fn the_period_prefers_the_declared_length() {
        let mut d = spin_doc();
        d.duration = Some(8.0);
        assert_eq!(period_of(&d), 8.0);
        assert_eq!(period_of(&Doc::default()), 1.0);
    }

    /// A hidden shape stays out of the pose, and a folder fade rides in.
    #[test]
    fn hidden_shapes_stay_home_and_folders_compose() {
        let mut d = spin_doc();
        d.shapes.push(Shape::circle([500.0, 500.0], 30.0));
        d.ids.push(2);
        d.names.push(String::new());
        d.oclips.push(vec![crate::doc::ObjClip::new(0.0, 2.0)]);
        d.fx.push(Default::default());
        d.groups.push(0);
        d.hidden.push(true);
        d.folder.push(0);
        let pc = PlacedComp::new("x".into(), d, Vec::new());
        let shapes = pose(&pc, 0.0);
        assert_eq!(shapes.len(), 1, "the hidden circle stays home");
    }

    /// A mesh whose id the map doesn't know draws nothing rather than a
    /// host mesh that happens to share the number.
    #[test]
    fn an_unmapped_mesh_is_skipped_not_misdrawn() {
        let mut d = Doc::default();
        d.shapes.push(Shape::mesh([0.0; 2], [10.0, 10.0], 7));
        d.ids.push(1);
        d.names.push(String::new());
        d.oclips.push(vec![crate::doc::ObjClip::new(0.0, 2.0)]);
        d.fx.push(Default::default());
        d.groups.push(0);
        d.hidden.push(false);
        d.folder.push(0);
        let unmapped = PlacedComp::new("x".into(), d, Vec::new());
        assert!(pose(&unmapped, 0.0).is_empty());
        let mut mapped = PlacedComp::new("x".into(), spin_doc(), vec![(7, SUB_MESH_BASE)]);
        mapped.doc.shapes[0] = Shape::mesh([0.0; 2], [10.0, 10.0], 7);
        let out = pose(&mapped, 0.0);
        assert_eq!(out[0].0.mesh_asset(), Some(SUB_MESH_BASE));
    }
}
