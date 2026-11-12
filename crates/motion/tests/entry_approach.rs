//! M11.3 B/D and the one-entry law (user hand-tests 2026-11-06): every
//! entry parameter changes the moves as specified and stays legal
//! (verify clean), and each level is entered ONCE - the descent ends on
//! the first cut's start (or one at-depth link joins them), no plunge
//! feeds down from the retract plane after it, and every descent starts
//! just over the material (top + plunge height), never tens of mm up.
//!
//! Gate map (M11.3): covers camcraft_motion::approach,
//! camcraft_motion::machining2d (descent, levels, ladder_links,
//! contour_bore), camcraft_motion::machining3d (descent, level_ctx),
//! camcraft_motion::entry (helix_about, disc_clears),
//! camcraft_motion::entry_arc (arc_lead_swept), camcraft_motion::links.

use camcraft_motion::approach::{Approach, CENTER_CUT_REFUSAL, DEFAULT_PLUNGE_HEIGHT};
use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::{Link, Links};
use camcraft_motion::machining2d::{pocket, PocketParams, PocketStrategy};
use camcraft_motion::verify::{verify, Mode, ToolGeom, VerifyContext, Warning};

const CLEAR: f64 = 25.0;
const HOP: f64 = 15.0;
const TOP: f64 = 0.0;

fn rect() -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(40.0, 0.0),
        Point2::new(40.0, 30.0),
        Point2::new(0.0, 30.0),
    ])
    .unwrap()])
    .unwrap()
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
        tool_diameter: 6.0,
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
        retract_z: Some(HOP),
        entry,
        helix_radius: 1.5,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links,
    }
}

fn with(a: Approach) -> Links {
    Links { approach: a, ..Links::PLANAR }
}

fn check(t: &Toolpath, r: &Region) -> Vec<Warning> {
    let tool = ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 3.0,
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

/// The cutting levels in path order.
fn levels(t: &Toolpath) -> Vec<f64> {
    let mut v: Vec<f64> = Vec::new();
    for m in t.moves.iter().filter(|m| planar(m)) {
        if v.last() != Some(&m.to.z) {
            v.push(m.to.z);
        }
    }
    v
}

/// For each level: the moves from the last planar cut above (or the
/// start) up to the level's first planar cut.
fn entries(t: &Toolpath) -> Vec<(f64, Vec<Move>)> {
    let mut out = Vec::new();
    let mut cur: Option<f64> = None;
    let mut buf: Vec<Move> = Vec::new();
    for m in &t.moves {
        if planar(m) {
            if cur != Some(m.to.z) {
                out.push((m.to.z, std::mem::take(&mut buf)));
                cur = Some(m.to.z);
            }
            buf.clear();
        } else {
            buf.push(*m);
        }
    }
    out
}

/// The one-entry law on a level's approach moves: one descent (the moves
/// that go down below the material top above the level), no lift after
/// the descent began, every plunge from no higher than `top + h`.
fn one_entry(t: &Toolpath, h: f64) {
    let lv = levels(t);
    for (i, (z, moves)) in entries(t).iter().enumerate() {
        let above = if i == 0 { TOP } else { lv[i - 1] };
        let first_down = moves.iter().position(|m| m.kind != MoveKind::Rapid && m.to.z < m.from.z);
        let Some(k) = first_down else {
            panic!("level {z}: no descent at all: {moves:?}");
        };
        for m in &moves[k..] {
            assert!(m.to.z <= m.from.z + 1e-9, "level {z}: a lift after the entry began: {m:?}\n{:?}", t.notes);
        }
        for m in moves.iter().filter(|m| m.kind == MoveKind::Plunge) {
            assert!(m.from.z <= above + h + 1e-6, "level {z}: a plunge feeds from {} (top {above} + {h})", m.from.z);
        }
        for m in moves.iter().filter(|m| m.kind == MoveKind::Feed && m.to.z < m.from.z) {
            assert!(m.from.z <= above.max(TOP) + h + 1e-6, "level {z}: a descent starts at {}: {m:?}", m.from.z);
        }
    }
}

fn plunges(t: &Toolpath) -> usize {
    t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count()
}

#[test]
fn every_entry_enters_each_level_once_on_round_and_rectangular_pockets() {
    for (name, r) in [("round", round()), ("rect", rect())] {
        for strategy in [PocketStrategy::Waterline, PocketStrategy::Zigzag] {
            for entry in [Entry3D::Helix, Entry3D::Ramp, Entry3D::Arc, Entry3D::Plunge] {
                let t = pocket(&r, &pp(entry, strategy, Links::PLANAR)).expect("pocket");
                assert_eq!(check(&t, &r), Vec::new(), "{name} {strategy:?} {entry:?}: {:?}", t.notes);
                one_entry(&t, DEFAULT_PLUNGE_HEIGHT);
                assert!(
                    !t.notes.iter().any(|n| n.contains("second dive")),
                    "{name} {strategy:?} {entry:?}: {:?}",
                    t.notes
                );
                if entry == Entry3D::Helix {
                    // The helix closes on the pass start: no dive at all
                    // except pass links between rings at a level.
                    let first_arc = t.moves.iter().find(|m| m.arc.is_some() && m.to.z < m.from.z).expect("a helix");
                    assert!(first_arc.from.z <= TOP + DEFAULT_PLUNGE_HEIGHT + 1e-9, "{name}: {first_arc:?}");
                }
            }
        }
    }
}

#[test]
fn the_reported_keep_down_helix_has_no_red_plunge() {
    // The user's settings: Helix entry, Helix level descent pitch 1,
    // keep down between levels, retract plane between passes.
    let links = Links { transition: Some(Entry3D::Helix), helix_pitch: 1.0, level: Link::KeepDown, ..Links::PLANAR };
    let r = round();
    let t = pocket(&r, &pp(Entry3D::Helix, PocketStrategy::Waterline, links)).expect("pocket");
    assert_eq!(check(&t, &r), Vec::new(), "{:?}", t.notes);
    one_entry(&t, DEFAULT_PLUNGE_HEIGHT);
    for (z, moves) in entries(&t) {
        let helix = moves.iter().any(|m| m.arc.is_some() && m.to.z < m.from.z);
        assert!(helix, "level {z} enters by helix");
        assert!(!moves.iter().any(|m| m.kind == MoveKind::Plunge), "level {z}: no plunge after the helix: {moves:?}");
    }
    // Any lift is said in plain words.
    for n in t.notes.iter().filter(|n| n.starts_with("link:")) {
        assert!(n.contains("the tool lifted") && n.contains("instead of"), "{n}");
    }
}

#[test]
fn a_zero_plunge_height_feeds_from_the_traverse_plane() {
    let r = rect();
    let a = Approach { plunge_height: 0.0, ..Approach::DEFAULT };
    let t = pocket(&r, &pp(Entry3D::Plunge, PocketStrategy::Zigzag, with(a))).expect("pocket");
    assert_eq!(check(&t, &r), Vec::new());
    let first = t.moves.iter().find(|m| m.kind == MoveKind::Plunge).expect("plunge");
    assert!(first.from.z >= HOP - 1e-9, "from the traverse plane: {first:?}");
    let a = Approach { plunge_height: 5.0, ..Approach::DEFAULT };
    let t = pocket(&r, &pp(Entry3D::Plunge, PocketStrategy::Zigzag, with(a))).expect("pocket");
    one_entry(&t, 5.0);
}

#[test]
fn the_entry_ramp_angle_and_leg_bound_every_descending_move() {
    let r = rect();
    for (angle, leg) in [(5.0_f64, 0.0_f64), (3.0, 4.0)] {
        let a = Approach { ramp_angle: angle, ramp_max_len: leg, ..Approach::DEFAULT };
        let t = pocket(&r, &pp(Entry3D::Ramp, PocketStrategy::Zigzag, with(a))).expect("pocket");
        assert_eq!(check(&t, &r), Vec::new(), "{:?}", t.notes);
        one_entry(&t, DEFAULT_PLUNGE_HEIGHT);
        let tan = angle.to_radians().tan();
        let ramps: Vec<&Move> = t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.to.z < m.from.z).collect();
        assert!(!ramps.is_empty());
        for m in ramps {
            let run = (m.to.x - m.from.x).hypot(m.to.y - m.from.y);
            assert!(m.from.z - m.to.z <= run * tan + 1e-6, "steeper than {angle}: {m:?}");
            if leg > 0.0 {
                assert!(run <= leg + 1e-6, "a leg past {leg}: {m:?}");
            }
        }
    }
}

#[test]
fn ramp_to_depth_starts_every_level_from_the_top() {
    let r = rect();
    let a = Approach { ramp_angle: 10.0, ramp_to_depth: true, ..Approach::DEFAULT };
    let t = pocket(&r, &pp(Entry3D::Ramp, PocketStrategy::Zigzag, with(a))).expect("pocket");
    assert_eq!(check(&t, &r), Vec::new(), "{:?}", t.notes);
    for (z, moves) in entries(&t) {
        let top = moves.iter().filter(|m| m.kind == MoveKind::Feed && m.to.z < m.from.z).map(|m| m.from.z).fold(f64::NEG_INFINITY, f64::max);
        assert!((top - (TOP + DEFAULT_PLUNGE_HEIGHT)).abs() < 1e-6, "level {z} ramps from {top}");
    }
}

#[test]
fn the_helix_shrinks_to_its_minimum_turns_its_way_and_takes_its_pitch() {
    // A 9 mm slot-ish pocket: a 1.5 mm helix does not fit a 6 mm tool's
    // 3 mm wide center band... a 0.5 minimum does.
    let r = Region::new(vec![Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(40.0, 0.0),
        Point2::new(40.0, 8.0),
        Point2::new(0.0, 8.0),
    ])
    .unwrap()])
    .unwrap();
    let base = pocket(&r, &pp(Entry3D::Helix, PocketStrategy::Zigzag, Links::PLANAR)).expect("pocket");
    assert!(base.notes.iter().any(|n| n.contains("widened disc")), "{:?}", base.notes);
    let a = Approach { helix_min_radius: 0.5, helix_cw: true, helix_pitch: 0.5, ..Approach::DEFAULT };
    let t = pocket(&r, &pp(Entry3D::Helix, PocketStrategy::Zigzag, with(a))).expect("pocket");
    assert_eq!(check(&t, &r), Vec::new(), "{:?}", t.notes);
    assert!(t.notes.iter().any(|n| n.contains("flown at")), "{:?}", t.notes);
    let arcs: Vec<&Move> = t.moves.iter().filter(|m| m.arc.is_some() && m.to.z < m.from.z).collect();
    assert!(!arcs.is_empty());
    assert!(arcs.iter().all(|m| m.arc.is_some_and(|a| !a.ccw)), "clockwise");
    // 0.5 mm per turn = 0.25 per half turn.
    for m in &arcs {
        assert!(m.from.z - m.to.z <= 0.25 + 1e-6, "pitch: {m:?}");
    }
}

#[test]
fn the_arc_entry_takes_its_radius_and_sweep() {
    let r = round();
    let a = Approach { arc_radius: 3.0, arc_sweep: 30.0, ..Approach::DEFAULT };
    let t = pocket(&r, &pp(Entry3D::Arc, PocketStrategy::Waterline, with(a))).expect("pocket");
    assert_eq!(check(&t, &r), Vec::new(), "{:?}", t.notes);
    one_entry(&t, DEFAULT_PLUNGE_HEIGHT);
    assert!(!t.notes.iter().any(|n| n.contains("axial dive")), "{:?}", t.notes);
    // Each level: an axial feed to the arc's top, R(1 - cos 30) over the
    // cut, then the arc.
    let drop = 3.0 * (1.0 - 30f64.to_radians().cos());
    let tops: Vec<f64> = t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).map(|m| m.to.z).collect();
    for (z, _) in entries(&t) {
        assert!(tops.iter().any(|&q| (q - (z + drop)).abs() < 2e-3), "level {z}: arc top {drop} over: {tops:?}");
    }
}

#[test]
fn entry_feeds_and_the_engagement_limit_are_honoured_and_named() {
    let r = rect();
    let a = Approach { ramp_feed: 250.0, plunge_feed: 60.0, lead_feed_pct: 50.0, ..Approach::DEFAULT };
    let t = pocket(&r, &pp(Entry3D::Ramp, PocketStrategy::Zigzag, with(a))).expect("pocket");
    let ramp = t.moves.iter().find(|m| m.kind == MoveKind::Feed && m.to.z < m.from.z).expect("ramp");
    assert_eq!(ramp.feed, Some(250.0));
    let a = Approach { max_engagement: 0.4, ..Approach::DEFAULT };
    let t = pocket(&r, &pp(Entry3D::Ramp, PocketStrategy::Zigzag, with(a))).expect("pocket");
    let ramp = t.moves.iter().find(|m| m.kind == MoveKind::Feed && m.to.z < m.from.z).expect("ramp");
    assert!((ramp.feed.unwrap_or(0.0) - 200.0).abs() < 1e-9, "500 x 40%: {ramp:?}");
    assert!(t.notes.iter().any(|n| n.contains("over the 40% limit")), "{:?}", t.notes);
    let a = Approach { plunge_feed: 60.0, ..Approach::DEFAULT };
    let t = pocket(&r, &pp(Entry3D::Plunge, PocketStrategy::Zigzag, with(a))).expect("pocket");
    assert_eq!(t.moves.iter().find(|m| m.kind == MoveKind::Plunge).and_then(|m| m.feed), Some(60.0));
    // Nonsense refused at the door.
    for bad in [
        Approach { ramp_angle: 95.0, ..Approach::DEFAULT },
        Approach { lead_feed_pct: 0.0, ..Approach::DEFAULT },
        Approach { max_engagement: 2.0, ..Approach::DEFAULT },
        Approach { plunge_height: f64::NAN, ..Approach::DEFAULT },
        Approach { arc_sweep: 120.0, ..Approach::DEFAULT },
    ] {
        assert!(pocket(&r, &pp(Entry3D::Ramp, PocketStrategy::Zigzag, with(bad))).is_err(), "{bad:?}");
    }
}

#[test]
fn a_tool_that_is_not_centre_cutting_never_plunges_into_solid() {
    let r = rect();
    let a = Approach { center_cutting: false, ..Approach::DEFAULT };
    let t = pocket(&r, &pp(Entry3D::Plunge, PocketStrategy::Zigzag, with(a))).expect("helix stands in");
    assert_eq!(check(&t, &r), Vec::new(), "{:?}", t.notes);
    assert!(t.notes.iter().any(|n| n.contains("not centre-cutting") && n.contains("helix")), "{:?}", t.notes);
    for (z, moves) in entries(&t) {
        assert!(!moves.iter().any(|m| m.kind == MoveKind::Plunge && m.to.z < TOP - 1e-9 && m.to.z <= z), "level {z}: {moves:?}");
    }
    // A strip no helix or ramp fits: refused by name.
    let thin = Region::new(vec![Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(7.0, 0.0),
        Point2::new(7.0, 6.5),
        Point2::new(0.0, 6.5),
    ])
    .unwrap()])
    .unwrap();
    let p = PocketParams { helix_radius: 3.0, ..pp(Entry3D::Plunge, PocketStrategy::Zigzag, with(Approach { ramp_angle: 0.1, ..a })) };
    let err = pocket(&thin, &p).expect_err("refused");
    assert!(format!("{err:?}").contains(CENTER_CUT_REFUSAL), "{err:?}");
    let _ = plunges;
}
