//! Document-adjacent editor state: layer names, the comp's audio track,
//! the canvas's size, and save/load through the `doc` format.

use spark_render::{CANVAS, Shape};

use crate::doc::{self, MeshAsset};
use crate::props::StyleClip;

use super::Editor;

/// The larger side of an imported mesh spans this many canvas units: half
/// the canvas's height, so a logo lands big enough to see and small
/// enough to leave room around it.
pub fn mesh_fit(canvas: [f32; 2]) -> f32 {
    canvas[1] * 0.5
}

/// A mesh object for `asset`, fitted from the model's bounds (its own
/// units, Spark's frame): centred on `canvas`, its larger side spanning
/// [`mesh_fit`], its aspect kept, white — the texture is the colour.
pub fn mesh_shape(asset: u32, (lo, hi): ([f32; 3], [f32; 3]), canvas: [f32; 2]) -> Shape {
    let size = [hi[0] - lo[0], hi[1] - lo[1]];
    let k = mesh_fit(canvas) / size[0].max(size[1]).max(1e-6);
    let half = [(size[0] * k * 0.5).max(1.5), (size[1] * k * 0.5).max(1.5)];
    let mut s = Shape::mesh([canvas[0] * 0.5, canvas[1] * 0.5], half, asset).color(1.0, 1.0, 1.0);
    // The third side, at the same scale: the model as it is.
    s.set_depth((hi[2] - lo[2]) * k);
    s
}

impl Editor {
    /// Ctrl+C: remember the primary's look (never its geometry).
    pub fn copy_style(&mut self) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        let s = &self.shapes[i];
        self.style_clip = Some(StyleClip {
            rgb: s.rgb(),
            intensity: s.brightness(),
            glow: s.glow_radius(),
            thickness: s.thickness(),
            outline: s.outline(),
            additive: s.additive(),
            gradient: s.gradient(),
            rgb2: s.rgb2(),
            density: s.density(),
            twinkle: s.twinkle(),
            twinkle_rate: s.twinkle_rate(),
            star_form: s.star_form(),
        });
        println!("copied style");
        false
    }

    /// Ctrl+V: apply the remembered look to every selected shape.
    pub fn paste_style(&mut self) -> bool {
        let Some(clip) = self.style_clip.clone() else {
            return false;
        };
        if self.selection.is_empty() {
            return false;
        }
        let snap = self.snap();
        self.history.push(snap);
        for &i in &self.selection.clone() {
            // Glow and gradient are effects: they reach the stack, which
            // is what draws them — a value set on the shape's own field is
            // overwritten by `fx::resolve` before it is ever seen.
            self.write_effects(i, clip.glow, clip.gradient.then_some(clip.rgb2));
            let sh = &mut self.shapes[i];
            sh.set_rgb(clip.rgb);
            sh.set_brightness(clip.intensity);
            sh.set_additive(clip.additive);
            if let Some(o) = clip.outline {
                sh.set_outline(o);
            }
            if let Some(t) = clip.thickness {
                sh.set_thickness(t);
            }
            // The scatter itself is geometry, not look: the seed stays put,
            // so pasting a style onto a field restyles the sky you have
            // rather than swapping it for the one you copied from.
            if let Some(n) = clip.density {
                sh.set_density(n);
            }
            if let Some(v) = clip.twinkle {
                sh.set_twinkle(v);
            }
            if let Some(v) = clip.twinkle_rate {
                sh.set_twinkle_rate(v);
            }
            if let Some(f) = clip.star_form {
                sh.set_star_form(f);
            }
        }
        self.mark_posed_selection();
        println!("pasted style to {} shape(s)", self.selection.len());
        true
    }

    /// Whether Ctrl+Shift+C has a style waiting — what would light a
    /// Paste Style row, which is one table entry away in the context
    /// menu should it come back.
    #[allow(dead_code)] // kept: the style row left the menu, not the editor
    pub fn has_style_clip(&self) -> bool {
        self.style_clip.is_some()
    }

    /// The layer's user-given name ("" = auto-label).
    pub fn name(&self, i: usize) -> &str {
        self.names.get(i).map(String::as_str).unwrap_or("")
    }

    /// What to call this layer on screen: its given name, or a label from
    /// its kind and stack position. One definition, so the layer card, the
    /// keyframe lane and the status strip can't disagree about what a shape
    /// is called.
    pub fn display_name(&self, i: usize) -> String {
        let name = self.name(i);
        if !name.is_empty() {
            return name.to_string();
        }
        match self.shapes.get(i) {
            Some(s) => format!("{} {}", crate::props::kind_parts(s.kind()).1, i + 1),
            None => String::new(),
        }
    }

    #[allow(dead_code)] // kept for the redesign; the old panels were the only caller
    pub fn rename_primary(&mut self, name: String) -> bool {
        let Some(i) = self.primary() else {
            return false;
        };
        if self.names[i] == name {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        self.names[i] = name;
        true
    }

    pub fn audio_path(&self) -> Option<&str> {
        self.audio_path.as_deref()
    }

    pub fn set_audio_path(&mut self, path: Option<String>) {
        self.audio_path = path;
    }

    /// The tempo the user typed, if they have. Detection is a guess; this is
    /// the number the person who made the track knows.
    pub fn bpm_override(&self) -> Option<f32> {
        self.bpm_override
    }

    pub fn set_bpm_override(&mut self, bpm: Option<f32>) {
        self.bpm_override = bpm;
    }

    /// The comp's size: canvas units, and the video's pixels.
    pub fn canvas(&self) -> [f32; 2] {
        self.canvas
    }

    /// Canvas > a preset: resize the comp. Shapes stay where they are in
    /// canvas units — the frame moves around them, the way a comp-settings
    /// change does everywhere — so a centred circle on a landscape comp
    /// sits right of centre on a portrait one until it is moved. Undoable.
    /// Whole, even numbers: a video encoder wants both sides even, and
    /// a canvas of fractional pixels is nothing at all.
    pub fn set_canvas(&mut self, canvas: [f32; 2]) -> bool {
        let c = [
            (canvas[0] / 2.0).round().max(1.0) * 2.0,
            (canvas[1] / 2.0).round().max(1.0) * 2.0,
        ];
        if c == self.canvas {
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        self.canvas = c;
        println!("canvas {} x {}", c[0], c[1]);
        true
    }

    /// File > New: a blank comp. Undoable, like open.
    pub fn new_project(&mut self) {
        let s = self.snap();
        self.history.push(s);
        self.shapes.clear();
        self.ids.clear();
        self.paths.clear();
        self.names.clear();
        self.base.clear();
        self.clips.clear();
        self.base_fx.clear();
        self.fx.clear();
        self.group.clear();
        self.hidden.clear();
        self.folder.clear();
        self.folders.clear();
        self.selection.clear();
        self.audio_path = None;
        self.assets.clear();
        self.bpm_override = None;
        self.canvas = CANVAS;
        self.comp_assets.clear();
        self.clips.clear();
        self.duration = None;
        self.drag = None;
        self.clear_posed();
    }

    /// The models mesh shapes draw.
    pub fn assets(&self) -> &[MeshAsset] {
        &self.assets
    }

    /// Register a mesh file with the comp. The same path twice is one
    /// asset, so re-importing a logo doesn't load it twice.
    pub fn add_asset(&mut self, path: String) -> u32 {
        if let Some(a) = self.assets.iter().find(|a| a.path == path) {
            return a.id;
        }
        let id = self.assets.iter().map(|a| a.id).max().unwrap_or(0) + 1;
        self.assets.push(MeshAsset { id, path });
        id
    }

    /// Meshes drawing `asset` from before depth existed have none set:
    /// give them the model's, at their own scale, now that the model's
    /// bounds are known. Not an edit — nothing on screen changes.
    pub fn backfill_mesh_depth(&mut self, asset: u32, (lo, hi): ([f32; 3], [f32; 3])) {
        for s in &mut self.shapes {
            if s.mesh_asset() == Some(asset) && s.depth() == Some(0.0) {
                let k = s.size() / ((hi[0] - lo[0]).max(hi[1] - lo[1]) * 0.5).max(1e-6);
                s.set_depth((hi[2] - lo[2]) * k);
            }
        }
        self.clear_posed();
    }

    /// A mesh object drawing `asset`, fitted and centred (see
    /// [`mesh_shape`]), named, selected. Undoable.
    pub fn add_mesh_shape(
        &mut self,
        asset: u32,
        name: &str,
        bounds: ([f32; 3], [f32; 3]),
    ) -> usize {
        let s = self.snap();
        self.history.push(s);
        let i = self.push_shape(mesh_shape(asset, bounds, self.canvas));
        self.names[i] = name.to_string();
        self.select(Some(i));
        self.clear_posed();
        i
    }

    /// The document as it stands, ready to serialize — the one truth the
    /// file, the dirty check and precompose all read. Keyed properties
    /// bake to their t=0 pose so a save at any playhead position writes
    /// identical bytes — comps keep diffing clean in git. Session fields
    /// (loop, playhead, tab) stay `None` here: they're where work left
    /// off, not what the work is, and the dirty check must ignore them.
    pub fn to_doc(&self) -> doc::Doc {
        doc::Doc {
            // Base state is the document truth — no baking: curves live in
            // clips and never touch the base, so a save at any playhead
            // position writes identical bytes.
            shapes: self.base.clone(),
            ids: self.ids.clone(),
            paths: self.paths.clone(),
            names: self.names.clone(),
            oclips: self.clips.clone(),
            fx: self.base_fx.clone(),
            groups: self.group.clone(),
            hidden: self.hidden.clone(),
            folder: self.folder.clone(),
            folders: self.folders.clone(),
            audio: self.audio_path.clone(),
            bpm: self.bpm_override,
            assets: self.assets.clone(),
            canvas: self.canvas,
            comps: self.comp_assets.clone(),
            clips: self.comp_clips.clone(),
            duration: self.duration,
            loop_region: None,
            playhead: None,
        }
    }

    /// Save, with where work left off riding along. Paths under the
    /// file's own directory are written relative to it, so a project
    /// folder moves, backs up and gits as one unit.
    pub fn save(&self, path: &str, loop_region: Option<(f32, f32, bool)>, playhead: Option<f32>) {
        let mut d = self.to_doc();
        d.loop_region = loop_region;
        d.playhead = playhead;
        if let Some(base) = std::path::Path::new(path).parent() {
            relativize_paths(&mut d, base);
        }
        match std::fs::write(path, doc::serialize(&d)) {
            Ok(()) => println!("saved {} shapes -> {path}", self.shapes.len()),
            Err(e) => println!("save failed: {e}"),
        }
    }

    pub fn load(&mut self, path: &str) -> doc::Session {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                println!("load failed: {e}");
                return doc::Session::default();
            }
        };
        let mut d = doc::parse(&text);
        if let Some(base) = std::path::Path::new(path).parent() {
            resolve_paths(&mut d, base);
        }
        println!("loaded {} shapes from {path}", d.shapes.len());
        let session = doc::Session {
            loop_region: d.loop_region,
            playhead: d.playhead,
        };
        let s = self.snap();
        self.history.push(s);
        // v2 files carry identity; the parser fixed up anything missing or
        // duplicated. next_id resumes above everything on disk.
        self.next_id = d.ids.iter().copied().max().unwrap_or(0).max(self.next_id) + 1;
        self.ids = d.ids;
        self.shapes = d.shapes.clone();
        self.base = d.shapes;
        self.paths = d.paths;
        self.names = d.names;
        self.clips = d.oclips;
        self.fx = d.fx.clone();
        self.base_fx = d.fx;
        self.group = d.groups;
        self.hidden = d.hidden;
        self.folder = d.folder;
        self.folders = d.folders;
        self.audio_path = d.audio;
        self.assets = d.assets;
        self.bpm_override = d.bpm;
        self.canvas = d.canvas;
        self.comp_assets = d.comps;
        self.comp_clips = d.clips;
        self.duration = d.duration;
        self.selection.clear();
        self.drag = None;
        self.clear_posed();
        // Trust the file's shape order, but re-establish the invariant in
        // case it was hand-edited.
        self.normalize_folders();
        session
    }

    /// File > Save Shape...: the selection, baked at t=0, written as a
    /// mini comp (no audio, no keys) for re-import into any project.
    pub fn save_shape(&self, path: &str) {
        if self.selection.is_empty() {
            println!("nothing selected to save");
            return;
        }
        let mut idx = self.selection.clone();
        idx.sort_unstable();
        idx.dedup();
        let mut shapes = Vec::new();
        let mut paths = Vec::new();
        let mut names = Vec::new();
        let mut groups = Vec::new();
        let mut hiddens = Vec::new();
        let mut folder = Vec::new();
        for &i in &idx {
            // Base state: the object as it is, no clip motion — a saved
            // shape is a look to reuse, not a performance.
            let mut c = self.base[i];
            if let Some((id, _, _)) = c.path_meta() {
                c.set_path_start(paths.len());
                paths.push(self.paths.get(id).cloned().unwrap_or_default());
            }
            shapes.push(c);
            names.push(self.names[i].clone());
            groups.push(self.group[i]);
            hiddens.push(self.hidden[i]);
            folder.push(self.folder[i]);
        }
        let folders: Vec<_> = self
            .folders
            .iter()
            .filter(|f| folder.contains(&f.id))
            .cloned()
            .collect();
        // A saved shape carries its effects but no clips or curves.
        let stacks: Vec<_> = idx.iter().map(|&i| self.base_fx[i].clone()).collect();
        let text = doc::serialize(&doc::Doc {
            shapes: shapes.clone(),
            ids: Vec::new(),
            paths,
            names,
            oclips: vec![Vec::new(); shapes.len()],
            fx: stacks,
            groups,
            hidden: hiddens,
            folder,
            folders,
            // A saved shape carries no song, so it carries no tempo.
            audio: None,
            bpm: None,
            // But it carries the models its meshes draw.
            assets: self
                .assets
                .iter()
                .filter(|a| shapes.iter().any(|s| s.mesh_asset() == Some(a.id)))
                .cloned()
                .collect(),
            // A shape is not a comp: it has no size, arrangement,
            // length or session of its own.
            canvas: [0.0; 2],
            comps: Vec::new(),
            clips: Vec::new(),
            duration: None,
            loop_region: None,
            playhead: None,
        });
        match std::fs::write(path, text) {
            Ok(()) => println!("saved {} shape(s) -> {path}", shapes.len()),
            Err(e) => println!("shape save failed: {e}"),
        }
    }

    /// File > Import Shape...: append a saved shape file to the comp,
    /// grouping intact, and select what arrived.
    pub fn import_shapes(&mut self, path: &str) -> bool {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                println!("shape import failed: {e}");
                return false;
            }
        };
        let d = doc::parse(&text);
        let shapes = d.shapes;
        let (names, groups, hiddens) = (d.names, d.groups, d.hidden);
        if shapes.is_empty() {
            println!("no shapes in {path}");
            return false;
        }
        let s = self.snap();
        self.history.push(s);
        let path_base = self.paths.len();
        // Imported group ids land above every id already in the comp.
        let group_base = self.group.iter().copied().max().unwrap_or(0);
        let folder_base = self.folders.iter().map(|f| f.id).max().unwrap_or(0);
        // Imported models join this comp's asset list — a path already
        // here keeps its id — and the mesh shapes are repointed at them.
        let asset_map: Vec<(u32, u32)> = d
            .assets
            .iter()
            .map(|a| (a.id, self.add_asset(a.path.clone())))
            .collect();
        let start = self.shapes.len();
        for (k, mut shape) in shapes.into_iter().enumerate() {
            if let Some((id, _, _)) = shape.path_meta() {
                shape.set_path_start(path_base + id);
            }
            if let Some(old) = shape.mesh_asset()
                && let Some(&(_, new)) = asset_map.iter().find(|(o, _)| *o == old)
            {
                shape.set_mesh_asset(new);
            }
            self.shapes.push(shape);
            self.base.push(shape);
            let id = self.new_id();
            self.ids.push(id);
            self.names.push(names.get(k).cloned().unwrap_or_default());
            // An import is born like a drawing: one bar at the playhead.
            self.clips
                .push(vec![crate::doc::ObjClip::new(self.time, self.bar_s)]);
            let stack = d.fx.get(k).cloned().unwrap_or_default();
            self.fx.push(stack.clone());
            self.base_fx.push(stack);
            let g = groups.get(k).copied().unwrap_or(0);
            self.group.push(if g == 0 { 0 } else { group_base + g });
            self.hidden.push(hiddens.get(k).copied().unwrap_or(false));
            // Imported folder ids land above every id already in the comp.
            let f = d.folder.get(k).copied().unwrap_or(0);
            self.folder.push(if f == 0 { 0 } else { folder_base + f });
        }
        for f in d.folders {
            self.folders.push(crate::editor::Folder {
                id: folder_base + f.id,
                ..f
            });
        }
        self.paths.extend(d.paths);
        self.selection = (start..self.shapes.len()).collect();
        let n = self.shapes.len() - start;
        self.normalize_folders();
        println!("imported {n} shape(s) from {path}");
        true
    }
}

/// Make every asset path in `d` absolute against the file's directory.
/// A path that is already absolute passes through — every file written
/// before paths went relative keeps opening.
pub(crate) fn resolve_paths(d: &mut doc::Doc, base: &std::path::Path) {
    let fix = |p: &mut String| {
        if !std::path::Path::new(p.as_str()).is_absolute() {
            *p = base.join(p.as_str()).to_string_lossy().into_owned();
        }
    };
    if let Some(a) = &mut d.audio {
        fix(a);
    }
    for a in &mut d.assets {
        fix(&mut a.path);
    }
    for c in &mut d.comps {
        fix(&mut c.path);
    }
}

/// Write paths under the file's own directory relative to it. Anything
/// elsewhere (the song in ~/Music, say) stays absolute — relative paths
/// are for the things that travel *with* the project.
pub(crate) fn relativize_paths(d: &mut doc::Doc, base: &std::path::Path) {
    let fix = |p: &mut String| {
        if let Ok(rel) = std::path::Path::new(p.as_str()).strip_prefix(base)
            && !rel.as_os_str().is_empty()
        {
            *p = rel.to_string_lossy().into_owned();
        }
    };
    if let Some(a) = &mut d.audio {
        fix(a);
    }
    for a in &mut d.assets {
        fix(&mut a.path);
    }
    for c in &mut d.comps {
        fix(&mut c.path);
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;
    use crate::doc::{CompAsset, Doc, MeshAsset};
    use std::path::Path;

    /// Paths beside the project go relative and come back absolute; the
    /// song off in the music library stays absolute both ways.
    #[test]
    fn paths_beside_the_project_travel_with_it() {
        let base = Path::new("/home/alva/vids/drop");
        let mut d = Doc {
            audio: Some("/home/alva/Music/INFERNO.wav".into()),
            assets: vec![MeshAsset {
                id: 1,
                path: "/home/alva/vids/drop/logo.glb".into(),
            }],
            comps: vec![CompAsset {
                id: 1,
                path: "/home/alva/vids/drop/comps/spin.spark".into(),
            }],
            ..Default::default()
        };
        relativize_paths(&mut d, base);
        assert_eq!(d.assets[0].path, "logo.glb");
        assert_eq!(d.comps[0].path, "comps/spin.spark");
        assert_eq!(d.audio.as_deref(), Some("/home/alva/Music/INFERNO.wav"));
        resolve_paths(&mut d, base);
        assert_eq!(d.assets[0].path, "/home/alva/vids/drop/logo.glb");
        assert_eq!(d.comps[0].path, "/home/alva/vids/drop/comps/spin.spark");
        assert_eq!(d.audio.as_deref(), Some("/home/alva/Music/INFERNO.wav"));
    }
}
