//! Climb means climb (#1555): with a right-hand (M3, clockwise seen
//! from +Z) spindle, climb milling keeps the material on the RIGHT of
//! the tool's travel - an outside profile runs clockwise around the
//! part, an inside profile (a hole or pocket wall) counter-clockwise,
//! and the control's compensation is G41 (tool left of the material).
//! Conventional is the reverse. Provenance: the down-milling convention
//! of the public machining handbooks (Machinery's Handbook, "Milling
//! Practice: climb and conventional milling"; RS-274 / ISO 6983 G41 =
//! cutter left of the programmed contour in the direction of travel).
//!
//! Gate map (M4.4): covers camcraft_motion::machining2d (contour,
//! contour_chain, contour_comp, chamfer, pocket, waterscan, arc_leads,
//! lead_fit, contour_entry), camcraft_motion::cl (CompSpan).

use camcraft_motion::cl::{CompSide, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::{Approach, Compensation, Links};
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::{
    chamfer, contour, contour_along_chain, pocket, ChamferParams, ContourParams, Lead, PocketParams,
    PocketStrategy, Side,
};

fn lp(pts: &[(f64, f64)]) -> Loop {
    let Ok(l) = Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()) else {
        panic!("fixture loop")
    };
    l
}

fn region(loops: Vec<Loop>) -> Region {
    let Ok(r) = Region::new(loops) else { panic!("fixture region") };
    r
}

/// A 40 x 40 part (or bore) centred on the origin.
fn square() -> Region {
    region(vec![lp(&[(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)])])
}

fn disc(r: f64) -> Region {
    let n = 256;
    let pts: Vec<(f64, f64)> = (0..n)
        .map(|k| {
            let a = k as f64 * std::f64::consts::TAU / n as f64;
            (r * a.cos(), r * a.sin())
        })
        .collect();
    region(vec![lp(&pts)])
}

fn cp(side: Side, climb: bool, lead: Lead, comp: Compensation) -> ContourParams {
    ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -2.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb,
        side,
        lead,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        links: Links { approach: Approach { compensation: comp, ..Approach::DEFAULT }, ..Links::PLANAR },
        picks: Default::default(),
    }
}

/// The swept turn of the feeds at `z` about the origin: the sum of
/// `a x b` over every planar feed whose ends both stand at least `rmin`
/// from the origin (the wall ring, not the dive or the leads into it).
/// Positive is counter-clockwise seen from +Z, negative clockwise.
fn turn_at(path: &Toolpath, z: f64, rmin: f64) -> f64 {
    path.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == z && m.to.z == z)
        .filter(|m| m.from.x.hypot(m.from.y) >= rmin && m.to.x.hypot(m.to.y) >= rmin)
        .map(|m| m.from.x * m.to.y - m.from.y * m.to.x)
        .sum()
}

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("generation refused: {e:?}"),
    }
}

#[test]
fn climb_outside_contour_runs_clockwise_and_conventional_counter_clockwise() {
    // Outside the 40 x 40 part the centre ring stands 3 mm off it, so
    // every ring feed is at least 20 mm from the origin.
    let climb = ok(contour(&square(), &cp(Side::Outside, true, Lead::None, Compensation::Computer)));
    let conv = ok(contour(&square(), &cp(Side::Outside, false, Lead::None, Compensation::Computer)));
    let (a, b) = (turn_at(&climb, -2.0, 19.9), turn_at(&conv, -2.0, 19.9));
    assert!(a < 0.0, "climb outside (M3) must run clockwise, swept {a}");
    assert!(b > 0.0, "conventional outside must run counter-clockwise, swept {b}");
}

#[test]
fn climb_inside_contour_runs_counter_clockwise_and_conventional_clockwise() {
    // Inside the 40 x 40 bore the centre ring is the [-17, 17] square.
    let climb = ok(contour(&square(), &cp(Side::Inside, true, Lead::None, Compensation::Computer)));
    let conv = ok(contour(&square(), &cp(Side::Inside, false, Lead::None, Compensation::Computer)));
    let (a, b) = (turn_at(&climb, -2.0, 16.9), turn_at(&conv, -2.0, 16.9));
    assert!(a > 0.0, "climb inside (M3) must run counter-clockwise, swept {a}");
    assert!(b < 0.0, "conventional inside must run clockwise, swept {b}");
}

#[test]
fn climb_hole_wall_contour_runs_counter_clockwise() {
    // A round hole wall (the issue's bore): Ø40, Ø6 tool, ring R17.
    let climb = ok(contour(&disc(20.0), &cp(Side::Inside, true, Lead::None, Compensation::Computer)));
    let a = turn_at(&climb, -2.0, 16.5);
    assert!(a > 0.0, "climb in a hole (M3) must run counter-clockwise, swept {a}");
}

#[test]
fn control_compensation_posts_g41_for_climb_and_g42_for_conventional() {
    let lead = Lead::Perpendicular { length: 5.0 };
    for side in [Side::Outside, Side::Inside] {
        for (climb, want) in [(true, CompSide::Left), (false, CompSide::Right)] {
            let t = ok(contour(&square(), &cp(side, climb, lead, Compensation::Control)));
            assert!(!t.comps.is_empty(), "control compensation records its span");
            assert!(
                t.comps.iter().all(|c| c.side == want),
                "{side:?} climb={climb}: want {want:?} (material right of travel = tool left = G41 for climb)"
            );
            // The centre the span replaces walks the direction the side says.
            let r = if side == Side::Outside { 19.9 } else { 16.9 };
            let turn = turn_at(&t, -2.0, r);
            let cw = turn < 0.0;
            assert_eq!(cw, (side == Side::Outside) == climb, "{side:?} climb={climb}: swept {turn}");
        }
    }
}

/// The lead's air side: every lead point stays out of the part (outside)
/// or inside the bore's centre area (inside), after the flip.
#[test]
fn leads_stay_on_the_air_side_after_the_flip() {
    for lead in [Lead::Arc { radius: 4.0 }, Lead::Linear { length: 4.0 }, Lead::Perpendicular { length: 4.0 }] {
        for climb in [true, false] {
            let out = ok(contour(&square(), &cp(Side::Outside, climb, lead, Compensation::Computer)));
            for m in out.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
                // Tool edge never enters the 40 x 40 part: centre >= 3 mm out.
                let (x, y) = (m.to.x.abs(), m.to.y.abs());
                let gap = if x <= 20.0 { y - 20.0 } else if y <= 20.0 { x - 20.0 } else { (x - 20.0).hypot(y - 20.0) };
                assert!(gap >= 3.0 - 1e-6, "outside {lead:?} climb={climb}: centre {:?} is {gap} mm off the part", m.to);
            }
            let bore = ok(contour(&square(), &cp(Side::Inside, climb, lead, Compensation::Computer)));
            for m in bore.moves.iter().filter(|m| m.kind != MoveKind::Rapid && m.to.z < 0.0) {
                // Tool edge stays inside the bore: centre within [-17, 17].
                assert!(
                    m.to.x.abs() <= 17.0 + 1e-6 && m.to.y.abs() <= 17.0 + 1e-6,
                    "inside {lead:?} climb={climb}: centre {:?} past the wall",
                    m.to
                );
            }
        }
    }
}

/// Arc leads join tangent: each R4 lead arc meets the cut it joins
/// heading the same way (its radius at the join is square to the cut).
#[test]
fn arc_leads_stay_tangent_after_the_flip() {
    let lead = Lead::Arc { radius: 4.0 };
    for side in [Side::Outside, Side::Inside] {
        for climb in [true, false] {
            let t = ok(contour(&square(), &cp(side, climb, lead, Compensation::Computer)));
            let feeds: Vec<_> = t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.to.z == -2.0 && m.from.z == -2.0).collect();
            let mut leads = 0;
            for (i, m) in feeds.iter().enumerate() {
                let Some(arc) = m.arc else { continue };
                if ((m.from.x - arc.center.x).hypot(m.from.y - arc.center.y) - 4.0).abs() > 1e-3 {
                    continue; // a ring corner, not a lead
                }
                leads += 1;
                // The join is the lead-in's end (next cut) or the lead-out's start (previous cut).
                let (at, cut) = match (feeds.get(i + 1), i.checked_sub(1).map(|j| feeds[j])) {
                    (Some(n), _) if n.arc.is_none() && n.from == m.to => (m.to, (n.to.x - n.from.x, n.to.y - n.from.y)),
                    (_, Some(q)) if q.arc.is_none() && q.to == m.from => (m.from, (q.to.x - q.from.x, q.to.y - q.from.y)),
                    _ => continue,
                };
                let (rx, ry) = (at.x - arc.center.x, at.y - arc.center.y);
                let dot = (rx * cut.0 + ry * cut.1) / (rx.hypot(ry) * cut.0.hypot(cut.1));
                assert!(dot.abs() < 1e-6, "{side:?} climb={climb}: lead arc not tangent at the join ({dot})");
            }
            assert!(leads >= 2, "{side:?} climb={climb}: lead-in and lead-out arcs, found {leads}");
        }
    }
}

fn pp(strategy: PocketStrategy, climb: bool) -> PocketParams {
    PocketParams {
        blend_radius: None,
        tool_diameter: 10.0,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: Some(1.0),
        top_z: 0.0,
        bottom_z: -5.0,
        stepdown: 5.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb,
        strategy,
        entry: Entry3D::Helix,
        helix_radius: 2.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links: Links::PLANAR,
    }
}

#[test]
fn pocket_wall_pass_climb_runs_counter_clockwise() {
    // The issue's bore: Ø25 pocket, Ø10 tool - the wall ring is R7.5.
    for strategy in [PocketStrategy::Waterline, PocketStrategy::OffsetRings, PocketStrategy::Adaptive, PocketStrategy::Auto] {
        for climb in [true, false] {
            let t = ok(pocket(&disc(12.5), &pp(strategy, climb)));
            let turn = turn_at(&t, -5.0, 7.4);
            assert_ne!(turn, 0.0, "{strategy:?}: no wall pass found");
            assert_eq!(turn > 0.0, climb, "{strategy:?} climb={climb}: wall pass swept {turn} (climb = CCW in a pocket)");
        }
    }
}

#[test]
fn chamfer_climb_keeps_the_material_right() {
    let p = |climb, inside| ChamferParams {
        width: 1.0,
        angle_deg: 45.0,
        minimum_cone_radius: 0.0,
        clearance_z: 5.0,
        retract_z: Some(2.0),
        top_z: 0.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb,
        inside,
    };
    for inside in [false, true] {
        for climb in [true, false] {
            let q = p(climb, inside);
            let t = ok(chamfer(&square(), &q));
            let turn = turn_at(&t, q.corner_z(), 19.0);
            assert_ne!(turn, 0.0);
            // Outside (a boss edge): climb CW; inside (a bore edge): climb CCW.
            assert_eq!(turn < 0.0, climb != inside, "chamfer inside={inside} climb={climb}: swept {turn}");
        }
    }
}

#[test]
fn open_centerline_run_keeps_its_travel() {
    // An open centerline run has no material side: climb rides it in
    // stored order, conventional reversed - unchanged by #1555.
    let run = [Point2::new(-10.0, 0.0), Point2::new(10.0, 0.0)];
    let climb = ok(contour_along_chain(&run, &cp(Side::Auto, true, Lead::None, Compensation::Computer)));
    let first = climb.moves.iter().find(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z).map(|m| m.to.x - m.from.x);
    assert!(first.is_some_and(|dx| dx > 0.0), "climb rides the run in stored order");
}
