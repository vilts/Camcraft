//! A user-picked start point on a closed 2D contour (#1448): the ring
//! begins at the pick's projection on every level, the lead-in runs onto
//! that point, the automatic seam is unchanged without a pick, and every
//! case the first slice does not certify is refused by name. #1473: the
//! exit point - the ring cut all the way round and on to the pick's
//! projection, the lead-out there - and the picks no lead fits at,
//! refused by name rather than moved. A picked start with tabs: the tab
//! windows measured from the pick, every level entering there.
//!
//! Gate map (M4.4): covers camcraft_motion::machining2d (contour,
//! contour_start, contour_chain), camcraft_motion::approach.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::{Approach, Compensation, Links};
use camcraft_motion::machining2d::{contour, contour_along_chain, ContourParams, Lead, Side};
use camcraft_motion::verify::{crossed_pairs, verify, Mode, ToolGeom, VerifyContext, Warning};

fn square() -> Region {
    let pts = [(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)];
    Region::new(vec![Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap()]).unwrap()
}

fn params(pick: Option<Point2>) -> ContourParams {
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
        climb: true,
        side: Side::Outside,
        lead: Lead::Perpendicular { length: 5.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: Links { approach: Approach { start_point: pick, ..Approach::DEFAULT }, ..Links::PLANAR },
    }
}

/// Distance from `p` to the 40 x 40 square (outside it).
fn to_square(p: Point2) -> f64 {
    let ox = if p.x < 0.0 { -p.x } else if p.x > 40.0 { p.x - 40.0 } else { 0.0 };
    let oy = if p.y < 0.0 { -p.y } else if p.y > 40.0 { p.y - 40.0 } else { 0.0 };
    ox.hypot(oy)
}

/// Per cut level, where the cutter first reaches the cutter-centre ring
/// (one radius off the square): the end of the lead-in.
fn ring_starts(t: &Toolpath) -> Vec<Point2> {
    let mut levels: Vec<f64> = Vec::new();
    let mut out = Vec::new();
    for m in &t.moves {
        if m.kind != MoveKind::Feed || m.from.z != m.to.z || levels.contains(&m.to.z) {
            continue;
        }
        let q = Point2::new(m.to.x, m.to.y);
        if (to_square(q) - 3.0).abs() < 1e-6 {
            levels.push(m.to.z);
            out.push(q);
        }
    }
    out
}

fn refusal(p: &ContourParams) -> String {
    match contour(&square(), p) {
        Ok(_) => panic!("expected a refusal"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn a_picked_start_begins_every_level_at_its_projection() {
    // A click 1 mm outside the cutter path on the right wall.
    let t = contour(&square(), &params(Some(Point2::new(44.0, 20.0)))).unwrap();
    let starts = ring_starts(&t);
    assert!(starts.len() >= 2, "a multi-level ladder: {starts:?} {:?}", t.notes);
    for s in &starts {
        assert!((s.x - 43.0).abs() < 1e-9 && (s.y - 20.0).abs() < 1e-9, "starts at the pick's foot: {s:?}");
    }
    // The perpendicular lead-in comes off the air side, square to the wall.
    let plunge = t.moves.iter().find(|m| m.kind == MoveKind::Plunge).unwrap();
    assert!((plunge.to.y - 20.0).abs() < 1e-9 && plunge.to.x > 43.0, "the dive sits on the lead: {:?}", plunge.to);
}

#[test]
fn no_pick_keeps_the_automatic_seam() {
    let auto = contour(&square(), &params(None)).unwrap();
    let base = contour(&square(), &ContourParams { links: Links::PLANAR, ..params(None) }).unwrap();
    assert_eq!(auto.moves, base.moves, "an unset start point is the default path, move for move");
    let picked = contour(&square(), &params(Some(Point2::new(44.0, 20.0)))).unwrap();
    assert_ne!(ring_starts(&auto)[0], ring_starts(&picked)[0], "the pick moved the start");
}

#[test]
fn a_pick_far_from_every_wall_is_refused_by_name() {
    let why = refusal(&params(Some(Point2::new(80.0, 20.0))));
    assert!(why.contains("start point (80.000, 20.000)") && why.contains("farther than"), "{why}");
}

#[test]
fn a_non_finite_pick_is_refused_before_any_move() {
    let why = refusal(&params(Some(Point2::new(f64::NAN, 0.0))));
    assert!(why.contains("start point"), "{why}");
}

#[test]
fn control_compensation_with_a_pick_is_refused_by_name() {
    let pick = Some(Point2::new(44.0, 20.0));
    let mut p = params(pick);
    p.links.approach.compensation = Compensation::Control;
    let comp = refusal(&p);
    assert!(comp.contains("compensation"), "{comp}");
}

/// Does a feed move at level `z` pass over the point `w`?
fn cut_at(t: &Toolpath, z: f64, w: Point2) -> bool {
    t.moves.iter().any(|m| {
        m.kind == MoveKind::Feed && m.from.z == z && m.to.z == z && {
            let (a, b) = (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y));
            let cross = (b.x - a.x) * (w.y - a.y) - (b.y - a.y) * (w.x - a.x);
            let dot = (w.x - a.x) * (b.x - a.x) + (w.y - a.y) * (b.y - a.y);
            let ll = (b.x - a.x).powi(2) + (b.y - a.y).powi(2);
            cross.abs() < 1e-6 && dot >= -1e-9 && dot <= ll + 1e-9
        }
    })
}

fn tabbed(pick: Option<Point2>) -> ContourParams {
    // The -4 level stands below the 1 mm tab top; -2 stands above it.
    ContourParams { tab_count: 2, tab_width: 4.0, tab_height: 1.0, ..params(pick) }
}

#[test]
fn a_picked_start_with_tabs_enters_every_level_at_its_projection() {
    // #1473: the tab windows are measured from the pick, so the tabbed
    // level's first window begins at the pick's foot as well.
    let t = contour(&square(), &tabbed(Some(Point2::new(44.0, 20.0)))).unwrap();
    let starts = ring_starts(&t);
    assert!(starts.len() >= 2, "every level, the tabbed one too: {starts:?} {:?}", t.notes);
    for s in &starts {
        assert!((s.x - 43.0).abs() < 1e-9 && (s.y - 20.0).abs() < 1e-9, "starts at the pick's foot: {s:?}");
    }
    // Climb walks the outside ring clockwise: down the right wall from
    // the pick. The last tab ends on the seam, so the 4 mm just above
    // the pick stand uncut at the tabbed level and are cut above it.
    assert!(cut_at(&t, -4.0, Point2::new(43.0, 18.0)), "the first window cuts on from the pick");
    assert!(!cut_at(&t, -4.0, Point2::new(43.0, 22.0)), "the tab before the pick stands");
    assert!(cut_at(&t, -2.0, Point2::new(43.0, 22.0)), "the untabbed level cuts the whole ring");
}

#[test]
fn a_picked_start_with_tabs_refuses_a_linear_lead_in() {
    let mut p = tabbed(Some(Point2::new(44.0, 20.0)));
    p.lead = Lead::Linear { length: 5.0 };
    p.links.approach.lead_out = Some(Lead::None);
    let why = refusal(&p);
    assert!(why.contains("start point (44.000, 20.000)") && why.contains("tabs") && why.contains("lead-in"), "{why}");
}

#[test]
fn a_picked_start_with_tabs_refuses_an_independent_linear_lead_out() {
    let mut p = tabbed(Some(Point2::new(44.0, 20.0)));
    p.links.approach.lead_out = Some(Lead::Linear { length: 5.0 });
    let why = refusal(&p);
    assert!(why.contains("start point (44.000, 20.000)") && why.contains("tabs") && why.contains("lead-out"), "{why}");
}

#[test]
fn a_picked_start_with_tabs_refuses_arc_and_tangential_leads() {
    for shape in [Lead::Arc { radius: 2.0 }, Lead::Tangential { radius: 2.0 }] {
        for incoming in [true, false] {
            let mut p = tabbed(Some(Point2::new(44.0, 20.0)));
            if incoming {
                p.lead = shape;
                p.links.approach.lead_out = Some(Lead::None);
            } else {
                p.links.approach.lead_out = Some(shape);
            }
            let why = refusal(&p);
            let name = if incoming { "lead-in" } else { "lead-out" };
            assert!(why.contains("start point") && why.contains("tabs") && why.contains(name), "{why}");
        }
    }
}

#[test]
fn a_picked_start_with_tabs_refuses_extensions_even_without_leads() {
    for lead in [Lead::None, Lead::Perpendicular { length: 5.0 }] {
        for incoming in [true, false] {
            let mut p = tabbed(Some(Point2::new(44.0, 20.0)));
            p.lead = lead;
            if incoming {
                p.links.approach.ext_in = 5.0;
            } else {
                p.links.approach.ext_out = 5.0;
            }
            let why = refusal(&p);
            assert!(why.contains("start point") && why.contains("tabs") && why.contains("extension"), "{why}");
        }
    }
}

#[test]
fn a_picked_start_with_tabs_without_leads_keeps_the_tab() {
    let mut p = tabbed(Some(Point2::new(44.0, 20.0)));
    p.lead = Lead::None;
    let t = contour(&square(), &p).unwrap();
    assert!(!cut_at(&t, -4.0, Point2::new(43.0, 22.0)), "the tab before the pick stands");
}

#[test]
fn tabs_without_a_pick_keep_their_automatic_windows() {
    let base = contour(&square(), &ContourParams { links: Links::PLANAR, ..tabbed(None) }).unwrap();
    let unset = contour(&square(), &tabbed(None)).unwrap();
    assert_eq!(base.moves, unset.moves, "an unset start point leaves the tab windows as they were");
    let picked = contour(&square(), &tabbed(Some(Point2::new(44.0, 20.0)))).unwrap();
    assert_ne!(base.moves, picked.moves, "the pick moved the tab windows");
}

#[test]
fn an_open_chain_with_a_pick_is_refused_by_name() {
    let run = [Point2::new(0.0, 0.0), Point2::new(20.0, 0.0)];
    let why = match contour_along_chain(&run, &params(Some(Point2::new(10.0, 0.0)))) {
        Ok(_) => panic!("expected a refusal"),
        Err(e) => e.to_string(),
    };
    assert!(why.contains("open chain"), "{why}");
}

fn with_exit(start: Option<Point2>, exit: Point2) -> ContourParams {
    let mut p = params(start);
    p.links.approach.exit_point = Some(exit);
    p
}

/// Per cut level, the last point the cutter stands on the cutter-centre
/// ring before it leaves (the start of the lead-out).
fn ring_ends(t: &Toolpath) -> Vec<(f64, Point2)> {
    let mut out: Vec<(f64, Point2)> = Vec::new();
    for m in &t.moves {
        if m.kind != MoveKind::Feed || m.from.z != m.to.z {
            continue;
        }
        let q = Point2::new(m.to.x, m.to.y);
        if (to_square(q) - 3.0).abs() < 1e-6 {
            match out.iter_mut().find(|(z, _)| *z == m.to.z) {
                Some(e) => e.1 = q,
                None => out.push((m.to.z, q)),
            }
        }
    }
    out
}

#[test]
fn a_picked_exit_cuts_the_whole_ring_then_leaves_at_its_projection() {
    // Start on the right wall, leave on the top one (a click 1 mm out).
    let t = contour(&square(), &with_exit(Some(Point2::new(44.0, 20.0)), Point2::new(20.0, 44.0))).unwrap();
    let ends = ring_ends(&t);
    assert!(ends.len() >= 2, "a multi-level ladder: {ends:?} {:?}", t.notes);
    for (z, e) in &ends {
        assert!((e.x - 20.0).abs() < 1e-9 && (e.y - 43.0).abs() < 1e-9, "leaves at the exit's foot: {e:?}");
        // The whole ring is cut before it: every wall's mid-run is passed at this level.
        for w in [Point2::new(43.0, 20.0), Point2::new(20.0, -3.0), Point2::new(-3.0, 20.0), Point2::new(20.0, 43.0)] {
            let passed = t.moves.iter().any(|m| {
                m.kind == MoveKind::Feed && m.to.z == *z && {
                    let (a, b) = (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y));
                    // w on the chord a-b
                    let cross = (b.x - a.x) * (w.y - a.y) - (b.y - a.y) * (w.x - a.x);
                    let dot = (w.x - a.x) * (b.x - a.x) + (w.y - a.y) * (b.y - a.y);
                    let ll = (b.x - a.x).powi(2) + (b.y - a.y).powi(2);
                    cross.abs() < 1e-6 && dot >= -1e-9 && dot <= ll + 1e-9
                }
            });
            assert!(passed, "level {z} passes {w:?}");
        }
        // The perpendicular lead-out leaves square to the top wall, into the air.
        let off = t.moves.iter().any(|m| {
            m.kind == MoveKind::Feed && m.to.z == *z && m.from.x == e.x && m.from.y == e.y && (m.to.x - 20.0).abs() < 1e-9 && m.to.y > 43.0
        });
        assert!(off, "the lead-out leaves at the exit, level {z}");
    }
}

#[test]
fn an_exit_on_the_seam_is_the_path_without_one() {
    let start = Some(Point2::new(44.0, 20.0));
    let base = contour(&square(), &params(start)).unwrap();
    let same = contour(&square(), &with_exit(start, Point2::new(44.0, 20.0))).unwrap();
    assert_eq!(base.moves, same.moves, "an exit on the start's own foot adds nothing");
}

#[test]
fn exit_picks_the_law_cannot_hold_are_refused_by_name() {
    let q = Point2::new(20.0, 44.0);
    let far = refusal(&with_exit(None, Point2::new(80.0, 20.0)));
    assert!(far.contains("exit point (80.000, 20.000)") && far.contains("farther than"), "{far}");
    let tabs = refusal(&ContourParams { tab_count: 2, tab_width: 4.0, tab_height: 1.0, ..with_exit(None, q) });
    assert!(tabs.contains("exit point") && tabs.contains("with tabs"), "{tabs}");
    let mut p = with_exit(None, q);
    p.links.approach.compensation = Compensation::Control;
    let comp = refusal(&p);
    assert!(comp.contains("exit point") && comp.contains("compensation"), "{comp}");
    let mut p = with_exit(None, q);
    p.links.level = camcraft_motion::links::Link::KeepDown;
    let keep = refusal(&p);
    assert!(keep.contains("exit point") && keep.contains("keep-down"), "{keep}");
    let mut p = with_exit(None, q);
    p.links.approach.overlap = 2.0;
    let ov = refusal(&p);
    assert!(ov.contains("exit point") && ov.contains("overlap"), "{ov}");
    let nan = refusal(&with_exit(None, Point2::new(f64::NAN, 0.0)));
    assert!(nan.contains("exit point"), "{nan}");
    let run = [Point2::new(0.0, 0.0), Point2::new(20.0, 0.0)];
    let chain = match contour_along_chain(&run, &with_exit(None, Point2::new(10.0, 0.0))) {
        Ok(_) => panic!("expected a refusal"),
        Err(e) => e.to_string(),
    };
    assert!(chain.contains("exit point") && chain.contains("open chain"), "{chain}");
}

/// A 40 x 6.4 bore cut inside with a 6 mm tool: the cutter-centre ring
/// is 0.4 mm wide, so a 20 mm perpendicular lead into the bore does not
/// fit even halved four times (1.25 mm).
fn narrow(lead: Lead) -> (Region, ContourParams) {
    let pts = [(0.0, 0.0), (40.0, 0.0), (40.0, 6.4), (0.0, 6.4)];
    let r = Region::new(vec![Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap()]).unwrap();
    (r, ContourParams { side: Side::Inside, lead, ..params(None) })
}

#[test]
fn a_picked_start_no_lead_in_fits_at_is_refused_not_moved() {
    let (r, mut p) = narrow(Lead::Perpendicular { length: 20.0 });
    // Control: the automatic seam cuts (it moves or drops the lead, named).
    assert!(contour(&r, &p).is_ok(), "the automatic seam still cuts");
    p.links.approach.start_point = Some(Point2::new(20.0, 3.0));
    let why = match contour(&r, &p) {
        Ok(t) => panic!("a pick no lead-in fits at is refused: {:?}", t.notes),
        Err(e) => e.to_string(),
    };
    assert!(why.contains("start point (20.000, 3.000)") && why.contains("no lead-in fits"), "{why}");
}

#[test]
fn a_picked_exit_no_lead_out_fits_at_is_refused_not_moved() {
    let (r, mut p) = narrow(Lead::None);
    p.links.approach.lead_out = Some(Lead::Perpendicular { length: 20.0 });
    p.links.approach.exit_point = Some(Point2::new(20.0, 3.0));
    let why = match contour(&r, &p) {
        Ok(t) => panic!("an exit no lead-out fits at is refused: {:?}", t.notes),
        Err(e) => e.to_string(),
    };
    assert!(why.contains("exit point (20.000, 3.000)") && why.contains("no lead-out fits"), "{why}");
}

fn plate() -> Region {
    let pts = [(-30.0, -20.0), (30.0, -20.0), (30.0, 20.0), (-30.0, 20.0)];
    Region::new(vec![Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap()]).unwrap()
}

fn assert_no_crossings_or_gouges(region: &Region, p: &ContourParams) -> Toolpath {
    let path = contour(region, p).unwrap();
    let pairs = crossed_pairs(&path, 0.01);
    assert!(pairs.is_empty(), "expected no XY feed crossings, got {pairs:?}");
    let tool = ToolGeom { radius: 3.0, nose_radius: 0.0, flute_len: 20.0, shank_radius: 3.0, shank_len: 10.0, holder_radius: 15.0, holder_len: 30.0 };
    let cx = VerifyContext { region, tool: &tool, mode: Mode::Boundary, stock_top_z: p.top_z, clearance_z: p.clearance_z, gouge_tol: 0.01, z_tol: 0.05, ..VerifyContext::default() };
    let bad: Vec<Warning> = verify(&path, &cx).into_iter().filter(|w| matches!(w, Warning::Crossing { .. } | Warning::Gouge { .. })).collect();
    assert!(bad.is_empty(), "expected no Crossing/Gouge warnings, got {bad:?}");
    path
}

#[test]
fn an_exit_past_a_rounded_corner_retraces_the_corner_arc_with_no_crossing() {
    let reg = plate();
    let p = ContourParams { top_z: 10.0, bottom_z: 7.0, stepdown: 3.0, clearance_z: 15.0, retract_z: 12.0, lead: Lead::Linear { length: 8.0 }, ..with_exit(Some(Point2::new(30.0, 0.0)), Point2::new(0.0, -20.0)) };
    let t = assert_no_crossings_or_gouges(&reg, &p);
    let corner_arcs: Vec<_> = t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.arc.is_some_and(|a| (a.center.x - 30.0).abs() < 1e-3 && (a.center.y + 20.0).abs() < 1e-3)).collect();
    let mut zs: Vec<f64> = corner_arcs.iter().map(|m| m.to.z).collect();
    zs.dedup();
    assert!(!zs.is_empty() && corner_arcs.len() == 2 * zs.len(), "each level cuts the (30,-20) corner once per lap as one arc: {corner_arcs:?}");
    let last_feed = t.moves.iter().rfind(|m| m.kind == MoveKind::Feed).unwrap();
    assert!((last_feed.from.x - 0.0).abs() < 1e-6 && (last_feed.from.y + 23.0).abs() < 1e-6, "lead-out leaves at exit projection (0,-23): {last_feed:?}");
}

#[test]
fn an_exit_inside_a_rounded_corner_arc_splits_both_laps_with_no_crossing() {
    let reg = plate();
    let p = ContourParams { top_z: 10.0, bottom_z: 7.0, stepdown: 3.0, clearance_z: 15.0, retract_z: 12.0, ..with_exit(Some(Point2::new(30.0, 0.0)), Point2::new(32.5, -21.5)) };
    let t = assert_no_crossings_or_gouges(&reg, &p);
    let last_feed = t.moves.iter().rfind(|m| m.kind == MoveKind::Feed).unwrap();
    let r = (last_feed.from.x - 30.0).hypot(last_feed.from.y + 20.0);
    assert!((r - 3.0).abs() < 1e-4, "lead-out starts on the r=3 corner arc (r={r}): {last_feed:?}");

    // Also a full circle (tessellated 96-gon, fitted into half-turn arcs) with an interior exit angle.
    let circle_pts: Vec<Point2> = (0..96).map(|i| {
        let a = std::f64::consts::TAU * (i as f64) / 96.0;
        Point2::new(20.0 * a.cos(), 20.0 * a.sin())
    }).collect();
    let disk = Region::new(vec![Loop::new(circle_pts).unwrap()]).unwrap();
    assert_no_crossings_or_gouges(&disk, &with_exit(Some(Point2::new(20.0, 0.0)), Point2::new(13.0, -15.0)));
}

#[test]
fn an_exit_on_an_oblique_wall_and_across_a_blended_corner_has_no_crossing() {
    let oblique_pts = [(30.0, 0.0), (10.0, 35.0), (-30.0, 15.0), (-10.0, -25.0)];
    let poly = Region::new(vec![Loop::new(oblique_pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap()]).unwrap();
    assert_no_crossings_or_gouges(&poly, &with_exit(Some(Point2::new(20.0, 18.0)), Point2::new(10.0, -13.0)));
    assert_no_crossings_or_gouges(&square(), &ContourParams { blend_turns: true, ..with_exit(Some(Point2::new(44.0, 20.0)), Point2::new(20.0, -4.0)) });
}
