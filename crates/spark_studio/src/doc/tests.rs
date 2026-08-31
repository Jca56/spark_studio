//! Format tests: every past line width still opens, and everything the
//! editor can express survives a round trip.
//!
//! Split from the parser so both stay inside the file budget — the format
//! has grown four shape-line eras and a folder era, and each one that still
//! has to read is a test that has to stay.

use super::*;
use crate::editor::Folder;
use crate::props::Prop;

#[test]
fn anim_round_trip() {
    let mut shape = Shape::circle([100.0, 200.0], 50.0);
    shape.set_gradient(true);
    shape.set_rgb2([0.1, 0.2, 0.3]);
    let shapes = vec![shape];
    let mut a = ShapeAnim::default();
    a.tracks.push(Track {
        target: Target::Shape(Prop::X),
        keys: vec![
            Key {
                t: 1.0,
                v: 100.0,
                ease: Ease::Smooth,
            },
            Key {
                t: 3.0,
                v: 500.0,
                ease: Ease::Linear,
            },
        ],
    });
    let text = serialize(&Doc {
        shapes,
        names: vec![String::new()],
        anims: vec![a.clone()],
        reacts: vec![[1.0, 0.5, 2.0]],
        groups: vec![3],
        hidden: vec![true],
        folder: vec![7],
        folders: vec![{
            let mut f = Folder::new(7, "Drop stuff".into());
            f.collapsed = true;
            f.hidden = true;
            f.x = 120.0;
            f.rotation = 0.5;
            f.scale = 1.75;
            f
        }],
        audio: Some("x.mp3".into()),
        ..Default::default()
    });
    let d = parse(&text);
    assert_eq!(d.shapes.len(), 1);
    assert_eq!(d.audio.as_deref(), Some("x.mp3"));
    assert_eq!(d.anims[0], a);
    assert_eq!(d.reacts[0], [1.0, 0.5, 2.0]);
    assert_eq!(d.groups[0], 3);
    assert!(d.shapes[0].gradient());
    assert_eq!(d.shapes[0].rgb2(), [0.1, 0.2, 0.3]);
    assert!(d.hidden[0]);
    // Folders round-trip whole: membership, name with a space, flags.
    assert_eq!(d.folder[0], 7);
    assert_eq!(d.folders.len(), 1);
    assert_eq!(d.folders[0].name, "Drop stuff");
    assert!(d.folders[0].collapsed && d.folders[0].hidden);
    // The transform rides along with the rest.
    assert_eq!(d.folders[0].x, 120.0);
    assert_eq!(d.folders[0].rotation, 0.5);
    assert_eq!(d.folders[0].scale, 1.75);
}

#[test]
fn folders_from_before_the_transform_read_as_identity() {
    // A folderdef written before folders had a transform: three fields,
    // then the name. It must not swallow the name as coordinates.
    let text = "spark-comp v1\nfolderdef 2 c h Old Name Here\n\
                0 0 100 100 0 0 0 1 1 1 1 30 1.4 4\nfolder 2\n";
    let d = parse(text);
    assert_eq!(d.folders.len(), 1);
    let f = &d.folders[0];
    assert_eq!(f.name, "Old Name Here");
    assert!(f.collapsed && f.hidden);
    assert!(f.is_identity(), "no transform means identity, not garbage");
}

/// A star field is entirely in its numbers — nobody placed those stars,
/// so if the seed or the form doesn't survive a save the comp reopens as
/// a different sky.
#[test]
fn star_fields_round_trip() {
    let mut s = Shape::stars([400.0, 300.0], [200.0, 150.0], 42.0);
    s.set_density(75.0);
    s.set_twinkle(0.8);
    s.set_twinkle_rate(5.5);
    s.set_star_form(2);
    s.set_thickness(6.0);
    let text = serialize(&Doc {
        shapes: vec![s],
        names: vec![String::new()],
        anims: vec![ShapeAnim::default()],
        reacts: vec![[1.0; 3]],
        groups: vec![0],
        hidden: vec![false],
        folder: vec![0],
        ..Default::default()
    });
    let back = parse(&text).shapes.remove(0);
    assert_eq!(back.kind(), spark_render::ShapeKind::Stars);
    assert_eq!(back.seed(), Some(42.0));
    assert_eq!(back.density(), Some(75.0));
    assert_eq!(back.twinkle(), Some(0.8));
    assert_eq!(back.twinkle_rate(), Some(5.5));
    assert_eq!(back.star_form(), Some(2));
    assert_eq!(back.thickness(), Some(6.0), "star size");
    assert_eq!(back.box_size(), Some([400.0, 300.0]), "the region");
}

/// A typed tempo overrides detection, so it has to survive the file —
/// otherwise the correction is retyped every time the comp opens.
#[test]
fn a_typed_tempo_round_trips() {
    let text = serialize(&Doc {
        audio: Some("track.wav".into()),
        bpm: Some(140.0),
        ..Default::default()
    });
    assert_eq!(parse(&text).bpm, Some(140.0));
    // And a comp that never had one stays untouched, so detection keeps
    // getting to answer.
    let plain = serialize(&Doc {
        audio: Some("track.wav".into()),
        ..Default::default()
    });
    assert!(!plain.contains("bpm"), "wrote a tempo nobody set");
    assert_eq!(parse(&plain).bpm, None);
}

/// The shape line grew from 18 floats to 22 for star fields. Comps
/// written before that still have to open, with the new tail at zero.
#[test]
fn eighteen_float_files_still_read() {
    // A gradient-era circle: 18 numbers, no `extra`.
    let text = "spark-comp v1\n\
                0 0 100 200 50 50 1 0 0 1.4 30 4 0 0 0 0.5 1 1\n";
    let d = parse(text);
    assert_eq!(d.shapes.len(), 1, "an 18-float line still parses");
    assert_eq!(d.shapes[0].center(), [100.0, 200.0]);
    assert!(d.shapes[0].gradient());
    assert_eq!(d.shapes[0].seed(), None, "and it isn't a star field");
}

/// ...and 22 to 26 for opacity, which is the one field a zeroed tail
/// would get *wrong*: a comp written before shapes could fade is a comp
/// where every shape is solid, so reading the gap as zero would open
/// every old project blank.
#[test]
fn files_from_before_opacity_open_solid() {
    let text = "spark-comp v1\n\
                0 0 100 200 50 50 1 0 0 1.4 30 4 0 0 0 0.5 1 1 0 0 0 0\n";
    let d = parse(text);
    assert_eq!(d.shapes.len(), 1, "a 22-float line still parses");
    assert_eq!(d.shapes[0].opacity(), 1.0, "an old shape opened faded out");
    // The eras before it too, all the way back.
    let old = parse("spark-comp v1\n0 0 100 100 0 0 0 1 1 1 1 30 1.4 4\n");
    assert_eq!(old.shapes[0].opacity(), 1.0);
}

/// And a line that carries the field means what it says.
#[test]
fn a_saved_fade_survives_a_round_trip() {
    let mut d = parse("spark-comp v1\n0 0 100 100 0 0 1 1 1 1 0 0 0 0\n");
    d.shapes[0].set_opacity(0.3);
    let back = parse(&serialize(&d));
    assert!(
        (back.shapes[0].opacity() - 0.3).abs() < 1e-6,
        "opacity did not survive save/load"
    );
}

/// A folder's fade rides its own line rather than a ninth column on
/// `folderdef`, because the name there runs to end of line: a folder
/// actually named "1" would otherwise be read as an opacity and lose
/// its name.
#[test]
fn a_folder_fade_survives_a_round_trip_and_so_does_a_numeric_name() {
    let mut d = parse(
        "spark-comp v1\n0 0 100 100 0 0 1 1 1 1 0 0 0 0\n\
                       0 0 200 200 0 0 1 1 1 1 0 0 0 0\n",
    );
    // A member each, or the loader prunes them as ghost rows.
    d.folder = vec![1, 2];
    let mut f = Folder::new(1, "1".to_string());
    f.opacity = 0.25;
    d.folders.push(f);
    d.folders.push(Folder::new(2, "solid".to_string()));
    let text = serialize(&d);
    assert!(
        !text.contains("folderfade") || text.matches("folderfade").count() == 1,
        "a solid folder wrote a fade line it didn't need"
    );
    let back = parse(&text);
    assert_eq!(back.folders[0].opacity, 0.25, "the fade did not survive");
    assert_eq!(back.folders[0].name, "1", "the name was eaten by the fade");
    assert_eq!(
        back.folders[1].opacity, 1.0,
        "a folder without one is solid"
    );
}

/// Additive was an effect for exactly one day. A comp that stacked one
/// has to keep its pure light: the effect carried the truth while the
/// shape's own field sat dead under it, so dropping the line as an
/// unknown tag would turn every additive shape back into an occluding
/// one.
#[test]
fn an_old_additive_effect_becomes_the_shapes_own_setting() {
    let d = parse(
        "spark-comp v1\n0 0 100 100 0 0 1 1 1 1 0 0 0 0\n\
         fx 1 add on 1\n",
    );
    assert!(d.shapes[0].additive(), "the shape stopped being pure light");
    assert!(
        d.fx[0].effects.is_empty(),
        "the effect came back as a stack entry too"
    );
    // ...and one that was switched off stays off.
    let off = parse(
        "spark-comp v1\n0 0 100 100 0 0 1 1 1 1 0 0 0 0\n\
         fx 1 add off 1\n",
    );
    assert!(!off.shapes[0].additive());
}

/// The Brightness effect never did anything — `fx::resolve` never read
/// it, while the shape's own brightness slider did the work. Its lines
/// go quietly, and must not resurrect as a nameless entry in the stack.
#[test]
fn an_old_brightness_effect_is_dropped() {
    let d = parse(
        "spark-comp v1\n0 0 100 100 0 0 1 1 1 2 0 0 0 0\n\
         fx 1 bright on 1.4\n",
    );
    assert!(d.fx[0].effects.is_empty(), "a dead effect came back");
    assert_eq!(d.shapes[0].brightness(), 2.0, "the real setting survived");
}

#[test]
fn old_files_without_folders_still_read() {
    // A v1 file predating folders: every shape loose, no folderdefs.
    let text = "spark-comp v1\n0 0 100 100 0 0 0 1 1 1 1 30 1.4 4\n";
    let d = parse(text);
    assert_eq!(d.shapes.len(), 1);
    assert_eq!(d.folder, vec![0]);
    assert!(d.folders.is_empty());
}

#[test]
fn ghost_folderdefs_are_dropped() {
    // A folderdef whose members are all gone would draw an empty row.
    let text = "spark-comp v1\nfolderdef 4 e v Orphan\n\
                0 0 100 100 0 0 0 1 1 1 1 30 1.4 4\n";
    let d = parse(text);
    assert!(d.folders.is_empty());
}

/// Effects and the curves that drive them both have to survive the
/// file. A stack that doesn't round-trip is a comp that reopens looking
/// different; a curve whose target doesn't round-trip is an animation
/// that silently stops.
#[test]
fn effects_and_their_curves_round_trip() {
    let mut stack = Stack::default();
    let glow = stack.add(EffectKind::Glow, stack.next_id());
    stack.find_mut(glow).unwrap().set(0, 85.0);
    let grad = stack.add(EffectKind::Gradient, stack.next_id());
    stack.find_mut(grad).unwrap().set(1, 0.5);
    stack.find_mut(grad).unwrap().on = false;

    let mut a = ShapeAnim::default();
    a.tracks.push(Track {
        target: Target::Effect { id: glow, param: 0 },
        keys: vec![
            Key {
                t: 0.0,
                v: 0.0,
                ease: Ease::Smooth,
            },
            Key {
                t: 4.0,
                v: 85.0,
                ease: Ease::Linear,
            },
        ],
    });

    let text = serialize(&Doc {
        shapes: vec![Shape::circle([10.0, 20.0], 5.0)],
        names: vec![String::new()],
        anims: vec![a.clone()],
        fx: vec![stack.clone()],
        reacts: vec![[1.0; 3]],
        groups: vec![0],
        hidden: vec![false],
        folder: vec![0],
        ..Default::default()
    });
    let d = parse(&text);
    assert_eq!(d.fx.len(), 1);
    assert_eq!(d.fx[0], stack, "the stack came back different");
    assert_eq!(d.anims[0], a, "the curve's target came back different");
    // And an effect id survives as an id, not as a position.
    assert_ne!(glow, grad);
    assert!(d.fx[0].find(glow).is_some() && d.fx[0].find(grad).is_some());
}

#[test]
fn track_sampling() {
    let tr = Track {
        target: Target::Shape(Prop::Brightness),
        keys: vec![
            Key {
                t: 1.0,
                v: 10.0,
                ease: Ease::Smooth,
            },
            Key {
                t: 3.0,
                v: 20.0,
                ease: Ease::Smooth,
            },
        ],
    };
    // Clamped outside, exact midpoint in the middle (smoothstep(0.5)=0.5).
    assert_eq!(tr.sample(0.0), Some(10.0));
    assert_eq!(tr.sample(9.0), Some(20.0));
    assert_eq!(tr.sample(2.0), Some(15.0));
    // Smooth eases: quarter-way in time is less than quarter-way in value.
    assert!(tr.sample(1.5).unwrap() < 12.5);
}

/// `asset` lines carry the models mesh shapes draw, path and all — spaces
/// included — and a mesh shape's line names its asset like any other
/// field.
#[test]
fn mesh_assets_ride_the_format() {
    let mut doc = super::Doc::default();
    doc.assets.push(super::MeshAsset {
        id: 3,
        path: "/home/alva/my logo.glb".into(),
    });
    doc.shapes.push(spark_render::Shape::mesh([960.0, 540.0], [270.0, 137.0], 3));
    doc.names.push("logo".into());
    doc.anims.push(Default::default());
    doc.fx.push(Default::default());
    doc.reacts.push([1.0; 3]);
    doc.groups.push(0);
    doc.hidden.push(false);
    doc.folder.push(0);
    let text = super::serialize(&doc);
    assert!(text.contains("asset 3 mesh /home/alva/my logo.glb\n"), "{text}");
    let back = super::parse(&text);
    assert_eq!(back.assets, doc.assets);
    assert_eq!(back.shapes.len(), 1);
    assert_eq!(back.shapes[0].mesh_asset(), Some(3));
    assert_eq!(back.shapes[0].mesh_half(), Some([270.0, 137.0]));
    assert_eq!(back.names[0], "logo");
    // An asset kind this build doesn't know is skipped, not misread.
    let odd = super::parse("spark-comp v1\nasset 9 image /x.png\nasset 4 mesh /y.glb\n");
    assert_eq!(odd.assets.len(), 1);
    assert_eq!(odd.assets[0].id, 4);
}

/// A light is a shape line like any other: its kind, cone and aim ride
/// the floats, and it comes back as the same light.
#[test]
fn lights_ride_the_format() {
    let mut doc = super::Doc::default();
    let mut spot = spark_render::Shape::light([300.0, 200.0], spark_render::LightKind::Spot)
        .color(1.0, 0.5, 0.0);
    spot.set_cone(45.0);
    spot.set_z(400.0);
    spot.set_tilt(0.3);
    doc.shapes.push(spot);
    doc.names.push("spot light".into());
    doc.anims.push(Default::default());
    doc.fx.push(Default::default());
    doc.reacts.push([1.0; 3]);
    doc.groups.push(0);
    doc.hidden.push(false);
    doc.folder.push(0);
    let back = super::parse(&super::serialize(&doc));
    let l = back.shapes[0].as_light().expect("still a light");
    assert_eq!(l.kind, spark_render::LightKind::Spot);
    assert!((l.cone - 45f32.to_radians()).abs() < 1e-6);
    assert_eq!(l.color, [1.0, 0.5, 0.0]);
    assert_eq!(l.position.z, 400.0);
    assert_eq!(back.names[0], "spot light");
}

/// The comp's size rides the format. A file from before comps had one —
/// every file until today — opens at the default; a portrait comp comes
/// back portrait; and a saved shape, which is not a comp, writes no size
/// at all.
#[test]
fn the_canvas_size_rides_the_format() {
    let old = "spark-comp v1\n";
    assert_eq!(parse(old).canvas, spark_render::CANVAS);
    let text = serialize(&Doc {
        canvas: [1080.0, 1920.0],
        ..Default::default()
    });
    assert!(text.contains("canvas 1080 1920\n"), "{text}");
    assert_eq!(parse(&text).canvas, [1080.0, 1920.0]);
    // A shape file: no size, and nothing on read to trip over.
    let shape = serialize(&Doc {
        canvas: [0.0; 2],
        ..Default::default()
    });
    assert!(!shape.contains("canvas"), "{shape}");
    assert_eq!(parse(&shape).canvas, spark_render::CANVAS);
    // Nonsense keeps the default rather than making a zero-sized comp.
    assert_eq!(parse("spark-comp v1\ncanvas 0 0\n").canvas, spark_render::CANVAS);
    assert_eq!(parse("spark-comp v1\ncanvas wide\n").canvas, spark_render::CANVAS);
}

/// The arrangement rides the format: placed comps as `asset ... comp`
/// lines, clips as `clip` lines, and an explicit length as `duration`.
/// Old files read as a comp with no arrangement, and an unknown asset
/// kind is still skipped rather than misread.
#[test]
fn clips_and_placed_comps_ride_the_format() {
    let text = serialize(&Doc {
        comps: vec![CompAsset {
            id: 3,
            path: "/comps/logo spin.spark".into(),
        }],
        clips: vec![Clip {
            track: 1,
            comp: 3,
            start: 8.0,
            len: 16.0,
        }],
        duration: Some(2.0),
        ..Default::default()
    });
    assert!(text.contains("asset 3 comp /comps/logo spin.spark\n"), "{text}");
    assert!(text.contains("clip 1 3 8 16\n"), "{text}");
    assert!(text.contains("duration 2\n"), "{text}");
    let d = parse(&text);
    assert_eq!(d.comps.len(), 1);
    assert_eq!(d.comps[0].path, "/comps/logo spin.spark");
    assert_eq!(d.clips, vec![Clip { track: 1, comp: 3, start: 8.0, len: 16.0 }]);
    assert_eq!(d.duration, Some(2.0));
    // Old files: no arrangement, derived length.
    let old = parse("spark-comp v1\n");
    assert!(old.comps.is_empty() && old.clips.is_empty() && old.duration.is_none());
    // A zero-length clip is not a clip; a nonsense duration is none.
    let junk = parse("spark-comp v1\nclip 0 1 4 0\nduration -3\nasset 9 image /x.png\n");
    assert!(junk.clips.is_empty() && junk.duration.is_none() && junk.assets.is_empty());
}

/// Session state rides the file — the loop region, the playhead and the
/// active tab come back next session — and a file without them (every
/// file until today) reads as none.
#[test]
fn where_work_left_off_rides_the_format() {
    let text = serialize(&Doc {
        loop_region: Some((8.0, 16.0, true)),
        playhead: Some(12.5),
        tab: Some("arrange".into()),
        ..Default::default()
    });
    assert!(text.contains("loop 8 16 1\n"), "{text}");
    assert!(text.contains("playhead 12.5\n"), "{text}");
    assert!(text.contains("tab arrange\n"), "{text}");
    let d = parse(&text);
    assert_eq!(d.loop_region, Some((8.0, 16.0, true)));
    assert_eq!(d.playhead, Some(12.5));
    assert_eq!(d.tab.as_deref(), Some("arrange"));
    let old = parse("spark-comp v1\n");
    assert!(old.loop_region.is_none() && old.playhead.is_none() && old.tab.is_none());
    // A backwards region is not a loop.
    assert!(parse("spark-comp v1\nloop 9 3 1\n").loop_region.is_none());
}
