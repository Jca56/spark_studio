//! Clip view layout tests — nobody who can run these can see the graph.

use std::f32::consts::{FRAC_PI_2, PI};

use spark_render::{Shape, Viewport};

use super::page::*;
use super::words::*;
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

fn circle() -> Shape {
    let mut s = Shape::circle([300.0, 300.0], 40.0);
    s.set_opacity(0.8);
    s
}

/// What the view would list for `clip` on `shape`: its keyed settings
/// plus `armed`, in the inspector's order.
fn listed(clip: &ObjClip, shape: &Shape, fx: &Stack, armed: &[Target]) -> Vec<(Target, String)> {
    keyable_targets(shape, fx)
        .into_iter()
        .filter(|(t, _)| {
            clip.anim.track(*t).is_some_and(|tr| !tr.keys.is_empty()) || armed.contains(t)
        })
        .collect()
}

struct Fix<'a> {
    clip: &'a ObjClip,
    shape: &'a Shape,
    fx: &'a Stack,
    listed: &'a [(Target, String)],
}

fn input<'a>(f: &Fix<'a>, target: Option<Target>, sel: Option<Sel>, scroll: f32) -> Input<'a> {
    Input {
        clip: f.clip,
        name: "circle 1",
        color: [1.0, 0.5, 0.0],
        fx: f.fx,
        canvas: spark_render::CANVAS,
        shape: f.shape,
        listed: f.listed,
        bpm: 120.0,
        target,
        sel,
        scroll,
        playhead: Some(0.5),
        frozen: None,
    }
}

/// The sidebar lists the keyed settings and the armed ones — the
/// inspector's order, the inspector's words, keyed ones marked — the
/// graph maps the chosen one's keys, the strip carries every moment,
/// and every widget answers where it is drawn, at both output scales.
#[test]
fn the_sidebar_lists_keyed_and_armed_settings_and_the_graph_maps_the_keys() {
    let c = clip();
    let shape = circle();
    let mut fx = Stack::default();
    let gid = fx.add(EffectKind::Glow, 1);
    let armed = [
        Target::Shape(Prop::Opacity),
        Target::Effect { id: gid, param: 0 },
    ];
    let list = listed(&c, &shape, &fx, &armed);
    let f = Fix {
        clip: &c,
        shape: &shape,
        fx: &fx,
        listed: &list,
    };
    for scale in [1.0f32, 1.4] {
        let p = panel(scale);
        let view = TimeView::new(0.0, content_span(&c, 2.0));
        let x = Target::Shape(Prop::X);
        let page = Page::build(
            &p,
            &view,
            scale,
            &input(&f, Some(x), Some(Sel::Key { target: x, k: 1 }), 0.0),
        );
        let labels: Vec<&str> = page.rows.iter().map(|r| r.label.as_str()).collect();
        // Keyed X and Rot, armed Opacity and Glow — in the inspector's
        // order: the strip's words first, then Style's, then effects.
        assert_eq!(labels, ["X", "Rot", "Opacity", "Glow"], "scale {scale}");
        assert!(page.rows[0].keyed && page.rows[1].keyed);
        assert!(!page.rows[2].keyed && !page.rows[3].keyed);
        assert!(page.rows[0].selected && !page.rows[1].selected);
        assert_eq!(page.rows[0].value, "600", "the curve at the playhead");
        assert_eq!(page.rows[2].value, "0.80", "unkeyed: as the object stands");
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
        let rects = super::draw::rects(&page, None);
        assert!(rects.axis.len() > 10);
        assert_eq!(
            rects.rows.len(),
            page.rows.len() + 2,
            "a card per row, a diamond on the two keyed ones"
        );
    }
}

/// The loop brace's end is a grip on the ruler, and past it the axis is
/// washed dark — that content never plays. A non-looping clip washes
/// what its trim cut instead.
#[test]
fn the_loop_end_is_a_grip_and_past_it_never_plays() {
    let mut c = clip();
    let shape = circle();
    let fx = Stack::default();
    let list = listed(&c, &shape, &fx, &[]);
    let p = panel(1.0);
    let view = TimeView::new(0.0, content_span(&c, 2.0));
    let f = Fix {
        clip: &c,
        shape: &shape,
        fx: &fx,
        listed: &list,
    };
    let page = Page::build(&p, &view, 1.0, &input(&f, None, None, 0.0));
    let lx = view.x_of(2.0, p.axis);
    assert_eq!(page.loop_end_x, Some(lx));
    let ry = p.ruler.y + p.ruler.h * 0.5;
    assert_eq!(page.hit(lx + 4.0, ry), Some(Hit::LoopEnd));
    assert_eq!(page.hit(lx - 40.0, ry), None, "elsewhere the ruler scrubs");
    assert_eq!(page.wash.len(), 1);
    assert!(
        (page.wash[0].x - lx).abs() < 0.5,
        "the wash starts at the loop's end"
    );
    assert!((page.wash[0].x + page.wash[0].w - (p.axis.0 + p.axis.1)).abs() < 0.5);
    assert!(
        !super::draw::rects(&page, Some(Hit::LoopEnd))
            .ruler
            .is_empty()
    );
    // Not looping, trimmed half a second in: the cut is washed, the
    // brace has no grip.
    c.loop_on = false;
    c.offset = 0.5;
    let view = TimeView::new(0.0, content_span(&c, 2.0));
    let f = Fix {
        clip: &c,
        shape: &shape,
        fx: &fx,
        listed: &list,
    };
    let page = Page::build(&p, &view, 1.0, &input(&f, None, None, 0.0));
    assert_eq!(page.loop_end_x, None);
    assert_eq!(page.wash.len(), 2);
    assert!((page.wash[0].x + page.wash[0].w - view.x_of(0.5, p.axis)).abs() < 0.5);
    assert!((page.wash[1].x - view.x_of(2.5, p.axis)).abs() < 0.5);
}

/// An armed setting with no keys yet shows as one flat hold at the
/// object's value — something to double-click on — and no diamonds.
#[test]
fn an_armed_setting_is_a_flat_line_to_start_from() {
    let c = clip();
    let shape = circle();
    let fx = Stack::default();
    let op = Target::Shape(Prop::Opacity);
    let list = listed(&c, &shape, &fx, &[op]);
    let p = panel(1.0);
    let view = TimeView::new(0.0, content_span(&c, 2.0));
    let f = Fix {
        clip: &c,
        shape: &shape,
        fx: &fx,
        listed: &list,
    };
    let page = Page::build(&p, &view, 1.0, &input(&f, Some(op), None, 0.0));
    assert!(page.keys.is_empty());
    assert!(!page.curve.is_empty());
    let y = page.y_of(0.8);
    assert!(
        page.curve
            .iter()
            .all(|(a, b, inside)| { (a[1] - y).abs() < 0.5 && (b[1] - y).abs() < 0.5 && !inside })
    );
    assert_eq!(page.span, (0.0, 1.0), "opacity stands on its range");
    let labels = page.labels(None, &fx, spark_render::CANVAS);
    assert!(labels.iter().any(|l| l.text == "1.00") && labels.iter().any(|l| l.text == "0.00"));
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

/// How much local time the view shows — the whole clip, always — and
/// where the song has to be for the clip to play a local time.
#[test]
fn the_span_covers_the_whole_clip_and_local_time_maps_back_to_the_song() {
    let mut c = clip();
    // Two bars looping two bars, keys to 1 s, a 2 s bar of air.
    assert!((content_span(&c, 2.0) - 4.0).abs() < 1e-5);
    // A one-bar loop inside an eight-second clip still shows the clip.
    c.loop_len = 1.0;
    c.len = 8.0;
    assert!(
        (content_span(&c, 2.0) - 10.0).abs() < 1e-5,
        "the clip, not the loop"
    );
    c.loop_on = false;
    c.len = 2.0;
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

/// Numbers print the way the inspector prints them, settings wear the
/// inspector's words, and a moment reads in bars and beats.
#[test]
fn readouts_speak_the_inspectors_units() {
    let fx = Stack::default();
    let canvas = spark_render::CANVAS;
    let shape = circle();
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
    assert_eq!(target_label(Target::Shape(Prop::Scale), &shape, &fx), "S");
    assert_eq!(
        target_label(Target::Shape(Prop::Rotation), &shape, &fx),
        "Rot"
    );
    let mut stack = Stack::default();
    let id = stack.add(EffectKind::React, 1);
    let tg = Target::Effect { id, param: 0 };
    assert_eq!(fmt_target(tg, 1.0, &stack, canvas, false), "1.00");
    assert_eq!(fmt_target(tg, 0.25, &stack, canvas, false), "0.25");
    assert_eq!(target_label(tg, &shape, &stack), "React · Scale");
    assert_eq!(
        target_label(Target::Effect { id: 9, param: 0 }, &shape, &stack),
        "effect 9·0"
    );
    let gid = stack.add(EffectKind::Glow, 2);
    assert_eq!(
        target_label(Target::Effect { id: gid, param: 0 }, &shape, &stack),
        "Glow"
    );
    assert_eq!(beat_label(0.0, 120.0), "Bar 1.1");
    assert_eq!(beat_label(1.0, 120.0), "Bar 1.3");
    assert_eq!(beat_label(2.0, 120.0), "Bar 2.1");
}

/// What an object can key follows the inspector's own presence rules:
/// a light is aimed, not spun; a star field has a Size; a circle has a
/// W and H (it can be an ellipse) but no Sides and no D; effects last.
#[test]
fn the_keyable_settings_follow_what_the_object_has() {
    let fx = Stack::default();
    let words = |s: &Shape, fx: &Stack| -> Vec<String> {
        keyable_targets(s, fx).into_iter().map(|(_, l)| l).collect()
    };
    let c = words(&circle(), &fx);
    assert_eq!(
        &c[..9],
        &["X", "Y", "Z", "Tilt", "Turn", "Rot", "S", "W", "H"]
    );
    assert!(!c.iter().any(|l| l == "D") && !c.iter().any(|l| l == "Sides"));
    assert!(c.iter().any(|l| l == "Opacity") && c.iter().any(|l| l == "Brightness"));
    let light = Shape::light([0.0, 0.0], spark_render::LightKind::Sun);
    let l = words(&light, &fx);
    assert!(!l.iter().any(|w| w == "Rot"), "a light is aimed, not spun");
    assert!(l.iter().any(|w| w == "Intensity"));
    let stars = Shape::stars([0.0, 0.0], [200.0, 100.0], 3.0);
    let s = words(&stars, &fx);
    assert!(s.iter().any(|w| w == "W") && s.iter().any(|w| w == "Size"));
    assert!(s.iter().any(|w| w == "Density") && s.iter().any(|w| w == "Rate"));
    let mut stack = Stack::default();
    stack.add(EffectKind::Glow, 1);
    stack.add(EffectKind::React, 2);
    let e = words(&circle(), &stack);
    assert_eq!(
        e[e.len() - 4..],
        [
            "Glow",
            "React · Scale",
            "React · Glow",
            "React · Brightness"
        ]
    );
}

/// Rows scroll under the breadcrumb and their words stay inside the
/// window; the breadcrumb itself never moves.
#[test]
fn rows_scroll_and_their_words_stay_in_the_window() {
    let c = clip();
    let shape = circle();
    let mut fx = Stack::default();
    fx.add(EffectKind::Gradient, 1);
    fx.add(EffectKind::React, 2);
    // Everything armed: the whole keyable list.
    let all: Vec<Target> = keyable_targets(&shape, &fx)
        .into_iter()
        .map(|(t, _)| t)
        .collect();
    let list = listed(&c, &shape, &fx, &all);
    let f = Fix {
        clip: &c,
        shape: &shape,
        fx: &fx,
        listed: &list,
    };
    let p = panel(1.0);
    let view = TimeView::new(0.0, 4.0);
    let unscrolled = Page::build(&p, &view, 1.0, &input(&f, None, None, 0.0));
    assert!(unscrolled.rows.len() >= 13);
    assert!(
        unscrolled.max_scroll() > 0.0,
        "the rows overflow a 400 px panel"
    );
    let scroll = unscrolled.max_scroll();
    let page = Page::build(&p, &view, 1.0, &input(&f, None, None, scroll));
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

/// A clip with nothing keyed and nothing armed is an empty list — the
/// inspector fills it — and the axis is just the strip.
#[test]
fn an_unkeyed_clip_starts_empty() {
    let c = ObjClip::new(0.0, 2.0);
    let shape = circle();
    let fx = Stack::default();
    let list = listed(&c, &shape, &fx, &[]);
    assert!(list.is_empty());
    let p = panel(1.0);
    let view = TimeView::new(0.0, content_span(&c, 2.0));
    let f = Fix {
        clip: &c,
        shape: &shape,
        fx: &fx,
        listed: &list,
    };
    let page = Page::build(&p, &view, 1.0, &input(&f, None, None, 0.0));
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
