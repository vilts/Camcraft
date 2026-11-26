//! The arc entry (M11.1): the circular ramp's geometry laws - tangent to
//! the first cut at the bottom, contained in its vertical plane, chord
//! fan inside the tolerance - its legality refusals and named fallbacks,
//! and the degenerate inputs that must error, never panic or hang.
//!
//! Gate map (M11.1): covers camcraft_motion::entry_arc,
//! camcraft_motion::machining2d (contour / pocket / slot arc arms),
//! camcraft_motion::machining3d (ladder arc arm).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::entry_arc::{arc_lead, ArcLead, ARC_ENTRY_MAX_CHORDS};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::Point3;
use camcraft_motion::machining2d::{
    contour, pocket, slot, ContourParams, Lead, PocketParams, PocketStrategy, Side, SlotParams,
};

/// A round bore about the origin: the inscribed 256-gon of radius `r`.
fn disk(r: f64) -> Region {
    let pts = (0..256)
        .map(|k| {
            let t = std::f64::consts::TAU * f64::from(k) / 256.0;
            Point2::new(r * t.cos(), r * t.sin())
        })
        .collect();
    Region::new(vec![Loop::new(pts).unwrap()]).unwrap()
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

/// The lattice slack every snapped fan vertex may carry (1 um grid).
const SNAP: f64 = 1e-6;

fn free(_: Point2, _: Point2) -> bool {
    true
}

/// Assert every law of one certified lead: start on the entry plane
/// behind `e` along `-u`, every vertex in the vertical plane through
/// `e` along `u`, on the circle about `(e, z_cut + R)`, Z falling and
/// `s` rising monotonically, the last vertex exactly `(e, z_cut)`, and
/// every chord's sagitta inside `tol`.
fn assert_lead_laws(l: &ArcLead, e: Point2, u: Point2, z_top: f64, z_cut: f64, tol: f64) {
    let r = l.radius;
    assert!(r >= z_top - z_cut - 1e-12, "the arc must cover the drop: R {r}");
    let s_of = |x: f64, y: f64| (x - e.x) * u.x + (y - e.y) * u.y;
    let off = |x: f64, y: f64| ((x - e.x) * u.y - (y - e.y) * u.x).abs();
    let mut prev = (s_of(l.start.x, l.start.y), z_top);
    assert!(prev.0 < 0.0, "the lead starts behind the entry: {l:?}");
    assert!(off(l.start.x, l.start.y) <= 2.0 * SNAP, "start off the plane");
    // The start sits on the circle at the top of the sweep.
    let on = |s: f64, z: f64| (s.hypot(z - (z_cut + r)) - r).abs();
    assert!(on(prev.0, prev.1) <= 4.0 * SNAP, "start off the circle: {}", on(prev.0, prev.1));
    for q in &l.fan {
        let s = s_of(q.x, q.y);
        assert!(off(q.x, q.y) <= 2.0 * SNAP, "vertex off the vertical plane: {q:?}");
        assert!(on(s, q.z) <= 4.0 * SNAP, "vertex off the circle: {q:?}");
        assert!(s >= prev.0 - SNAP && q.z <= prev.1 + SNAP, "the fan runs forward and down");
        // Sagitta of this chord: the circle's distance from the chord
        // midpoint, measured toward the center.
        let (ms, mz) = ((s + prev.0) * 0.5, (q.z + prev.1) * 0.5);
        let sag = r - ms.hypot(mz - (z_cut + r));
        assert!(sag <= tol + 4.0 * SNAP, "chord sagitta {sag} past tol {tol}");
        prev = (s, q.z);
    }
    let last = l.fan.last().expect("a fan has a last vertex");
    assert_eq!((last.x, last.y, last.z), (e.x, e.y, z_cut), "the fan lands on the entry exactly");
    // Tangency at the bottom: the circle's center stands straight over
    // the entry (the radius at the touch point is vertical, so the
    // tangent there is horizontal along +u), and the last chord's
    // slope is bounded by its half-angle - it shrinks with the tol.
    if l.fan.len() >= 2 {
        let q = l.fan[l.fan.len() - 2];
        let ds = s_of(last.x, last.y) - s_of(q.x, q.y);
        let dz = q.z - last.z;
        assert!(ds > 0.0, "the last chord travels along +u");
        let half = (1.0 - tol.min(r) / r).acos();
        assert!(dz.atan2(ds) <= half + 1e-6, "the last chord dives too steeply to be tangent");
    }
}

#[test]
fn the_arc_is_tangent_at_the_bottom_and_stands_in_the_cut_plane() {
    for (dir, nominal, z_top, z_cut, tol) in [
        (Point2::new(1.0, 0.0), 10.0, 5.0, 0.0, 0.01),
        (Point2::new(0.0, -3.0), 6.0, -1.0, -3.0, 0.001),
        (Point2::new(1.0, 1.0), 4.0, 2.0, -2.0, 0.01), // R == h: quarter turn
        (Point2::new(-2.0, 0.5), 50.0, 0.25, 0.0, 0.05),
    ] {
        let e = Point2::new(12.5, -7.25);
        let l = arc_lead(e, dir, z_top, z_cut, nominal, tol, free).expect("free room certifies");
        let n = dir.x.hypot(dir.y);
        let u = Point2::new(dir.x / n, dir.y / n);
        assert_eq!(l.radius, nominal.max(z_top - z_cut), "free room keeps the nominal");
        assert_lead_laws(&l, e, u, z_top, z_cut, tol);
    }
}

#[test]
fn the_quarter_turn_starts_one_drop_behind_and_vertical() {
    // R == h: the sweep is exactly 90 degrees, the lead exactly h.
    let e = Point2::new(0.0, 0.0);
    let l = arc_lead(e, Point2::new(1.0, 0.0), 3.0, 0.0, 1.0, 0.01, free).expect("quarter");
    assert_eq!(l.radius, 3.0, "a nominal under the drop is raised to it");
    assert!((l.start.x + 3.0).abs() <= SNAP && l.start.y == 0.0, "lead = h: {:?}", l.start);
}

#[test]
fn a_micron_skew_turns_the_lead_onto_its_axis_plane_and_a_real_one_does_not() {
    // M11.2 B: a tessellated wall's first cut is axis-aligned only to
    // its fit residual. Within the chord tolerance of sideways travel
    // the lead lies IN the axis plane through the (off-lattice) entry -
    // one G19 arc downstream - and its chord is what `clears` proved.
    let e = Point2::new(3.000_000_9, -11.999_623_9);
    let skew = Point2::new(3.5e-7, 1.0);
    let seen = std::cell::RefCell::new(Vec::new());
    let room = |s: Point2, q: Point2| {
        seen.borrow_mut().push(s);
        q == e
    };
    let l = arc_lead(e, skew, 5.0, 0.0, 10.0, 0.01, room).expect("snapped");
    assert_eq!(l.start.x, e.x, "the lead lies in the YZ plane through e: {:?}", l.start);
    assert!(l.fan.iter().all(|q| q.x == e.x), "every fan vertex too");
    assert_eq!(seen.borrow().last(), Some(&l.start), "the snapped chord is the proved one");
    // Refused room for the snapped lead: the exact tangent is asked
    // next, and certifies on its own chord.
    let exact = |s: Point2, _: Point2| s.x != e.x;
    let l = arc_lead(e, skew, 5.0, 0.0, 10.0, 0.01, exact).expect("exact");
    assert_ne!(l.start.x, e.x, "the unsnapped lead: {:?}", l.start);
    assert_lead_laws(&l, e, Point2::new(3.5e-7, 1.0), 5.0, 0.0, 0.01);
    // A real angle (sideways travel far past the tolerance) is not
    // snapped: the tangent is the cut's own.
    let d = Point2::new(0.1, 1.0);
    let l = arc_lead(e, d, 5.0, 0.0, 10.0, 0.01, free).expect("oblique");
    let n = d.x.hypot(d.y);
    assert_lead_laws(&l, e, Point2::new(d.x / n, d.y / n), 5.0, 0.0, 0.01);
}

#[test]
fn a_tight_room_shrinks_the_radius_toward_the_drop() {
    // The room admits a lead of 4 mm only: the nominal 20 (lead ~13.7
    // for h = 5) is refused, the halvings land on one that fits.
    let e = Point2::new(0.0, 0.0);
    let room = |s: Point2, q: Point2| (s.x - q.x).hypot(s.y - q.y) <= 6.0;
    let l = arc_lead(e, Point2::new(1.0, 0.0), 5.0, 0.0, 20.0, 0.01, room).expect("fits shrunk");
    assert!(l.radius < 20.0 && l.radius >= 5.0, "shrunk, never below the drop: {}", l.radius);
    assert!(e.x - l.start.x <= 6.0, "the lead fits the room");
    assert_lead_laws(&l, e, Point2::new(1.0, 0.0), 5.0, 0.0, 0.01);
}

#[test]
fn no_room_is_refused_by_name() {
    let e = Point2::new(0.0, 0.0);
    let never = |_: Point2, _: Point2| false;
    let err = arc_lead(e, Point2::new(1.0, 0.0), 5.0, 0.0, 10.0, 0.01, never).unwrap_err();
    assert!(err.contains("lead room is blocked"), "{err}");
    // A room shorter than the quarter turn's lead (= the drop) refuses
    // too: no arc covers the drop without a vertical leg.
    let short = |s: Point2, q: Point2| (s.x - q.x).hypot(s.y - q.y) < 4.9;
    let err = arc_lead(e, Point2::new(1.0, 0.0), 5.0, 0.0, 10.0, 0.01, short).unwrap_err();
    assert!(err.contains("lead room is blocked"), "{err}");
}

#[test]
fn degenerate_inputs_error_and_never_panic() {
    let e = Point2::new(1.0, 2.0);
    let x = Point2::new(1.0, 0.0);
    let nan = f64::NAN;
    let cases: [(Point2, Point2, f64, f64, f64, f64, &str); 10] = [
        (e, x, 0.0, 0.0, 10.0, 0.01, "no drop"),     // zero drop
        (e, x, -1.0, 0.0, 10.0, 0.01, "no drop"),    // a rise, not a drop
        (e, x, nan, 0.0, 10.0, 0.01, "no drop"),     // NaN top
        (e, x, 5.0, f64::INFINITY, 10.0, 0.01, "no drop"),
        (Point2::new(nan, 0.0), x, 5.0, 0.0, 10.0, 0.01, "no drop"),
        (e, Point2::new(0.0, 0.0), 5.0, 0.0, 10.0, 0.01, "no direction"),
        (e, Point2::new(nan, 1.0), 5.0, 0.0, 10.0, 0.01, "no direction"),
        (e, x, 5.0, 0.0, 10.0, 0.0, "tolerance"),
        (e, x, 5.0, 0.0, 10.0, nan, "tolerance"),
        (e, x, 1e-9, 0.0, 1e-9, 0.01, "shorter than the lattice"), // tiny radius
    ];
    for (e, d, zt, zc, r, tol, want) in cases {
        let err = arc_lead(e, d, zt, zc, r, tol, free).unwrap_err();
        assert!(err.contains(want), "({zt}, {zc}, {r}, {tol}): {err} lacks {want:?}");
    }
    // A tolerance so fine the fan would pass its budget refuses.
    let err = arc_lead(e, x, 5.0, 0.0, 1000.0, 1e-15, free).unwrap_err();
    assert!(err.contains("budget"), "{err}");
    // NaN or absurd nominals fall to the quarter turn or refuse - with
    // bounded work either way.
    let l = arc_lead(e, x, 2.0, 0.0, nan, 0.01, free).expect("NaN nominal asks the quarter turn");
    assert_eq!(l.radius, 2.0);
    assert!(arc_lead(e, x, 2.0, 0.0, 1e300, 0.01, free).is_ok_or_named());
    let l = arc_lead(e, x, 2.0, 0.0, -5.0, 0.01, free).expect("negative nominal -> quarter");
    assert!(l.fan.len() <= ARC_ENTRY_MAX_CHORDS);
}

/// `Ok`, or an `Err` that names itself: the only two answers allowed.
trait Named {
    fn is_ok_or_named(&self) -> bool;
}
impl Named for Result<ArcLead, &'static str> {
    fn is_ok_or_named(&self) -> bool {
        match self {
            Ok(l) => l.fan.len() <= ARC_ENTRY_MAX_CHORDS,
            Err(why) => why.starts_with("the arc"),
        }
    }
}

fn cp() -> ContourParams {
    ContourParams {
        tool_diameter: 10.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 3.0,
        top_z: 0.0,
        bottom_z: -2.0,
        stepdown: 1.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Inside,
        lead: Lead::Arc { radius: 3.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Arc,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        picks: Default::default(),
    }
}

/// The descents into each level: maximal runs of Feed moves whose Z
/// falls - chord fans, or (M11.2 B) true G18/G19 arcs, sampled along
/// their sweep so one law reads both - ending on a plane.
fn descents(t: &Toolpath) -> Vec<Vec<Point3>> {
    let mut out = Vec::new();
    let mut run: Vec<Point3> = Vec::new();
    for m in &t.moves {
        let vertical = m.arc.is_none_or(|a| !a.plane.is_xy());
        if m.kind == MoveKind::Feed && vertical && m.to.z < m.from.z {
            if run.is_empty() {
                run.push(m.from);
            }
            if let Some(a) = m.arc {
                run.extend((1..16).filter_map(|k| a.point_at(m.from, m.to, f64::from(k) / 16.0)));
            }
            run.push(m.to);
        } else if !run.is_empty() {
            out.push(std::mem::take(&mut run));
        }
    }
    if !run.is_empty() {
        out.push(run);
    }
    out
}

/// Every fan vertex in one vertical plane, on one circle tangent to
/// the plane at the fan's end, and the following cut move leaving the
/// end along the fan's own horizontal direction.
fn assert_fan_is_a_tangent_arc(f: &[Point3]) {
    let (a, e) = (f[0], f[f.len() - 1]);
    let (dx, dy) = (e.x - a.x, e.y - a.y);
    let len = dx.hypot(dy);
    assert!(len > 0.0, "a fan travels in XY");
    let u = (dx / len, dy / len);
    let h = a.z - e.z;
    // The circle through start and end, tangent at the end: (len^2 +
    // h^2) / (2h) is its radius.
    let r = (len * len + h * h) / (2.0 * h);
    for q in f {
        let s = (q.x - e.x) * u.0 + (q.y - e.y) * u.1;
        let off = ((q.x - e.x) * u.1 - (q.y - e.y) * u.0).abs();
        assert!(off <= 2e-6, "fan vertex off the vertical plane: {q:?}");
        let d = (s.hypot(q.z - (e.z + r)) - r).abs();
        assert!(d <= 5e-6, "fan vertex off the tangent circle by {d}: {q:?}");
    }
}

#[test]
fn the_bore_contour_enters_each_level_on_a_tangent_arc() {
    let p = cp();
    // A round bore: the arc lead-in starts in the air, off the wall, so
    // the room behind it is the bore's own floor.
    let t = contour(&disk(20.0), &p).expect("arc bore valid");
    assert!(t.is_chained() && t.all_finite());
    assert!(t.notes.is_empty(), "the bore holds the lead: {:?}", t.notes);
    assert!(!t.moves.iter().any(|m| m.kind == MoveKind::Plunge), "no axial dive anywhere");
    let fans = descents(&t);
    // Levels -1, -2 (#2004: no air level at Top Z 0): the first from
    // clearance, the rest from the plane above - one arc each.
    assert_eq!(fans.len(), 2, "one arc per level: {fans:?}");
    for f in &fans {
        assert!(f.len() >= 3, "a fan, not one straight ramp chord: {f:?}");
        assert_fan_is_a_tangent_arc(f);
        // The first cut after the fan leaves the end along the fan's
        // own direction: the lead-in arc's tangent at its start.
        let end = f[f.len() - 1];
        let i = t.moves.iter().position(|m| m.from == end && m.to.z == end.z).expect("cut");
        let m = &t.moves[i];
        let c = m.arc.expect("the lead-in is an arc").center;
        let (rx, ry) = (end.x - c.x, end.y - c.y);
        let tan = if m.arc.is_some_and(|a| a.ccw) { (-ry, rx) } else { (ry, -rx) };
        let a = f[0];
        let cross = (end.x - a.x) * tan.1 - (end.y - a.y) * tan.0;
        let dot = (end.x - a.x) * tan.0 + (end.y - a.y) * tan.1;
        assert!(dot > 0.0 && cross.abs() <= 1e-4 * dot.abs().max(1.0), "not tangent: {cross}");
    }
}

#[test]
fn a_bore_too_small_for_the_lead_substitutes_and_says_it() {
    // A 12 mm bore under a 10 mm tool leaves a 2 mm cutter-center
    // square: no arc covering a 5 mm drop fits behind the entry.
    let mut p = cp();
    p.lead = Lead::None;
    let t = contour(&rect(0.0, 0.0, 12.0, 12.0), &p).expect("valid");
    assert!(t.is_chained() && t.all_finite());
    assert!(
        t.notes.iter().any(|n| n.contains("arc's lead room is blocked")),
        "the fallback is named: {:?}",
        t.notes
    );
}

#[test]
fn the_wall_side_enters_on_a_tangent_arc_in_the_air_beside_the_wall() {
    // The wall side (hand-test report): the arc is no longer refused.
    // Each level descends one tangent arc whose every vertex stands
    // outside the part's tool-radius offset - the air beside the wall -
    // and the walked cut is the plunge entry's, move for move.
    let part = rect(0.0, 0.0, 30.0, 30.0);
    let mut base = cp();
    base.side = Side::Outside;
    base.entry = Entry3D::Plunge;
    let plain = contour(&part, &base).expect("graze");
    let mut p = base;
    p.entry = Entry3D::Arc;
    let t = contour(&part, &p).expect("wall arc valid");
    assert!(t.is_chained() && t.all_finite());
    assert!(!t.notes.iter().any(|n| n.contains("bounded air")), "{:?}", t.notes);
    let f = descents(&t);
    assert_eq!(f.len(), 2, "levels -1, -2 each enter on one arc (#2004)");
    for fan in &f {
        assert_fan_is_a_tangent_arc(fan);
        for q in fan {
            // Distance from the cutter center to the 30 x 30 part: at
            // least the tool radius (the grown region's round joins
            // flatten by a few microns).
            let dx = (-q.x).max(q.x - 30.0).max(0.0);
            let dy = (-q.y).max(q.y - 30.0).max(0.0);
            assert!(dx.hypot(dy) >= 5.0 - 0.01, "fan vertex inside the part offset: {q:?}");
        }
    }
    let planar = |t: &Toolpath| -> Vec<_> {
        t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z && m.arc.is_some()).map(|m| (m.from, m.to)).collect()
    };
    assert_eq!(planar(&t), planar(&plain), "the walked cut is unchanged");
}

fn pp() -> PocketParams {
    PocketParams {
        blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        tool_diameter: 6.0,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Arc,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    }
}

#[test]
fn a_pocket_arc_entry_is_tangent_or_named_and_always_safe() {
    for strategy in [PocketStrategy::Zigzag, PocketStrategy::Waterline] {
        let mut p = pp();
        p.strategy = strategy;
        let t = pocket(&rect(0.0, 0.0, 60.0, 40.0), &p).expect("arc pocket valid");
        assert!(t.is_chained() && t.all_finite(), "{strategy:?}");
        let fans: Vec<_> = descents(&t).into_iter().filter(|f| f.len() >= 3).collect();
        // Every level either rode a certified fan or said why not.
        let said = t.notes.iter().filter(|n| n.starts_with("entry: the arc")).count();
        assert!(fans.len() + said >= 2, "{strategy:?}: fans {fans:?}, notes {:?}", t.notes);
        for f in &fans {
            assert_fan_is_a_tangent_arc(f);
        }
    }
}

#[test]
fn a_slot_arc_entry_stays_inside_the_channel_stadium() {
    let p = SlotParams {
        links: camcraft_motion::links::Links::PLANAR,
        tool_diameter: 6.0,
        width: 10.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -2.0,
        stepdown: 1.0,
        stepover: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        entry: Entry3D::Arc,
        helix_radius: 1.0, blend_turns: false, blend_radius: None,
    };
    let (a, b) = (Point2::new(0.0, 0.0), Point2::new(50.0, 0.0));
    let t = slot(a, b, &p).expect("arc slot valid");
    assert!(t.is_chained() && t.all_finite());
    // The cutter-center stadium half-width is 2: any fan's XY stays in
    // it (a fan certified there), else the level said its fallback.
    for f in descents(&t) {
        for q in &f {
            let cx = q.x.clamp(a.x, b.x);
            assert!((q.x - cx).hypot(q.y) <= 2.0 + 1e-6, "fan left the stadium: {q:?}");
        }
    }
}

#[test]
fn the_arc_entry_is_deterministic() {
    let a = contour(&disk(20.0), &cp()).expect("a");
    let b = contour(&disk(20.0), &cp()).expect("b");
    assert_eq!(a.moves, b.moves);
}
