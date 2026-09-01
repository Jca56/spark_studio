//! Clip view layout tests — nobody who can run these can see the graph.

use std::f32::consts::{FRAC_PI_2, PI};

use spark_render::Viewport;

use super::page::*;
use super::*;
use crate::anim::{Ease, Key, Track};
use crate::fx::{EffectKind, Stack};
use crate::props::Prop;
use crate::timeline;

fn keys(pts: &[(f32, f32)]) -> Vec<Key> {
    pts.iter()
        .map(|&(t, v)| Key {
            t,
            v,
            ease: Ease::Smooth,
        })
        .collect()
}

fn panel(scale: f32) -> Panel {
    timeline::panel(
        Viewport {
            x: 0.0,
            y: 500.0,
            w: 3000.0,
            h: 400.0,
        },
        scale,
    )
}

/// A two-bar looping clip at 4 s with an X move and a half-turn.
fn clip() -> ObjClip {
    let mut c = ObjClip::new(4.0, 2.0);
    c.anim.tracks.push(Track {
        target: Target::Shape(Prop::X),
        keys: keys(&[(0.0, 300.0), (0.5, 600.0), (1.0, 900.0)]),
    });
    c.anim.tracks.push(Track {
        target: Target::Shape(Prop::Rotation),
        keys: keys(&[(0.0, 0.0), (1.0, PI)]),
    });
    c
}

fn input<'a>(
    clip: &'a ObjClip,
    fx: &'a Stack,
    target: Option<Target>,
    sel: Option<Sel>,
    scroll: f32,
) -> Input<'a> {
    Input {
        clip,
        name: "circle 1",
        color: [1.0, 0.5, 0.0],
        fx,
        canvas: spark_render::CANVAS,
        is_light: false,
        bpm: 120.0,
        target,
        sel,
        scroll,
        playhead: Some(0.5),
        frozen: None,
    }
}

/// The sidebar lists the clip's tracks, the graph maps the chosen one's
/// keys, the strip carries every moment, and every widget answers where
/// it is drawn — at both output scales.
#[test]
fn the_sidebar_lists_the_tracks_and_the_graph_maps_the_keys() {
    let c = clip();
    let fx = Stack::default();
    for scale in [1.0f32, 1.4] {
        let p = panel(scale);
        let view = TimeView::new(0.0, content_span(&c, 2.0));
        let x = Target::Shape(Prop::X);
        let page = Page::build(
            &p,
            &view,
            scale,
            &input(&c, &fx, Some(x), Some(Sel::Key { target: x, k: 1 }), 0.0),
        );
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.rows[0].label, "X");
        assert_eq!(page.rows[1].label, "Rotation");
        assert!(page.rows[0].selected && !page.rows[1].selected);
        assert_eq!(page.rows[0].value, "600", "the value at the playhead");
        // Keys sit at their times; a higher value sits higher.
        assert_eq!(page.keys.len(), 3);
        for d in &page.keys {
            assert!(
                (d.at[0] - view.x_of(d.t, p.axis)).abs() < 0.5,
                "scale {scale}"
            );
        }
        assert!(
            page.keys[2].at[1] < page.keys[0].at[1],
            "900 draws above 300"
        );
        assert!(page.keys[1].selected && !page.keys[0].selected);
        assert!(!page.curve.is_empty(), "the curve was sampled");
        // Three moments across the two tracks: 0, 0.5, 1.
        assert_eq!(page.strip_dots.len(), 3);
        // Hit and draw agree.
        let h = page.header;
        assert_eq!(page.hit(h.x + h.w * 0.5, h.y + h.h * 0.5), Some(Hit::Back));
        let r = page.rows[1].cell;
        assert_eq!(
            page.hit(r.x + r.w * 0.5, r.y + r.h * 0.5),
            Some(Hit::Row(1))
        );
        for (k, d) in page.keys.iter().enumerate() {
            assert_eq!(
                page.hit(d.at[0], d.at[1]),
                Some(Hit::Key(k)),
                "scale {scale}"
            );
        }
        let sy = page.strip.y + page.strip.h * 0.5;
        assert_eq!(page.hit(page.strip_dots[1].x, sy), Some(Hit::StripKey(1)));
        let far = page.strip.x + page.strip.w - 4.0;
        assert_eq!(page.hit(far, sy), Some(Hit::Strip));
        assert_eq!(
            page.hit(page.graph.x + page.graph.w * 0.9, page.graph.y + 4.0),
            Some(Hit::Graph)
        );
        assert_eq!(page.hit(p.gutter.x + 2.0, p.gutter.y + 2.0), None);
        // Value ↔ height is a round trip.
        for v in [300.0f32, 555.0, 900.0] {
            assert!(
                (page.value_at(page.y_of(v)) - v).abs() < 0.5,
                "scale {scale}: {v}"
            );
        }
        assert!(super::draw::rects(&page, None).axis.len() > 10);
    }
}

/// The graph stands on a bounded property's range, opens around a free
/// one's keys, and gives a flat curve a window to sit in.
#[test]
fn value_spans_fit_their_targets() {
    let fx = Stack::default();
    let canvas = spark_render::CANVAS;
    let x = Target::Shape(Prop::X);
    assert_eq!(
        value_span(x, &keys(&[(0.0, 300.0), (1.0, 900.0)]), &fx, canvas),
        (0.0, canvas[0]),
        "X stands on the canvas"
    );
    let (lo, _) = value_span(x, &keys(&[(0.0, -200.0)]), &fx, canvas);
    assert_eq!(lo, -200.0, "a key off the canvas widens it");
    let rot = Target::Shape(Prop::Rotation);
    let (lo, hi) = value_span(rot, &keys(&[(0.0, 0.0), (1.0, PI)]), &fx, canvas);
    assert!(lo < 0.0 && hi > PI, "air either side: {lo}..{hi}");
    let (lo, hi) = value_span(rot, &keys(&[(0.0, 1.0)]), &fx, canvas);
    assert!((lo - (1.0 - FRAC_PI_2)).abs() < 1e-5 && (hi - (1.0 + FRAC_PI_2)).abs() < 1e-5);
    let mut stack = Stack::default();
    let id = stack.add(EffectKind::React, 1);
    let tg = Target::Effect { id, param: 0 };
    assert_eq!(
        value_span(tg, &keys(&[(0.0, 1.0)]), &stack, canvas),
        (0.0, 20.0)
    );
}

/// How much local time the view shows, and where the song has to be for
/// the clip to play a local time.
#[test]
fn the_span_covers_the_content_and_local_time_maps_back_to_the_song() {
    let mut c = clip();
    // Looping two bars (loop_len 2), keys to 1 s, a 2 s bar of air.
    assert!((content_span(&c, 2.0) - 4.0).abs() < 1e-5);
    c.loop_on = false;
    c.offset = 0.5;
    assert!(
        (content_span(&c, 2.0) - 4.5).abs() < 1e-5,
        "offset + len + a bar"
    );
    c.anim.tracks[0].keys.push(Key {
        t: 6.0,
        v: 0.0,
        ease: Ease::Smooth,
    });
    assert!(
        (content_span(&c, 2.0) - 8.0).abs() < 1e-5,
        "the last key wins"
    );
    // Local → song: the first pass inside the clip.
    let mut l = ObjClip::new(4.0, 2.0);
    assert!((song_time_for(&l, 0.5) - 4.5).abs() < 1e-5);
    assert!(song_time_for(&l, 3.0) < 6.0, "past the end clamps inside");
    l.offset = 1.0;
    assert!(
        (song_time_for(&l, 0.0) - 5.0).abs() < 1e-5,
        "before the trim: the next pass"
    );
    assert!((song_time_for(&l, 1.5) - 4.5).abs() < 1e-5);
    l.loop_on = false;
    assert!(
        (song_time_for(&l, 0.0) - 4.0).abs() < 1e-5,
        "trimmed away: the clip's start"
    );
}

/// Numbers print the way the inspector prints them, and a moment reads
/// in bars and beats.
#[test]
fn readouts_speak_the_inspectors_units() {
    let fx = Stack::default();
    let canvas = spark_render::CANVAS;
    assert_eq!(
        fmt_target(Target::Shape(Prop::Rotation), PI, &fx, canvas, false),
        "180°"
    );
    assert_eq!(
        fmt_target(Target::Shape(Prop::Scale), 50.0, &fx, canvas, false),
        "100"
    );
    assert_eq!(
        fmt_target(Target::Shape(Prop::Scale), 50.0, &fx, canvas, true),
        "50"
    );
    assert_eq!(
        fmt_target(Target::Shape(Prop::Opacity), 0.5, &fx, canvas, false),
        "0.50"
    );
    assert_eq!(
        fmt_target(Target::Shape(Prop::X), 640.0, &fx, canvas, false),
        "640"
    );
    let mut stack = Stack::default();
    let id = stack.add(EffectKind::React, 1);
    let tg = Target::Effect { id, param: 0 };
    assert_eq!(fmt_target(tg, 1.0, &stack, canvas, false), "1.0");
    assert_eq!(fmt_target(tg, 0.7, &stack, canvas, false), "0.7");
    assert_eq!(target_label(tg, &stack), "React · Scale");
    assert_eq!(
        target_label(Target::Effect { id: 9, param: 0 }, &stack),
        "effect 9·0"
    );
    assert_eq!(beat_label(0.0, 120.0), "Bar 1.1");
    assert_eq!(beat_label(1.0, 120.0), "Bar 1.3");
    assert_eq!(beat_label(2.0, 120.0), "Bar 2.1");
}

/// Rows scroll under the breadcrumb and their words stay inside the
/// window; the breadcrumb itself never moves.
#[test]
fn rows_scroll_and_their_words_stay_in_the_window() {
    let mut c = clip();
    for p in [
        Prop::Y,
        Prop::Scale,
        Prop::Opacity,
        Prop::Brightness,
        Prop::Tilt,
        Prop::Turn,
        Prop::Z,
        Prop::Sides,
    ] {
        c.anim.tracks.push(Track {
            target: Target::Shape(p),
            keys: keys(&[(0.0, 4.0)]),
        });
    }
    let fx = Stack::default();
    let p = panel(1.0);
    let view = TimeView::new(0.0, 4.0);
    let unscrolled = Page::build(&p, &view, 1.0, &input(&c, &fx, None, None, 0.0));
    assert!(
        unscrolled.max_scroll() > 0.0,
        "ten rows overflow a 400 px panel"
    );
    let scroll = unscrolled.max_scroll();
    let page = Page::build(&p, &view, 1.0, &input(&c, &fx, None, None, scroll));
    assert_eq!(page.header, unscrolled.header, "the breadcrumb is pinned");
    let labels = page.labels(None, &fx, spark_render::CANVAS);
    assert!(labels.iter().any(|l| l.text == "circle 1"));
    let win = page.rows_clip;
    for l in labels.iter().filter(|l| l.text != "circle 1") {
        assert!(
            l.pos[1] >= win.y - 1.0 && l.pos[1] <= win.y + win.h,
            "{} printed outside the rows' window",
            l.text
        );
    }
    assert!(
        !labels.iter().any(|l| l.text == "X"),
        "the first row scrolled out and its word went with it"
    );
    // A row scrolled under the header isn't clickable through it.
    let r0 = page.rows[0].cell;
    assert_ne!(page.hit(r0.x + 10.0, r0.y + 10.0), Some(Hit::Row(0)));
}

/// With no target (a clip with no keys yet) the sidebar is just the
/// breadcrumb and the axis is just the strip — nothing to trip over.
#[test]
fn an_unkeyed_clip_shows_an_empty_view() {
    let c = ObjClip::new(0.0, 2.0);
    let fx = Stack::default();
    let p = panel(1.0);
    let view = TimeView::new(0.0, content_span(&c, 2.0));
    let page = Page::build(&p, &view, 1.0, &input(&c, &fx, None, None, 0.0));
    assert!(page.rows.is_empty() && page.keys.is_empty() && page.strip_dots.is_empty());
    let r = super::draw::rects(&page, None);
    assert_eq!(r.sidebar.len(), 2, "plate and chevron");
    assert!(r.rows.is_empty());
    assert_eq!(
        page.labels(None, &fx, spark_render::CANVAS).len(),
        1,
        "the name alone"
    );
}
