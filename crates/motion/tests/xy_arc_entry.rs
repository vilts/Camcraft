//! M11.3 (owed): the pocket's horizontal arc entry - the entry descends
//! at the arc's start and one G17 arc runs tangentially onto the first
//! pass, inside the legal area (verify clean, every arc point in the
//! cutter-centre area); a pocket too tight for any arc falls back to the
//! plain entry, named; the vertical Arc entry and every default pocket
//! are untouched.
//!
//! Gate map (M11.3): covers camcraft_motion::machining2d (descent_xy,
//! descent, ladder_links, levels, pocket), camcraft_motion::approach
//! (arc_xy).

use camcraft_motion::approach::Approach;
use camcraft_motion::cl::{ArcPlane, Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::{Link, Links};
use camcraft_motion::machining2d::{pocket, PocketParams, PocketStrategy};
use camcraft_motion::verify::{verify, Mode, ToolGeom, VerifyContext, Warning};

const CLEAR: f64 = 25.0;
const TOP: f64 = 0.0;
const R: f64 = 3.0;

fn rect(w: f64, h: f64) -> Region {
    let pts = vec![Point2::new(0.0, 0.0), Point2::new(w, 0.0), Point2::new(w, h), Point2::new(0.0, h)];
    Region::new(vec![Loop::new(pts).unwrap()]).unwrap()
}

fn round() -> Region {
    let pts = (0..256)
        .map(|k| {
            let t = std::f64::consts::TAU * f64::from(k) / 256.0;
            Point2::new(15.0 * t.cos(), 15.0 * t.sin())
        })
        .collect();
    Region::new(vec![Loop::new(pts).unwrap()]).unwrap()
}

fn pp(entry: Entry3D, strategy: PocketStrategy, links: Links) -> PocketParams {
    PocketParams {
        tool_diameter: 2.0 * R,
        stepover: 2.5,
        stock_to_leave: 0.0,
        clearance_z: CLEAR,
        top_z: TOP,
        bottom_z: -6.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy,
        retract_z: Some(15.0),
        entry,
        helix_radius: 1.5,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links,
    }
}

fn arc_on(a: Approach) -> Approach {
    Approach { arc_xy: true, ..a }
}

fn check(t: &Toolpath, r: &Region) -> Vec<Warning> {
    let tool = ToolGeom {
        radius: R,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: R,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    let cx = VerifyContext {
        region: r,
        stock: None,
        model: None,
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: TOP,
        clearance_z: CLEAR,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(t, &cx)
}

fn planar(m: &Move) -> bool {
    m.kind == MoveKind::Feed && m.from.z == m.to.z
}

/// The unit direction a move leaves its start with (XY).
fn dir_out(m: &Move) -> (f64, f64) {
    let (dx, dy) = match m.arc {
        Some(a) => {
            let (rx, ry) = (m.from.x - a.center.x, m.from.y - a.center.y);
            if a.ccw { (-ry, rx) } else { (ry, -rx) }
        }
        None => (m.to.x - m.from.x, m.to.y - m.from.y),
    };
    let l = dx.hypot(dy);
    (dx / l, dy / l)
}

/// The unit direction an arc move arrives with (XY).
fn dir_in(m: &Move) -> (f64, f64) {
    let a = m.arc.expect("an arc");
    let (rx, ry) = (m.to.x - a.center.x, m.to.y - a.center.y);
    let (dx, dy) = if a.ccw { (-ry, rx) } else { (ry, -rx) };
    let l = dx.hypot(dy);
    (dx / l, dy / l)
}

/// Every level's first planar cut, with the move after it.
fn level_firsts(t: &Toolpath) -> Vec<(Move, Move)> {
    let mut out = Vec::new();
    let mut cur: Option<f64> = None;
    for (i, m) in t.moves.iter().enumerate() {
        if planar(m) && cur != Some(m.to.z) {
            cur = Some(m.to.z);
            out.push((*m, t.moves[i + 1]));
        }
    }
    out
}

#[test]
fn the_arc_entry_starts_each_level_with_a_g17_arc_tangent_to_its_first_pass() {
    let keep = Links { level: Link::KeepDown, pass: Link::KeepDown, ..Links::PLANAR };
    for (name, r) in [("round", round()), ("rect", rect(40.0, 30.0))] {
        let legal = r.offset(-(R - 1e-3)).expect("legal area");
        for strategy in [PocketStrategy::Waterline, PocketStrategy::OffsetRings] {
            for entry in [Entry3D::Plunge, Entry3D::Ramp, Entry3D::Helix] {
                for base in [Links::PLANAR, keep] {
                    let links = Links { approach: arc_on(base.approach), ..base };
                    let t = pocket(&r, &pp(entry, strategy, links)).expect("pocket");
                    let tag = format!("{name} {strategy:?} {entry:?} {:?}", base.level);
                    assert_eq!(check(&t, &r), Vec::new(), "{tag}: {:?}", t.notes);
                    assert!(!t.notes.iter().any(|n| n.contains("does not fit")), "{tag}: {:?}", t.notes);
                    for (arc, next) in level_firsts(&t) {
                        let a = arc.arc.unwrap_or_else(|| panic!("{tag}: the level opens with {arc:?}"));
                        assert_eq!(a.plane, ArcPlane::Xy, "{tag}: a G17 arc");
                        let (u, v) = (dir_in(&arc), dir_out(&next));
                        assert!((u.0 * v.1 - u.1 * v.0).abs() < 1e-3 && u.0 * v.0 + u.1 * v.1 > 0.0, "{tag}: tangent {u:?} {v:?}");
                        // Every point of the arc in the cutter-centre area.
                        let rad = (arc.from.x - a.center.x).hypot(arc.from.y - a.center.y);
                        let t0 = (arc.from.y - a.center.y).atan2(arc.from.x - a.center.x);
                        let t1 = (arc.to.y - a.center.y).atan2(arc.to.x - a.center.x);
                        let mut sw = t1 - t0;
                        if a.ccw && sw < 0.0 {
                            sw += std::f64::consts::TAU;
                        }
                        if !a.ccw && sw > 0.0 {
                            sw -= std::f64::consts::TAU;
                        }
                        for k in 0..=64 {
                            let th = t0 + sw * f64::from(k) / 64.0;
                            let p = Point2::new(a.center.x + rad * th.cos(), a.center.y + rad * th.sin());
                            assert!(legal.contains(p), "{tag}: arc point {p:?} leaves the legal area");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn the_arc_takes_its_radius_and_sweep() {
    // A 60 x 60 pocket on 3 mm stepover keeps a 1 mm, 45 degree arc
    // clear of the next ring: flown exactly as asked.
    let r = rect(60.0, 60.0);
    let a = Approach { arc_radius: 1.0, arc_sweep: 45.0, ..arc_on(Approach::DEFAULT) };
    let mut p = pp(Entry3D::Helix, PocketStrategy::Waterline, Links { approach: a, ..Links::PLANAR });
    p.stepover = 3.0;
    let t = pocket(&r, &p).expect("pocket");
    assert_eq!(check(&t, &r), Vec::new(), "{:?}", t.notes);
    let (arc, _) = level_firsts(&t)[0];
    let c = arc.arc.expect("an arc").center;
    let ra = (arc.from.x - c.x).hypot(arc.from.y - c.y);
    let rb = (arc.to.x - c.x).hypot(arc.to.y - c.y);
    assert!((ra - 1.0).abs() < 1e-5 && (rb - 1.0).abs() < 1e-5, "radius {ra} {rb}");
    let chord = (arc.to.x - arc.from.x).hypot(arc.to.y - arc.from.y);
    let sweep = 2.0 * (chord / 2.0).asin().to_degrees();
    assert!((sweep - 45.0).abs() < 1e-3, "sweep {sweep}");
    assert!(!t.notes.iter().any(|n| n.contains("horizontal arc")), "{:?}", t.notes);
}

#[test]
fn a_tight_pocket_falls_back_to_the_plain_entry_named() {
    // A 6.2 mm slot for a 6 mm tool: a 0.2 mm centre strip, no arc fits;
    // and a zigzag row starting on the wall has no room behind its start.
    for (r, strategy) in [(rect(40.0, 6.2), PocketStrategy::Waterline), (rect(40.0, 30.0), PocketStrategy::Zigzag)] {
    for entry in [Entry3D::Plunge, Entry3D::Ramp, Entry3D::Helix] {
        let plain = pocket(&r, &pp(entry, strategy, Links::PLANAR)).expect("plain");
        let links = Links { approach: arc_on(Approach::DEFAULT), ..Links::PLANAR };
        let t = pocket(&r, &pp(entry, strategy, links)).expect("arc asked");
        assert!(
            t.notes.iter().any(|n| n.starts_with("entry: the horizontal arc entry does not fit")),
            "{entry:?}: {:?}",
            t.notes
        );
        if strategy == PocketStrategy::Zigzag {
            assert_eq!(t.moves, plain.moves, "{entry:?}: the plain entry flies instead");
        } else {
            // The closed ring was reseated mid-edge for the arc; with no
            // arc it is the same ring, entered plainly.
            assert!(level_firsts(&t).iter().all(|(m, _)| m.arc.is_none()), "{entry:?}: no arc");
        }
        assert_eq!(check(&t, &r), Vec::new(), "{entry:?}: {:?}", t.notes);
    }
    }
}

#[test]
fn a_shrunk_arc_is_named_and_the_vertical_arc_is_untouched() {
    // Tool-diameter arcs cross the next row on a 2.5 mm stepover: the
    // radius halves until it clears, said once.
    let r = rect(40.0, 30.0);
    let links = Links { approach: arc_on(Approach::DEFAULT), ..Links::PLANAR };
    let t = pocket(&r, &pp(Entry3D::Helix, PocketStrategy::Waterline, links)).expect("pocket");
    assert!(t.notes.iter().any(|n| n.starts_with("entry: the horizontal arc fits only under its radius")), "{:?}", t.notes);
    // The vertical Arc entry is its own lead: the flag changes nothing.
    let v = pocket(&r, &pp(Entry3D::Arc, PocketStrategy::Zigzag, Links::PLANAR)).expect("vertical");
    let w = pocket(&r, &pp(Entry3D::Arc, PocketStrategy::Zigzag, links)).expect("vertical, flag on");
    assert_eq!(v.moves, w.moves);
    assert_eq!(v.notes, w.notes);
}

#[test]
fn a_tool_that_is_not_centre_cutting_still_never_plunges_at_the_arc_start() {
    let r = round();
    let a = Approach { center_cutting: false, ..arc_on(Approach::DEFAULT) };
    let t = pocket(&r, &pp(Entry3D::Plunge, PocketStrategy::Waterline, Links { approach: a, ..Links::PLANAR }))
        .expect("pocket");
    assert_eq!(check(&t, &r), Vec::new(), "{:?}", t.notes);
    let (arc, _) = level_firsts(&t)[0];
    assert!(arc.arc.is_some_and(|a| a.plane == ArcPlane::Xy), "{arc:?}");
    // The first descent is a helix (the plunge stands down), not a plunge.
    let first_down = t.moves.iter().find(|m| m.kind != MoveKind::Rapid && m.to.z < m.from.z).expect("a descent");
    assert!(first_down.arc.is_some(), "{first_down:?}\n{:?}", t.notes);
}
