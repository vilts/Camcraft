//! A user-picked start point on a closed 2D contour under control or
//! wear compensation (#1473 item 4): the seam is the compensation law's
//! (G41/G42 engage and G40 cancel on one straight run), so a pick is
//! taken only where that law holds - the projection mid-run on a
//! straight cut - and every level's span engages there. A pick on a
//! corner arc or a vertex is refused by name, never moved to the
//! automatic mid-run seam.
//!
//! Gate map (M4.4): covers camcraft_motion::machining2d (contour,
//! contour_start, contour_comp), camcraft_motion::cl (CompSpan).

use camcraft_motion::cl::Toolpath;
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::{Approach, Compensation, Links};
use camcraft_motion::machining2d::{contour, ContourParams, Lead, Side};
use camcraft_motion::path_picks::{PathPick, PathPicks};

fn square() -> Region {
    let pts = [(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)];
    Region::new(vec![Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap()]).unwrap()
}

/// A bore: the cutter-centre ring of an inside contour keeps sharp corners.
fn bore_params(pick: Point2, comp: Compensation) -> ContourParams {
    let mut p = params(pick, comp);
    p.side = Side::Inside;
    p.lead = Lead::Perpendicular { length: 4.0 };
    p
}

fn params(pick: Point2, comp: Compensation) -> ContourParams {
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
        blend_radius: None,
        links: Links {
            approach: Approach { compensation: comp, ..Approach::DEFAULT },
            ..Links::PLANAR
        },
        picks: PathPicks { start: vec![PathPick::xy(pick)], exit: Vec::new() },
    }
}

/// Where each span engages: the end of its lead-in move (the centre on
/// the ring) and the end of its programmed lead-in (on the part contour).
fn engages(t: &Toolpath) -> Vec<(Point2, Point2)> {
    t.comps
        .iter()
        .map(|c| {
            let m = &t.moves[c.first];
            let g = &c.programmed[0];
            (Point2::new(m.to.x, m.to.y), Point2::new(g.to.x, g.to.y))
        })
        .collect()
}

fn near(a: Point2, b: Point2) -> bool {
    (a.x - b.x).hypot(a.y - b.y) < 1e-6
}

fn refusal(r: &Region, p: &ContourParams) -> String {
    match contour(r, p) {
        Ok(t) => panic!("expected a refusal, got {} moves with {} spans", t.moves.len(), t.comps.len()),
        Err(e) => e.to_string(),
    }
}

#[test]
fn a_picked_start_under_control_compensation_engages_every_level_at_its_projection() {
    let t = contour(&square(), &params(Point2::new(44.0, 12.0), Compensation::Control)).expect("a mid-run pick holds the comp law");
    let e = engages(&t);
    assert_eq!(e.len(), 2, "one span per level: {:?}", t.notes);
    for (centre, part) in e {
        assert!(near(centre, Point2::new(43.0, 12.0)), "centre engages at {centre:?}");
        assert!(near(part, Point2::new(40.0, 12.0)), "the part contour engages at {part:?}");
    }
}

#[test]
fn a_picked_start_under_wear_compensation_engages_every_level_at_its_projection() {
    let t = contour(&square(), &params(Point2::new(12.0, -5.0), Compensation::Wear)).expect("a mid-run pick holds the comp law");
    let e = engages(&t);
    assert_eq!(e.len(), 2, "one span per level: {:?}", t.notes);
    assert!(t.comps.iter().all(|c| c.wear));
    for (centre, _) in e {
        assert!(near(centre, Point2::new(12.0, -3.0)), "centre engages at {centre:?}");
    }
}

#[test]
fn a_compensated_pick_on_a_corner_arc_is_refused_by_name_not_moved() {
    for comp in [Compensation::Control, Compensation::Wear] {
        let why = refusal(&square(), &params(Point2::new(43.0, 43.0), comp));
        assert!(why.contains("start point (43.000, 43.000)"), "{why}");
        assert!(why.contains("straight run"), "{why}");
        assert!(why.contains(&format!("{} compensation", comp.label())), "{why}");
    }
}

#[test]
fn a_compensated_pick_on_a_sharp_corner_is_refused_by_name_not_moved() {
    for comp in [Compensation::Control, Compensation::Wear] {
        let why = refusal(&square(), &bore_params(Point2::new(37.0, 37.0), comp));
        assert!(why.contains("start point (37.000, 37.000)"), "{why}");
        assert!(why.contains("straight run"), "{why}");
    }
}

#[test]
fn a_compensated_pick_mid_run_in_a_bore_is_taken() {
    let t = contour(&square(), &bore_params(Point2::new(20.0, 3.0), Compensation::Control)).expect("a mid-run pick holds the comp law");
    for (centre, part) in engages(&t) {
        assert!(near(centre, Point2::new(20.0, 3.0)), "centre engages at {centre:?}");
        assert!(near(part, Point2::new(20.0, 0.0)), "the part contour engages at {part:?}");
    }
}
