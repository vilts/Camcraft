//! M11.1 B/C turning-point blends at contour corners and between-pass
//! links: every fillet is ONE arc tangent at both ends, its radius
//! inside [floor, tool radius], no point of the blended path comes
//! closer to the part than the sharp path did (the gouge law, sampled
//! on the true arcs), the corner rest the rounding leaves is stated and
//! matches what the geometry measures, refusals keep their sharp corner
//! and are counted, and degenerate input never panics.
//!
//! Gate map (M11.1): covers camcraft_motion::machining2d (fillet, contour, pocket, levels).

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{
    contour, contour_along_chain, fillet_corners, fillet_floor, pocket, ContourParams, Lead,
    Legal, PocketParams, PocketStrategy, Seg, Side, MIN_BLEND_RADIUS,
};

fn poly(pts: &[(f64, f64)]) -> Loop {
    Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap()
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
}

/// An L-shaped boss: one concave (reflex) corner at (20, 20).
fn l_boss() -> Region {
    Region::new(vec![poly(&[
        (0.0, 0.0),
        (40.0, 0.0),
        (40.0, 20.0),
        (20.0, 20.0),
        (20.0, 40.0),
        (0.0, 40.0),
    ])])
    .unwrap()
}

/// 40x30 plate with a 16x12 rectangular through hole.
fn plate_with_hole() -> Region {
    Region::new(vec![rect(0.0, 0.0, 40.0, 30.0), rect(12.0, 9.0, 28.0, 21.0)]).unwrap()
}

fn square_with_island() -> Region {
    Region::new(vec![rect(0.0, 0.0, 30.0, 30.0), rect(10.0, 10.0, 20.0, 20.0)]).unwrap()
}

fn cp(side: Side, climb: bool, blend: bool) -> ContourParams {
    ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb,
        side,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: blend, blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        picks: Default::default(),
    }
}

fn pp(strategy: PocketStrategy, blend: bool) -> PocketParams {
    PocketParams {
        blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        tool_diameter: 8.0,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 400.0,
        plunge_feed: 80.0,
        climb: true,
        strategy,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: blend,
    }
}

// ---- measuring helpers ---------------------------------------------------

fn seg_dist(q: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    if l2 == 0.0 {
        return (q.x - a.x).hypot(q.y - a.y);
    }
    let t = (((q.x - a.x) * dx + (q.y - a.y) * dy) / l2).clamp(0.0, 1.0);
    (q.x - a.x - t * dx).hypot(q.y - a.y - t * dy)
}

/// Distance from `q` to the part outline (every loop edge).
fn to_part(region: &Region, q: Point2) -> f64 {
    let mut d = f64::INFINITY;
    for l in region.loops() {
        let p = l.points();
        for i in 0..p.len() {
            d = d.min(seg_dist(q, p[i], p[(i + 1) % p.len()]));
        }
    }
    d
}

/// Points along a planar move ON the true curve: lines every ~0.05 mm,
/// arcs by angle (radius from the start point, sweep in travel sense).
fn samples(m: &Move) -> Vec<Point2> {
    let a = Point2::new(m.from.x, m.from.y);
    let b = Point2::new(m.to.x, m.to.y);
    match m.arc {
        None => {
            let n = (((b.x - a.x).hypot(b.y - a.y) / 0.05).ceil() as usize).clamp(1, 4000);
            (0..=n)
                .map(|k| {
                    let t = k as f64 / n as f64;
                    Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
                })
                .collect()
        }
        Some(arc) => {
            let c = arc.center;
            let rho = (a.x - c.x).hypot(a.y - c.y);
            let s = (a.y - c.y).atan2(a.x - c.x);
            let e = (b.y - c.y).atan2(b.x - c.x);
            let tau = std::f64::consts::TAU;
            let sweep = if arc.ccw { (e - s).rem_euclid(tau) } else { (s - e).rem_euclid(tau) };
            let n = 256;
            (0..=n)
                .map(|k| {
                    let t = s + if arc.ccw { 1.0 } else { -1.0 } * sweep * k as f64 / n as f64;
                    Point2::new(c.x + rho * t.cos(), c.y + rho * t.sin())
                })
                .collect()
        }
    }
}

/// Cutting moves in the plane of `z` (no plunges, lifts or rapids).
fn cuts(t: &Toolpath, z: f64) -> Vec<&Move> {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == z && m.to.z == z)
        .collect()
}

fn min_to_part(t: &Toolpath, z: f64, region: &Region) -> f64 {
    cuts(t, z)
        .iter()
        .flat_map(|m| samples(m))
        .map(|q| to_part(region, q))
        .fold(f64::INFINITY, f64::min)
}

fn tangent(m: &Move, start: bool) -> (f64, f64) {
    let (a, b) = ((m.from.x, m.from.y), (m.to.x, m.to.y));
    let v = match m.arc {
        None => (b.0 - a.0, b.1 - a.1),
        Some(arc) => {
            let p = if start { a } else { b };
            let (dx, dy) = (p.0 - arc.center.x, p.1 - arc.center.y);
            if arc.ccw { (-dy, dx) } else { (dy, -dx) }
        }
    };
    let l = v.0.hypot(v.1);
    (v.0 / l, v.1 / l)
}

fn is_new_arc(m: &Move, sharp: &Toolpath) -> bool {
    m.arc.is_some()
        && !sharp.moves.iter().any(|s| s.arc == m.arc && s.to == m.to && s.from == m.from)
}

/// Every arc the blend added: tangent to both neighbours (the center
/// rides the 1 um lattice, so the angle budget is micro-radians per mm
/// of radius) and radius in [floor, r].
fn assert_fillets_tangent_and_bounded(blended: &Toolpath, sharp: &Toolpath, r: f64, tol: f64) -> usize {
    let mut n = 0;
    for (i, m) in blended.moves.iter().enumerate() {
        if !is_new_arc(m, sharp) {
            continue;
        }
        n += 1;
        let Some(arc) = m.arc else { continue };
        let rho = (m.from.x - arc.center.x).hypot(m.from.y - arc.center.y);
        assert!(rho <= r + 1e-6, "fillet radius {rho} over the tool radius {r}");
        assert!(rho >= fillet_floor(tol) - 1e-6, "fillet radius {rho} under the floor");
        let budget = 2e-6 / rho + 1e-9;
        let before = &blended.moves[i - 1];
        let after = &blended.moves[i + 1];
        for (u, w) in [(tangent(before, false), tangent(m, true)), (tangent(m, false), tangent(after, true))] {
            let cross = u.0 * w.1 - u.1 * w.0;
            let dot = u.0 * w.0 + u.1 * w.1;
            assert!(dot > 0.0 && cross.abs() <= budget, "kink {cross} at a fillet end (move {i})");
        }
    }
    n
}

fn notes_with(t: &Toolpath, word: &str) -> Vec<String> {
    t.notes.iter().filter(|n| n.contains(word)).cloned().collect()
}

/// The `up to X mm` figure of the rounding note.
fn stated_rest(t: &Toolpath) -> f64 {
    let n = notes_with(t, "rounded");
    let s = n.first().expect("a rounding note");
    let after = s.split("up to ").nth(1).expect("rest figure");
    after.split(' ').next().and_then(|v| v.parse().ok()).expect("number")
}

// ---- the pure fillet -----------------------------------------------------

fn open_legal(region: &Region) -> Legal<'_> {
    Legal { region, inside: true, air_left: None }
}

#[test]
fn a_right_angle_rounds_tangent_on_both_legs() {
    let room = Region::new(vec![rect(-50.0, -50.0, 50.0, 50.0)]).unwrap();
    let walk = vec![Seg::Line(vec![Point2::new(20.0, 0.0), Point2::new(20.0, 20.0)])];
    let (out, t) = fillet_corners(&walk, Point2::new(0.0, 0.0), 5.0, 0.01, &open_legal(&room));
    assert_eq!((t.corners, t.rounded), (1, 1), "{t:?}");
    assert_eq!(
        out,
        vec![
            Seg::Line(vec![Point2::new(15.0, 0.0)]),
            Seg::Arc { center: Point2::new(15.0, 5.0), ccw: true, to: Point2::new(20.0, 5.0) },
            Seg::Line(vec![Point2::new(20.0, 20.0)]),
        ]
    );
    // e = rho (sec 45 - 1): the stated rest is the bisector offset.
    assert!((t.max_rest - 5.0 * (2f64.sqrt() - 1.0)).abs() < 1e-12, "{}", t.max_rest);
}

#[test]
fn the_radius_is_bounded_by_tool_legs_and_tolerance() {
    let room = Region::new(vec![rect(-50.0, -50.0, 50.0, 50.0)]).unwrap();
    let corner = |leg: f64| vec![Seg::Line(vec![Point2::new(leg, 0.0), Point2::new(leg, leg)])];
    let radius = |out: &[Seg]| match out.get(1) {
        Some(Seg::Arc { center, to, .. }) => (to.x - center.x).hypot(to.y - center.y),
        _ => 0.0,
    };
    // Tool bound: legs of 40 would allow 20, the tool says 3.
    let (out, _) = fillet_corners(&corner(40.0), Point2::new(0.0, 0.0), 3.0, 0.01, &open_legal(&room));
    assert!((radius(&out) - 3.0).abs() < 1e-12);
    // Leg bound: t = rho tan(45) <= leg/2.
    let (out, _) = fillet_corners(&corner(4.0), Point2::new(0.0, 0.0), 3.0, 0.01, &open_legal(&room));
    assert!((radius(&out) - 2.0).abs() < 1e-12);
    // Tolerance bound: legs of 0.03 leave 0.015, under a 0.02 tolerance.
    let (out, t) = fillet_corners(&corner(0.03), Point2::new(0.0, 0.0), 3.0, 0.02, &open_legal(&room));
    assert_eq!((t.rounded, t.tiny), (0, 1));
    assert_eq!(out, corner(0.03), "a refused walk comes back verbatim");
    // Resolution bound: the floor never drops under MIN_BLEND_RADIUS.
    assert_eq!(fillet_floor(1e-6), MIN_BLEND_RADIUS);
    assert_eq!(fillet_floor(f64::NAN), MIN_BLEND_RADIUS);
    let (_, t) = fillet_corners(&corner(0.015), Point2::new(0.0, 0.0), 3.0, 1e-6, &open_legal(&room));
    assert_eq!(t.tiny, 1);
}

#[test]
fn degenerate_walks_round_nothing_and_never_panic() {
    let room = Region::new(vec![rect(-50.0, -50.0, 50.0, 50.0)]).unwrap();
    let l = open_legal(&room);
    let o = Point2::new(0.0, 0.0);
    // Empty and single-point walks.
    assert_eq!(fillet_corners(&[], o, 3.0, 0.01, &l).1.corners, 0);
    assert_eq!(fillet_corners(&[Seg::Line(vec![o])], o, 3.0, 0.01, &l).1.corners, 0);
    // Zero-length pieces between real legs: still one corner, rounded.
    let dup = vec![Seg::Line(vec![
        Point2::new(10.0, 0.0),
        Point2::new(10.0, 0.0),
        Point2::new(10.0, 10.0),
        Point2::new(10.0, 10.0),
    ])];
    assert_eq!(fillet_corners(&dup, o, 3.0, 0.01, &l).1.rounded, 1);
    // Collinear: no corner at all, the walk verbatim.
    let line = vec![Seg::Line(vec![Point2::new(5.0, 0.0), Point2::new(10.0, 0.0)])];
    let (out, t) = fillet_corners(&line, o, 3.0, 0.01, &l);
    assert_eq!((t.corners, out), (0, line));
    // A 180 degree reversal: named, sharp.
    let back = vec![Seg::Line(vec![Point2::new(10.0, 0.0), Point2::new(2.0, 0.0)])];
    let (out, t) = fillet_corners(&back, o, 3.0, 0.01, &l);
    assert_eq!((t.corners, t.reversal, t.rounded), (1, 1, 0));
    assert_eq!(out, back);
    // Non-finite and non-positive radii round nothing.
    let corner = vec![Seg::Line(vec![Point2::new(10.0, 0.0), Point2::new(10.0, 10.0)])];
    for r in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
        let (out, t) = fillet_corners(&corner, o, r, 0.01, &l);
        assert_eq!((t.rounded, t.tiny), (0, 1), "r = {r}");
        assert_eq!(out, corner);
    }
    // A curved neighbour: counted, sharp.
    let curved = vec![
        Seg::Line(vec![Point2::new(10.0, 0.0)]),
        Seg::Arc { center: Point2::new(10.0, 5.0), ccw: false, to: Point2::new(10.0, 10.0) },
    ];
    let (_, t) = fillet_corners(&curved, o, 3.0, 0.01, &l);
    assert_eq!((t.curved, t.rounded), (1, 0), "{t:?}");
    // Non-finite points never become arcs.
    let nan = vec![Seg::Line(vec![Point2::new(f64::NAN, 0.0), Point2::new(10.0, 10.0)])];
    assert_eq!(fillet_corners(&nan, o, 3.0, 0.01, &l).1.rounded, 0);
}

#[test]
fn a_turn_toward_the_part_stays_sharp_and_is_named() {
    let room = Region::new(vec![rect(-50.0, -50.0, 50.0, 50.0)]).unwrap();
    // A left turn while the air is on the right: the part is inside it.
    let legal = Legal { region: &room, inside: true, air_left: Some(false) };
    let walk = vec![Seg::Line(vec![Point2::new(20.0, 0.0), Point2::new(20.0, 20.0)])];
    let (out, t) = fillet_corners(&walk, Point2::new(0.0, 0.0), 5.0, 0.01, &legal);
    assert_eq!((t.toward_part, t.rounded), (1, 0));
    assert_eq!(out, walk);
    assert!(t.notes("walked", 0.01)[0].contains("toward the part"));
}

#[test]
fn an_island_in_the_corner_shrinks_or_blocks_the_arc() {
    // A 0.4 mm island standing 1 mm inside the corner's bisector.
    let isl = rect(18.6, 0.6, 19.0, 1.0);
    let room = Region::new(vec![rect(-50.0, -50.0, 50.0, 50.0), isl]).unwrap();
    let walk = vec![Seg::Line(vec![Point2::new(20.0, 0.0), Point2::new(20.0, 20.0)])];
    let (out, t) = fillet_corners(&walk, Point2::new(0.0, 0.0), 5.0, 0.01, &open_legal(&room));
    assert_eq!(t.corners, 1);
    if t.rounded == 1 {
        let Some(Seg::Arc { center, to, .. }) = out.get(1) else { panic!("{out:?}") };
        let rho = (to.x - center.x).hypot(to.y - center.y);
        assert!(rho < 5.0, "a shrunk radius, never the blocked one: {rho}");
        // The corner triangle V T1 T2 that holds the arc - the points
        // with (20 - x) + y <= rho - misses every island corner.
        for (x, y) in [(18.6, 0.6), (19.0, 0.6), (19.0, 1.0), (18.6, 1.0)] {
            assert!((20.0 - x) + y > rho, "{rho}");
        }
    } else {
        assert_eq!(t.blocked, 1);
        assert_eq!(out, walk);
    }
    // An island INSIDE the full-size triangle refuses the full radius.
    let isl = rect(17.0, 1.0, 17.5, 1.5);
    let room = Region::new(vec![rect(-50.0, -50.0, 50.0, 50.0), isl]).unwrap();
    let (out, _) = fillet_corners(&walk, Point2::new(0.0, 0.0), 5.0, 0.01, &open_legal(&room));
    if let Some(Seg::Arc { center, to, .. }) = out.get(1) {
        assert!((to.x - center.x).hypot(to.y - center.y) < 5.0);
    }
}

// ---- contour corners (M11.1 B) -------------------------------------------

#[test]
fn an_outside_l_boss_rounds_its_concave_corner_off_the_part() {
    let region = l_boss();
    for climb in [true, false] {
        let sharp = contour(&region, &cp(Side::Outside, climb, false)).unwrap();
        let blended = contour(&region, &cp(Side::Outside, climb, true)).unwrap();
        assert!(sharp.notes.is_empty(), "{:?}", sharp.notes);
        // One concave corner per level, two levels (-2, -4; #2004: no
        // air level at Top Z 0).
        let added = assert_fillets_tangent_and_bounded(&blended, &sharp, 3.0, 0.01);
        assert_eq!(added, 2, "climb {climb}");
        assert_eq!(blended.arc_count(), sharp.arc_count() + 2);
        assert_eq!(notes_with(&blended, "rounded").len(), 1, "{:?}", blended.notes);
        assert!(blended.is_chained() && blended.all_finite());
        for z in [-2.0, -4.0] {
            let d_sharp = min_to_part(&sharp, z, &region);
            let d_blend = min_to_part(&blended, z, &region);
            // The round joins ride fitted arcs, within the fitter's own
            // 0.01 mm of the offset - the sharp path's standing budget.
            assert!((d_sharp - 3.0).abs() <= 0.01 + 1e-9, "{d_sharp}");
            assert!(d_blend >= d_sharp - 1e-6, "gouge: {d_blend} < {d_sharp}");
        }
    }
}

#[test]
fn a_bore_rounds_its_corners_and_states_the_rest_it_leaves() {
    // The plate's rectangular hole cut from inside: four corners per
    // level turn toward the bore's air; the ring starts on one of them,
    // which stays sharp (the entry stands there) and is named.
    let hole = Region::new(vec![rect(12.0, 9.0, 28.0, 21.0)]).unwrap();
    let sharp = contour(&hole, &cp(Side::Inside, true, false)).unwrap();
    let blended = contour(&hole, &cp(Side::Inside, true, true)).unwrap();
    let added = assert_fillets_tangent_and_bounded(&blended, &sharp, 3.0, 0.01);
    // Two levels (-2, -4; #2004: no air level at Top Z 0).
    assert_eq!(added, 6, "{:?}", blended.notes);
    assert_eq!(notes_with(&blended, "6 of 8 walked corner(s) rounded").len(), 1, "{:?}", blended.notes);
    assert_eq!(notes_with(&blended, "2 of 8 walked corner(s) kept sharp - the walk starts").len(), 1);
    let rest = stated_rest(&blended);
    // Measurable: every sharp corner point of the walk stands exactly the
    // stated rest from the blended path (to the note's 4 decimals), and
    // no farther anywhere.
    let z = -4.0;
    let blend_pts: Vec<Point2> = cuts(&blended, z).iter().flat_map(|m| samples(m)).collect();
    let mut worst: f64 = 0.0;
    for m in cuts(&sharp, z) {
        let v = Point2::new(m.to.x, m.to.y);
        let d = blend_pts.iter().map(|q| (q.x - v.x).hypot(q.y - v.y)).fold(f64::INFINITY, f64::min);
        worst = worst.max(d);
    }
    assert!((worst - rest).abs() < 2e-3, "measured {worst}, stated {rest}");
    // Never closer to the wall than the sharp walk: the bore's wall is
    // the hole outline.
    for z in [-2.0, -4.0] {
        let d_sharp = min_to_part(&sharp, z, &hole);
        let d_blend = min_to_part(&blended, z, &hole);
        assert!(d_blend >= d_sharp - 1e-6, "gouge: {d_blend} < {d_sharp}");
    }
}

#[test]
fn a_plate_hole_seen_from_the_outside_law_rounds_the_hole_corners() {
    // Auto on a plate with a hole is the Outside law: the outer ring's
    // corners are round joins already, the hole's ring is sharp and
    // turns toward the hole's air.
    let region = plate_with_hole();
    let sharp = contour(&region, &cp(Side::Auto, true, false)).unwrap();
    let blended = contour(&region, &cp(Side::Auto, true, true)).unwrap();
    let added = assert_fillets_tangent_and_bounded(&blended, &sharp, 3.0, 0.01);
    // Two levels (-2, -4; #2004: no air level at Top Z 0).
    assert_eq!(added, 6, "{:?}", blended.notes);
    for z in [-2.0, -4.0] {
        assert!(min_to_part(&blended, z, &region) >= min_to_part(&sharp, z, &region) - 1e-6);
    }
}

#[test]
fn nothing_to_round_leaves_the_contour_untouched() {
    // A plain rectangle boss: every corner of its grown ring is a
    // round join, there is no line-to-line corner to fillet.
    let region = Region::new(vec![rect(0.0, 0.0, 40.0, 20.0)]).unwrap();
    let sharp = contour(&region, &cp(Side::Outside, true, false)).unwrap();
    let blended = contour(&region, &cp(Side::Outside, true, true)).unwrap();
    assert_eq!(sharp.moves, blended.moves);
}

#[test]
fn an_open_run_says_its_corners_stay_sharp() {
    let run = [Point2::new(0.0, 0.0), Point2::new(10.0, 0.0), Point2::new(10.0, 10.0)];
    let sharp = contour_along_chain(&run, &cp(Side::Auto, true, false)).unwrap();
    let blended = contour_along_chain(&run, &cp(Side::Auto, true, true)).unwrap();
    assert_eq!(sharp.moves, blended.moves);
    assert_eq!(notes_with(&blended, "open centerline").len(), 1);
}

// ---- between-pass links (M11.1 C) ----------------------------------------

#[test]
fn zigzag_links_round_inside_the_cleared_floor() {
    let region = Region::new(vec![rect(0.0, 0.0, 30.0, 30.0)]).unwrap();
    let sharp = pocket(&region, &pp(PocketStrategy::Zigzag, false)).unwrap();
    let blended = pocket(&region, &pp(PocketStrategy::Zigzag, true)).unwrap();
    let added = assert_fillets_tangent_and_bounded(&blended, &sharp, 4.0, 0.01);
    assert!(added > 0);
    // Every row-to-link jog is a right angle and a link is at most one
    // stepover long (the band's last row snaps closer), so each fillet
    // is a quarter circle of at most stepover/2 - and the full rows get
    // exactly that.
    let radii: Vec<f64> = blended
        .moves
        .iter()
        .filter(|m| is_new_arc(m, &sharp))
        .filter_map(|m| m.arc.map(|a| (m.from.x - a.center.x).hypot(m.from.y - a.center.y)))
        .collect();
    assert!(radii.iter().all(|r| *r <= 1.5 + 1e-6), "{radii:?}");
    assert!(radii.iter().filter(|r| (**r - 1.5).abs() < 1e-6).count() >= radii.len() / 2, "{radii:?}");
    // Lifts, plunges and pass starts never moved.
    let plunges = |t: &Toolpath| t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).map(|m| m.to).collect::<Vec<_>>();
    assert_eq!(plunges(&sharp), plunges(&blended));
    for z in [0.0, -2.0, -4.0] {
        let d = min_to_part(&blended, z, &region);
        assert!(d >= min_to_part(&sharp, z, &region) - 1e-6 && d >= 4.0 - 1e-6, "gouge: {d}");
    }
    assert!(stated_rest(&blended) > 0.0);
}

#[test]
fn island_pockets_hold_the_gouge_law_for_both_strategies() {
    let region = square_with_island();
    for strategy in [PocketStrategy::Zigzag, PocketStrategy::Waterline] {
        let sharp = pocket(&region, &pp(strategy, false)).unwrap();
        let blended = pocket(&region, &pp(strategy, true)).unwrap();
        assert!(blended.is_chained() && blended.all_finite());
        assert_fillets_tangent_and_bounded(&blended, &sharp, 4.0, 0.01);
        for z in [0.0, -2.0, -4.0] {
            let d_sharp = min_to_part(&sharp, z, &region);
            let d_blend = min_to_part(&blended, z, &region);
            assert!(d_blend >= d_sharp - 1e-6, "{strategy:?} gouge: {d_blend} < {d_sharp}");
        }
        // Whatever the law refused is named, and the counts add up.
        for n in &blended.notes {
            assert!(n.starts_with("turn blends:") || n.starts_with("entry:"), "{n}");
        }
    }
}

#[test]
fn blends_off_say_nothing() {
    let region = square_with_island();
    let t = pocket(&region, &pp(PocketStrategy::Zigzag, false)).unwrap();
    assert!(notes_with(&t, "turn blends").is_empty());
    let t = contour(&l_boss(), &cp(Side::Outside, true, false)).unwrap();
    assert!(notes_with(&t, "turn blends").is_empty());
}

// ---- fuzz-lite: odd shapes never panic, never gouge ----------------------

#[test]
fn star_shapes_never_panic_and_never_gouge() {
    let mut seed: u64 = 0x5eed_ba11;
    let mut next = move || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((seed >> 11) as f64) / ((1u64 << 53) as f64)
    };
    for case in 0..12 {
        let n = 5 + (case % 7);
        let pts: Vec<(f64, f64)> = (0..n)
            .map(|k| {
                let a = std::f64::consts::TAU * k as f64 / n as f64;
                let rr = 12.0 + 14.0 * next();
                (30.0 + rr * a.cos(), 30.0 + rr * a.sin())
            })
            .collect();
        let region = Region::new(vec![poly(&pts)]).unwrap();
        for side in [Side::Outside, Side::Inside] {
            for climb in [true, false] {
                let (Ok(sharp), Ok(blended)) = (
                    contour(&region, &cp(side, climb, false)),
                    contour(&region, &cp(side, climb, true)),
                ) else {
                    continue;
                };
                assert!(blended.is_chained() && blended.all_finite());
                for z in [0.0, -2.0, -4.0] {
                    let (ds, db) = (min_to_part(&sharp, z, &region), min_to_part(&blended, z, &region));
                    assert!(db >= ds - 1e-6, "case {case} {side:?} climb {climb}: {db} < {ds}");
                }
            }
        }
        if let (Ok(sharp), Ok(blended)) = (
            pocket(&region, &pp(PocketStrategy::Zigzag, false)),
            pocket(&region, &pp(PocketStrategy::Zigzag, true)),
        ) {
            for z in [0.0, -2.0, -4.0] {
                let (ds, db) = (min_to_part(&sharp, z, &region), min_to_part(&blended, z, &region));
                assert!(db >= ds - 1e-6, "case {case} pocket: {db} < {ds}");
            }
        }
    }
}
