//! #1759 arc turns with a radius on Face, 2D Contour and Slot. Every arc
//! the option adds is ONE arc tangent at both ends with a radius no
//! larger than the one asked; no arc cuts past the wall (Face: no center
//! inside the sharp overrun line; Contour: no center closer to the part
//! than the sharp path; Slot: no center outside the cutter-center
//! stadium), sampled on the true arcs; no radius is the arc every earlier
//! file turns on; the door refuses a radius no control can cut, by name;
//! with the option off the radius changes nothing.
//!
//! Gate map (#1759): covers camcraft_motion::machining2d (face, blend,
//! contour, tab_bridge, slots, slot_turns, fillet_tally).

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{
    contour, face, slot, ContourParams, FaceParams, GenError, Lead, Side, SlotParams,
};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let pts = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    Region::new(vec![Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap()]).unwrap()
}

#[path = "common/arc_measure.rs"]
mod arc_measure;
use arc_measure::{added_tangent_radii, cut_points, samples, seg_dist, to_wall};

fn refused(r: Result<Toolpath, GenError>, what: &str) {
    match r {
        Err(GenError::Invalid(why)) => assert!(why.contains("corner arc radius"), "{what}: {why}"),
        other => panic!("{what} must be refused, got {:?}", other.map(|t| t.moves.len())),
    }
}

const BAD_RADII: [f64; 5] = [0.0, -1.0, 0.001, f64::NAN, f64::INFINITY];

// ---- Face ------------------------------------------------------------------

const FACE_R: f64 = 5.0;

fn fp(stepover: f64, blend: bool, radius: Option<f64>) -> FaceParams {
    FaceParams {
        holes: Default::default(),
        tool_diameter: 2.0 * FACE_R,
        stepover,
        clearance_z: 5.0,
        cut_z: -2.0,
        top_z: None,
        stepdown: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        blend_turns: blend,
        blend_radius: radius,
        stock_top: None,
    }
}

#[test]
fn face_row_ends_turn_on_tangent_arcs_of_the_given_radius() {
    // 60x50 footprint, 8 mm rows: 5, 13, .., 45 - every stride 8 mm.
    let region = rect(0.0, 0.0, 60.0, 50.0);
    let sharp = face(&region, &fp(8.0, false, None)).unwrap();
    let blended = face(&region, &fp(8.0, true, Some(2.0))).unwrap();
    assert!(blended.is_chained() && blended.all_finite());
    let radii = added_tangent_radii(&blended, &sharp, "face");
    // Five turns, each two quarter arcs of the radius.
    assert_eq!(radii.len(), 10, "{radii:?}");
    assert!(radii.iter().all(|r| (*r - 2.0).abs() < 1e-6), "{radii:?}");
    // The rows are the sharp rows: the same row feeds, end to end.
    let rows = |t: &Toolpath| -> Vec<(f64, f64, f64)> {
        t.moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed && m.arc.is_none() && m.from.y == m.to.y)
            .map(|m| (m.from.x, m.to.x, m.to.y))
            .collect()
    };
    assert_eq!(rows(&blended), rows(&sharp));
}

#[test]
fn no_face_arc_turn_cuts_past_the_overrun_line() {
    let region = rect(0.0, 0.0, 60.0, 50.0);
    for radius in [Some(0.5), Some(2.0), Some(4.0), Some(FACE_R), None] {
        let t = face(&region, &fp(8.0, true, radius)).unwrap();
        let rho = radius.unwrap_or(4.0).min(4.0);
        for m in t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.from.y != m.to.y) {
            for q in samples(m) {
                // A turn stands outside the sharp overrun line (one tool
                // radius off the footprint), by no more than its radius.
                let out = (-FACE_R - q.x).max(q.x - (60.0 + FACE_R));
                assert!(out >= -1e-6 && out <= rho + 1e-6, "r={radius:?}: turn point ({}, {}) stands {out} mm off the overrun line", q.x, q.y);
            }
        }
    }
}

#[test]
fn a_face_radius_past_half_the_row_spacing_turns_on_the_semicircle() {
    let region = rect(0.0, 0.0, 60.0, 50.0);
    let none = face(&region, &fp(8.0, true, None)).unwrap();
    for radius in [4.0, 4.5, FACE_R] {
        let t = face(&region, &fp(8.0, true, Some(radius))).unwrap();
        assert_eq!(none.moves, t.moves, "r={radius}");
        assert_eq!(none.notes, t.notes, "r={radius}");
    }
}

#[test]
fn a_face_turn_never_exceeds_the_radius_when_the_straight_step_is_short() {
    // Review of #2831: half the spacing just past the radius once gave
    // the semicircle (0.0125 mm for a 0.01 mm request).
    for (spacing, rho) in [(0.025, 0.01), (0.028, 0.01), (8.004, 4.0), (8.009, 4.0)] {
        let region = rect(0.0, 0.0, 60.0, 2.0 * FACE_R + spacing);
        let t = face(&region, &fp(spacing, true, Some(rho))).unwrap();
        assert!(t.is_chained() && t.all_finite());
        let radii: Vec<f64> = t
            .moves
            .iter()
            .filter_map(|m| m.arc.map(|a| (m.from.x - a.center.x).hypot(m.from.y - a.center.y)))
            .collect();
        assert!(radii.iter().all(|r| *r <= rho + 1e-6), "spacing {spacing}, requested {rho} mm; actual radii: {radii:?}");
    }
}

#[test]
fn a_face_radius_rounds_rows_spaced_wider_than_the_tool() {
    // 12 mm rows under a 10 mm tool: the semicircle would reach 6 mm
    // past the overrun and is refused; a 3 mm corner arc reaches 3 mm.
    let region = rect(0.0, 0.0, 60.0, 58.0);
    let none = face(&region, &fp(12.0, true, None)).unwrap();
    assert!(none.moves.iter().all(|m| m.arc.is_none()));
    assert!(none.notes.iter().any(|n| n.contains("wider than the tool diameter")), "{:?}", none.notes);
    let t = face(&region, &fp(12.0, true, Some(3.0))).unwrap();
    let radii = added_tangent_radii(&t, &none, "wide face");
    assert_eq!(radii.len(), 8, "{radii:?}");
    assert!(radii.iter().all(|r| (*r - 3.0).abs() < 1e-6), "{radii:?}");
    assert!(!t.notes.iter().any(|n| n.contains("kept sharp")), "{:?}", t.notes);
}

#[test]
fn the_face_door_refuses_a_radius_no_control_can_cut_and_reads_none_when_off() {
    let region = rect(0.0, 0.0, 60.0, 50.0);
    for radius in BAD_RADII.into_iter().chain([FACE_R + 0.01]) {
        refused(face(&region, &fp(8.0, true, Some(radius))), &format!("face radius {radius}"));
    }
    let plain = face(&region, &fp(8.0, false, None)).unwrap();
    for radius in [Some(1.0), Some(f64::NAN)] {
        let t = face(&region, &fp(8.0, false, radius)).unwrap();
        assert_eq!(plain.moves, t.moves, "r={radius:?}");
        assert_eq!(plain.notes, t.notes, "r={radius:?}");
    }
}

// ---- 2D Contour ------------------------------------------------------------

const CONTOUR_R: f64 = 3.0;

fn cp(side: Side, blend: bool, radius: Option<f64>) -> ContourParams {
    ContourParams {
        tool_diameter: 2.0 * CONTOUR_R,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -2.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: blend,
        blend_radius: radius,
        links: camcraft_motion::links::Links::PLANAR,
        picks: Default::default(),
    }
}

/// A 40x30 pocket wall: its inside contour turns toward the air (the
/// pocket) at each of the four corners.
fn bore() -> Region {
    rect(0.0, 0.0, 40.0, 30.0)
}

#[test]
fn contour_corners_turn_on_tangent_arcs_of_the_given_radius() {
    let region = bore();
    let sharp = contour(&region, &cp(Side::Inside, false, None)).unwrap();
    let blended = contour(&region, &cp(Side::Inside, true, Some(1.0))).unwrap();
    assert!(blended.is_chained() && blended.all_finite());
    let radii = added_tangent_radii(&blended, &sharp, "contour");
    assert!(!radii.is_empty(), "no corner turned on an arc: {:?}", blended.notes);
    assert!(radii.iter().all(|r| (*r - 1.0).abs() < 1e-6), "{radii:?}");
    // A smaller radius leaves less corner rest, and says so.
    let full = contour(&region, &cp(Side::Inside, true, None)).unwrap();
    let rest = |t: &Toolpath| -> f64 {
        let n = t.notes.iter().find(|n| n.contains("rounded")).expect("a rounding note");
        n.split("up to ").nth(1).and_then(|s| s.split(' ').next()).and_then(|v| v.parse().ok()).expect("number")
    };
    assert!(rest(&blended) > 0.0 && rest(&blended) < rest(&full), "{} / {}", rest(&blended), rest(&full));
}

#[test]
fn no_contour_arc_turn_cuts_past_the_wall() {
    let region = bore();
    let outside = rect(0.0, 0.0, 40.0, 30.0);
    for (side, wall) in [(Side::Inside, &region), (Side::Outside, &outside)] {
        let sharp = contour(wall, &cp(side, false, None)).unwrap();
        let d_sharp = cut_points(&sharp, -2.0).into_iter().map(|q| to_wall(wall, q)).fold(f64::INFINITY, f64::min);
        for radius in [Some(0.5), Some(2.0), None] {
            let t = contour(wall, &cp(side, true, radius)).unwrap();
            let d = cut_points(&t, -2.0).into_iter().map(|q| to_wall(wall, q)).fold(f64::INFINITY, f64::min);
            assert!(
                d_sharp >= CONTOUR_R - 0.01 && d >= d_sharp - 1e-6,
                "{side:?} r={radius:?}: center {d} mm from the wall (sharp {d_sharp})"
            );
        }
    }
    // An outside contour of a convex part turns toward the part at each
    // corner: no arc there, and the notes name the reason.
    let t = contour(&outside, &cp(Side::Outside, true, Some(1.0))).unwrap();
    let sharp = contour(&outside, &cp(Side::Outside, false, None)).unwrap();
    assert!(added_tangent_radii(&t, &sharp, "outside").is_empty());
}

#[test]
fn no_contour_radius_is_the_tool_radius_and_the_door_refuses_by_name() {
    let region = bore();
    let none = contour(&region, &cp(Side::Inside, true, None)).unwrap();
    let tool = contour(&region, &cp(Side::Inside, true, Some(CONTOUR_R))).unwrap();
    assert_eq!(none.moves, tool.moves);
    assert_eq!(none.notes, tool.notes);
    for radius in BAD_RADII.into_iter().chain([CONTOUR_R + 0.01]) {
        refused(contour(&region, &cp(Side::Inside, true, Some(radius))), &format!("contour radius {radius}"));
    }
    let plain = contour(&region, &cp(Side::Inside, false, None)).unwrap();
    for radius in [Some(1.0), Some(f64::NAN)] {
        let t = contour(&region, &cp(Side::Inside, false, radius)).unwrap();
        assert_eq!(plain.moves, t.moves, "r={radius:?}");
        assert_eq!(plain.notes, t.notes, "r={radius:?}");
    }
}

// ---- Slot ------------------------------------------------------------------

const SLOT_R: f64 = 3.0;
/// The cutter-center stadium's half-width: 20 / 2 - 3.
const SLOT_H: f64 = 7.0;

fn sp(blend: bool, radius: Option<f64>) -> SlotParams {
    SlotParams {
        links: camcraft_motion::links::Links::PLANAR,
        tool_diameter: 2.0 * SLOT_R,
        width: 20.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -2.0,
        stepdown: 2.0,
        stepover: 4.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: blend,
        blend_radius: radius,
    }
}

fn slot_ends() -> (Point2, Point2) {
    (Point2::new(0.0, 0.0), Point2::new(40.0, 0.0))
}

#[test]
fn slot_row_ends_turn_on_tangent_arcs_of_the_given_radius() {
    let (a, b) = slot_ends();
    let sharp = slot(a, b, &sp(false, None)).unwrap();
    // #2906: the only arcs of the sharp pass are the wall lap's, on the caps.
    let on_cap = |m: &Move| m.arc.is_some_and(|c| c.center == a || c.center == b);
    let lap_arcs = |t: &Toolpath| t.moves.iter().filter(|m| on_cap(m)).count();
    assert!(sharp.moves.iter().all(|m| m.arc.is_none() || on_cap(m)));
    assert!(lap_arcs(&sharp) > 0, "the sharp pass has no wall lap");
    let blended = slot(a, b, &sp(true, Some(1.0))).unwrap();
    // The rounded rows keep the wall lap after them, arc for arc.
    assert_eq!(lap_arcs(&blended), lap_arcs(&sharp), "the arc turns lost the wall lap");
    assert!(blended.is_chained() && blended.all_finite());
    let radii = added_tangent_radii(&blended, &sharp, "slot");
    assert!(!radii.is_empty(), "no row end turned on an arc: {:?}", blended.notes);
    // The law may shrink a corner without room, never grow one.
    assert!(radii.iter().all(|r| *r <= 1.0 + 1e-6), "{radii:?}");
    assert!(radii.iter().any(|r| (*r - 1.0).abs() < 1e-6), "no arc at the radius: {radii:?}");
    assert!(blended.notes.iter().any(|n| n.contains("rounded") && n.contains("corner rest")), "{:?}", blended.notes);
    // The pass starts and ends where the sharp pass does.
    let ends = |t: &Toolpath| {
        let f: Vec<&Move> = t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.from.z == -2.0 && m.to.z == -2.0).collect();
        (f[0].from, f[f.len() - 1].to)
    };
    assert_eq!(ends(&blended), ends(&sharp));
}

#[test]
fn no_slot_arc_turn_cuts_past_the_wall() {
    let (a, b) = slot_ends();
    for radius in [Some(0.5), Some(1.0), Some(SLOT_R), None] {
        let t = slot(a, b, &sp(true, radius)).unwrap();
        assert!(t.moves.iter().any(|m| m.arc.is_some()), "r={radius:?}: no arc");
        for q in cut_points(&t, -2.0) {
            let d = seg_dist(q, a, b);
            assert!(d <= SLOT_H + 1e-6, "r={radius:?}: center ({}, {}) stands {d} mm off the slot line, past {SLOT_H}", q.x, q.y);
        }
    }
}

#[test]
fn no_slot_radius_is_the_tool_radius_and_the_door_refuses_by_name() {
    let (a, b) = slot_ends();
    let none = slot(a, b, &sp(true, None)).unwrap();
    let tool = slot(a, b, &sp(true, Some(SLOT_R))).unwrap();
    assert_eq!(none.moves, tool.moves);
    assert_eq!(none.notes, tool.notes);
    for radius in BAD_RADII.into_iter().chain([SLOT_R + 0.01]) {
        refused(slot(a, b, &sp(true, Some(radius))), &format!("slot radius {radius}"));
    }
    let plain = slot(a, b, &sp(false, None)).unwrap();
    for radius in [Some(1.0), Some(f64::NAN)] {
        let t = slot(a, b, &sp(false, radius)).unwrap();
        assert_eq!(plain.moves, t.moves, "r={radius:?}");
        assert_eq!(plain.notes, t.notes, "r={radius:?}");
    }
}

#[test]
fn a_one_row_slot_has_no_corner_and_says_so() {
    let (a, b) = slot_ends();
    let one = SlotParams { width: 2.0 * SLOT_R, ..sp(false, None) };
    let plain = slot(a, b, &one).unwrap();
    let t = slot(a, b, &SlotParams { blend_turns: true, ..one }).unwrap();
    assert_eq!(plain.moves, t.moves);
    assert!(t.notes.iter().any(|n| n.contains("turn blends") && n.contains("one row")), "{:?}", t.notes);
}
