//! The plasma profile's laws (#670): kerf side, direction, leads, the
//! small-hole rule, inner-before-outer order, refusals, and the CL
//! records round-tripping through the text dump.

use super::*;
use crate::cl::{text, ContourRole, MoveKind, Toolpath};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).unwrap()
}

fn circle(cx: f64, cy: f64, r: f64) -> Loop {
    Loop::new((0..96).map(|k| {
        let t = std::f64::consts::TAU * f64::from(k) / 96.0;
        Point2::new(cx + r * t.cos(), cy + r * t.sin())
    }).collect()).unwrap()
}

fn params() -> PlasmaParams {
    PlasmaParams {
        kerf: 1.5,
        control_comp: false,
        reversed: false,
        lead_in: 4.0,
        lead_out: 3.0,
        small_hole_dia: SMALL_HOLE_DIA,
        small_hole_feed: SMALL_HOLE_FEED,
        overburn: 2.0,
        thickness: 6.0,
        feed: Some(2500.0),
        clearance_z: 10.0,
        cut_z: 0.0,
        pierce_height: None,
        cut_height: None,
        pierce_delay: None,
        model_excess: 0.0,
    }
}

/// A 100 x 60 plate with a Ø40 hole and a Ø10 hole.
fn plate() -> (Vec<Loop>, Region) {
    let loops = vec![rect(0.0, 0.0, 100.0, 60.0), circle(30.0, 30.0, 20.0), circle(75.0, 30.0, 5.0)];
    let part = Region::new(loops.clone()).unwrap();
    (loops, part)
}

fn lit_points(path: &Toolpath, contour: u32) -> Vec<Point2> {
    let p = path.plasma.as_ref().unwrap();
    let t = p.torches.iter().find(|t| t.contour == contour).unwrap();
    path.moves[t.on..=t.off].iter().map(|m| Point2::new(m.to.x, m.to.y)).collect()
}

fn signed_area(pts: &[Point2]) -> f64 {
    let n = pts.len();
    (0..n).map(|i| pts[i].x * pts[(i + 1) % n].y - pts[(i + 1) % n].x * pts[i].y).sum::<f64>() * 0.5
}

fn cut_plate(loops: &[Loop], part: &Region, p: &PlasmaParams) -> Toolpath {
    plasma_profile(loops, &[], part, p).unwrap()
}

#[test]
fn holes_are_cut_before_the_outline_that_frees_the_part() {
    let (loops, part) = plate();
    let path = cut_plate(&loops, &part, &params());
    let rec = path.plasma.as_ref().unwrap();
    let roles: Vec<ContourRole> = rec.torches.iter().map(|t| t.role).collect();
    assert_eq!(roles, vec![ContourRole::Inner, ContourRole::Inner, ContourRole::Outer]);
    let outer = rec.torches[2].contour;
    assert!(rec.torches[..2].iter().all(|t| t.parent == Some(outer)), "{:?}", rec.torches);
    assert_eq!(rec.torches[2].parent, None);
    assert!(rec.is_sane(&path.moves));
}

#[test]
fn the_kerf_offsets_to_the_waste_side() {
    let (loops, part) = plate();
    let path = cut_plate(&loops, &part, &params());
    let r = 0.75;
    let rec = path.plasma.as_ref().unwrap();
    // The outline: every lit point stands at least r outside the plate.
    for q in lit_points(&path, 2) {
        let (dx, dy) = ((-q.x).max(q.x - 100.0).max(0.0), (-q.y).max(q.y - 60.0).max(0.0));
        assert!(dx.hypot(dy) > r - 0.02, "outline point {q:?} inside the kerf band");
    }
    // The large hole's ride (lead moves aside) stands at 20 - r from its centre.
    let t = rec.torches.iter().find(|t| t.small.is_none() && t.role == ContourRole::Inner).unwrap();
    let ride: Vec<_> = path.moves[t.on + 1..t.off].iter().map(|m| m.to).collect();
    for q in ride {
        let d = (q.x - 30.0).hypot(q.y - 30.0);
        assert!((d - (20.0 - r)).abs() < 0.02, "hole ride at {d}");
    }
}

#[test]
fn outer_contours_run_clockwise_and_holes_counter_clockwise() {
    let (loops, part) = plate();
    let path = cut_plate(&loops, &part, &params());
    assert!(signed_area(&lit_points(&path, 2)) < 0.0, "outline must run CW");
    assert!(signed_area(&lit_points(&path, 0)) > 0.0, "hole must run CCW");
    assert!(path.notes.iter().any(|n| n.contains("outer contours clockwise")));
    let rev = cut_plate(&loops, &part, &PlasmaParams { reversed: true, ..params() });
    assert!(signed_area(&lit_points(&rev, 2)) > 0.0);
    assert!(signed_area(&lit_points(&rev, 0)) < 0.0);
    assert!(rev.notes.iter().any(|n| n.contains("REVERSED")), "{:?}", rev.notes);
    assert!(rev.plasma.as_ref().unwrap().reversed);
}

#[test]
fn arc_leads_enter_and_leave_on_the_waste_side() {
    let (loops, part) = plate();
    let path = cut_plate(&loops, &part, &params());
    let t = path.plasma.as_ref().unwrap().torches[2];
    let (first, last) = (&path.moves[t.on], &path.moves[t.off]);
    assert!(first.arc.is_some() && last.arc.is_some(), "outline leads are arcs");
    // Pierce and lead-out end stand off the plate (outside it).
    for q in [first.from, last.to] {
        assert!(!part.contains(Point2::new(q.x, q.y)), "lead end {q:?} on the part");
    }
    // A quarter arc of length 4 has radius 8/pi.
    let a = first.arc.unwrap();
    let rr = (first.to.x - a.center.x).hypot(first.to.y - a.center.y);
    assert!((rr - 8.0 / std::f64::consts::PI).abs() < 1e-3, "lead radius {rr}");
}

#[test]
fn a_small_hole_leads_in_from_its_centre_with_no_lead_out_at_the_slow_feed() {
    let (loops, part) = plate();
    let path = cut_plate(&loops, &part, &params());
    let rec = path.plasma.as_ref().unwrap();
    let t = rec.torches.iter().find(|t| t.small.is_some()).copied().unwrap();
    assert_eq!(t.small, Some(0.65));
    let first = &path.moves[t.on];
    assert!((first.from.x - 75.0).abs() < 1e-6 && (first.from.y - 30.0).abs() < 1e-6, "pierce at the centre");
    assert!(first.arc.is_none(), "a perpendicular, straight lead-in");
    assert!(path.moves[t.on..=t.off].iter().all(|m| m.feed == Some(2500.0 * 0.65)));
    // No lead-out: the last lit move ends ON the ride (radius 5 - r),
    // past the start by the overburn.
    let end = path.moves[t.off].to;
    assert!(((end.x - 75.0).hypot(end.y - 30.0) - 4.25).abs() < 0.02);
    let start = first.to;
    assert!((end.x - start.x).hypot(end.y - start.y) > 1.0, "the overburn runs past the start");
    // The big hole (Ø40 > 32 and > 1.5 x 6) is not small.
    assert_eq!(rec.torches.iter().filter(|t| t.small.is_some()).count(), 1);
    // The thickness word: a 9 mm plate makes a Ø40 hole... still not
    // small (40 > 13.5); a 30 mm plate does (40 < 45).
    let thick = cut_plate(&loops, &part, &PlasmaParams { thickness: 30.0, ..params() });
    assert_eq!(thick.plasma.unwrap().torches.iter().filter(|t| t.small.is_some()).count(), 2);
}

#[test]
fn an_island_in_a_hole_is_cut_before_the_hole() {
    let loops = vec![rect(0.0, 0.0, 200.0, 200.0), rect(40.0, 40.0, 160.0, 160.0), rect(80.0, 80.0, 120.0, 120.0)];
    let part = Region::new(loops.clone()).unwrap();
    let path = cut_plate(&loops, &part, &params());
    let rec = path.plasma.unwrap();
    let roles: Vec<_> = rec.torches.iter().map(|t| (t.contour, t.role, t.parent)).collect();
    assert_eq!(
        roles,
        vec![(0, ContourRole::Outer, Some(1)), (1, ContourRole::Inner, Some(2)), (2, ContourRole::Outer, None)]
    );
}

#[test]
fn a_hole_picked_alone_is_read_as_a_hole_from_the_part() {
    let (loops, part) = plate();
    let path = cut_plate(&loops[1..2], &part, &params());
    let t = path.plasma.as_ref().unwrap().torches[0];
    assert_eq!(t.role, ContourRole::Inner);
    assert!(signed_area(&lit_points(&path, 0)) > 0.0);
}

#[test]
fn degenerate_input_is_refused_by_name() {
    let (loops, part) = plate();
    let err = |r: Result<Toolpath, crate::machining2d::GenError>| r.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err(plasma_profile(&loops, &[], &part, &PlasmaParams { kerf: 0.0, ..params() })).contains("kerf width"));
    assert!(err(plasma_profile(&loops, &[], &part, &PlasmaParams { kerf: -1.0, ..params() })).contains("kerf width"));
    // A Ø1 hole under a 1.5 kerf.
    let tiny = vec![rect(0.0, 0.0, 50.0, 50.0), circle(25.0, 25.0, 0.5)];
    let tp = Region::new(tiny.clone()).unwrap();
    let e = err(plasma_profile(&tiny, &[], &tp, &params()));
    assert!(e.contains("smaller than the kerf") && e.contains("C0"), "{e}");
    // A bow tie crosses itself.
    let bow = Loop::new(vec![Point2::new(0.0, 0.0), Point2::new(10.0, 10.0), Point2::new(10.0, 0.0), Point2::new(0.0, 10.0)]).unwrap();
    assert!(err(plasma_profile(&[bow], &[], &Region::empty(), &params())).contains("crosses itself"));
    // A collinear ring has no area.
    let flat = Loop::new(vec![Point2::new(0.0, 0.0), Point2::new(5.0, 0.0), Point2::new(10.0, 0.0)]).unwrap();
    assert!(err(plasma_profile(&[flat], &[], &Region::empty(), &params())).contains("no area"));
    // A zero-length open run.
    let run = vec![Point2::new(1.0, 1.0), Point2::new(1.0, 1.0)];
    assert!(err(plasma_profile(&[], &[run], &Region::empty(), &params())).contains("no length"));
    assert!(err(plasma_profile(&[], &[], &part, &params())).contains("no contour"));
    // Non-finite numbers at the door.
    assert!(err(plasma_profile(&loops, &[], &part, &PlasmaParams { feed: Some(f64::NAN), ..params() })).contains("feed"));
    assert!(err(plasma_profile(&loops, &[], &part, &PlasmaParams { small_hole_feed: 1.5, ..params() })).contains("factor"));
}

#[test]
fn control_compensation_carries_g41_g42_spans_with_long_straight_entries() {
    let (loops, part) = plate();
    let p = PlasmaParams { control_comp: true, lead_in: 2.0, lead_out: 2.0, ..params() };
    let path = cut_plate(&loops, &part, &p);
    // The two non-small contours are compensated; the small hole is not.
    assert_eq!(path.comps.len(), 2);
    for c in &path.comps {
        assert!(c.is_sane(&path.moves));
        let e = &c.programmed[0];
        let len = (e.to.x - e.from.x).hypot(e.to.y - e.from.y);
        assert!(e.arc.is_none() && len > 0.75, "engage {len}");
    }
    assert!(path.plasma.as_ref().unwrap().control_comp);
    let short = PlasmaParams { control_comp: true, lead_in: 0.5, ..params() };
    let e = plasma_profile(&loops, &[], &part, &short).err().unwrap().to_string();
    assert!(e.contains("straight lead-in"), "{e}");
}

#[test]
fn open_runs_are_cut_on_their_centre_line_and_nested() {
    let (loops, part) = plate();
    let run = vec![Point2::new(5.0, 70.0), Point2::new(40.0, 70.0)];
    let path = plasma_profile(&loops[..1], &[run], &part, &params()).unwrap();
    let rec = path.plasma.as_ref().unwrap();
    assert!(rec.torches.iter().any(|t| t.role == ContourRole::Open && t.parent.is_none()));
    assert!(path.notes.iter().any(|n| n.contains("centre line")));
    let inside = vec![Point2::new(5.0, 5.0), Point2::new(15.0, 5.0)];
    let path = plasma_profile(&loops[..1], &[inside], &Region::empty(), &params()).unwrap();
    let rec = path.plasma.unwrap();
    assert_eq!((rec.torches[0].role, rec.torches[0].parent), (ContourRole::Open, Some(1)));
}

#[test]
fn the_torch_records_round_trip_through_the_cl_text() {
    let (loops, part) = plate();
    let p = PlasmaParams { feed: None, pierce_height: Some(3.8), cut_height: Some(1.5), pierce_delay: Some(0.5), ..params() };
    let path = cut_plate(&loops, &part, &p);
    let s = text::to_text(&path).unwrap();
    assert!(s.starts_with("; HSMW-CL v5"), "{}", &s[..40]);
    assert!(s.contains("; PLASMA KERF1500 PIERCE3800 CUTH1500 DELAY500 AUTOFEED\n"), "{s}");
    assert!(s.contains("; TORCH ON C0 INNER PARENT2\n") && s.contains("; TORCH OFF C2\n"));
    assert!(s.contains(" SMALL650"));
    let back = text::from_text(&s).unwrap();
    assert_eq!(back.plasma, path.plasma);
    assert_eq!(back.moves.len(), path.moves.len());
    // The lit moves carry no F when the controller feeds.
    assert!(path.moves.iter().filter(|m| m.kind == MoveKind::Feed).all(|m| m.feed.is_none()));
    // A torch never put out is refused, never guessed.
    let cut = s.replace("; TORCH OFF C2\n", "");
    assert!(text::from_text(&cut).is_err());
}

/// #672 (#676 decision 12): a slot or notch narrower than the kerf is
/// bridged by the offset ring - one loop, so the ring law accepts it -
/// and the cover law refuses it by name instead of leaving the feature
/// uncut; the same notch twice the kerf wide cuts, and a keyway under
/// the kerf on a hole is refused the same way.
#[test]
fn a_slot_narrower_than_the_kerf_is_refused_not_bridged() {
    let err = |r: Result<Toolpath, crate::machining2d::GenError>| r.err().map(|e| e.to_string()).unwrap_or_default();
    let q = Point2::new;
    // A 100 x 60 plate with a notch `w` wide and 10 deep in its top edge.
    let notched = |w: f64| {
        let (l, r) = (50.0 - w * 0.5, 50.0 + w * 0.5);
        Loop::new(vec![q(0.0, 0.0), q(100.0, 0.0), q(100.0, 60.0), q(r, 60.0), q(r, 50.0), q(l, 50.0), q(l, 60.0), q(0.0, 60.0)]).unwrap()
    };
    let narrow = notched(1.0);
    let part = Region::new(vec![narrow.clone()]).unwrap();
    let e = err(plasma_profile(&[narrow], &[], &part, &params()));
    assert!(e.contains("contour C0") && e.contains("bridged") && e.contains("narrower than the kerf"), "{e}");
    assert!(e.contains("Y5") || e.contains("Y6"), "located in the notch: {e}");
    assert!(e.contains("about 1.000 mm across") && e.contains("(1.500 mm)"), "names the gap against the kerf: {e}");
    // 3 mm wide (twice the 1.5 kerf): the torch enters it and every edge is reached.
    let wide = notched(3.0);
    let part = Region::new(vec![wide.clone()]).unwrap();
    let path = cut_plate(&[wide], &part, &params());
    assert!(path.plasma.is_some_and(|r| r.torches.len() == 1));
    // A 20 x 20 hole with a 1 x 5 keyway: the inward offset drops the
    // keyway, the cover law refuses the hole by name.
    let keyed = Loop::new(vec![
        q(30.0, 20.0), q(50.0, 20.0), q(50.0, 40.0), q(40.5, 40.0), q(40.5, 45.0), q(39.5, 45.0), q(39.5, 40.0), q(30.0, 40.0),
    ]).unwrap();
    let loops = vec![rect(0.0, 0.0, 100.0, 60.0), keyed];
    let part = Region::new(loops.clone()).unwrap();
    let e = err(plasma_profile(&loops, &[], &part, &params()));
    assert!(e.contains("contour C0") && e.contains("bridged"), "{e}");
    assert!(e.contains("about 1.000 mm across"), "names the keyway's gap: {e}");
}

/// #791 review: every inside corner leaves a kerf-radius tip the torch
/// cannot reach - normal for plasma - so a sharp V-notch generates with
/// its tip left, whatever its opening; the cover law judges only the
/// material beyond that tip, which is what a bridged slot produces.
#[test]
fn a_sharp_inside_corner_generates_with_its_tip_left_not_refused() {
    let q = Point2::new;
    // A 100 x 60 plate with a V-notch 20 mm across its top edge, opening `deg`.
    let vee = |deg: f64| {
        let depth = 10.0 / (deg.to_radians() * 0.5).tan();
        Loop::new(vec![q(0.0, 0.0), q(100.0, 0.0), q(100.0, 60.0), q(60.0, 60.0), q(50.0, 60.0 - depth), q(40.0, 60.0), q(0.0, 60.0)])
            .unwrap()
    };
    for deg in [30.0, 45.0, 55.0] {
        let l = vee(deg);
        let part = Region::new(vec![l.clone()]).unwrap();
        let path = plasma_profile(&[l], &[], &part, &params());
        assert!(path.is_ok(), "{deg}° V-notch: {:?}", path.err().map(|e| e.to_string()));
        assert!(path.is_ok_and(|p| p.plasma.is_some_and(|r| r.torches.len() == 1)));
    }
}

/// #791 delta review: the corner tip zone is capped at one kerf width.
/// Uncapped it grows without bound as the opening closes, and a deep
/// narrow V-slot was exempted whole and generated with the feature
/// uncut; now only 2 r of its tip is left and the rest of its walls is
/// judged, so each of these is refused by name.
#[test]
fn a_deep_narrow_wedge_is_refused_not_exempted_as_a_corner_tip() {
    let q = Point2::new;
    // A 100 x 60 plate with a V-slot `deg` wide at its tip and `depth` deep.
    let wedge = |deg: f64, depth: f64| {
        let half = depth * (deg.to_radians() * 0.5).tan();
        Loop::new(vec![q(0.0, 0.0), q(100.0, 0.0), q(100.0, 60.0), q(50.0 + half, 60.0), q(50.0, 60.0 - depth), q(50.0 - half, 60.0), q(0.0, 60.0)])
            .unwrap()
    };
    for (deg, depth) in [(4.0, 20.0), (2.0, 40.0), (10.0, 5.0)] {
        let l = wedge(deg, depth);
        let part = Region::new(vec![l.clone()]).unwrap();
        let e = plasma_profile(&[l], &[], &part, &params()).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(e.contains("contour C0") && e.contains("bridged"), "{deg}° x {depth} mm wedge: {e:?}");
    }
}

/// #801: a picked contour with a zero-width slit - out along a line and
/// straight back, the third #791 review's probe - normalises to the
/// plain rectangle in `Region::new` (the region keeps area, and the
/// slit has none). It is refused by name, never cut as the rectangle;
/// a collinear vertex the region drops stays legal.
#[test]
fn a_zero_width_slit_in_a_picked_contour_is_refused_not_dropped() {
    let q = Point2::new;
    let slit = Loop::new(vec![q(0.0, 0.0), q(100.0, 0.0), q(100.0, 60.0), q(50.0, 60.0), q(50.0, 40.0), q(50.0, 60.0), q(0.0, 60.0)]).unwrap();
    let part = Region::new(vec![slit.clone()]).unwrap();
    let e = plasma_profile(&[slit], &[], &part, &params()).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("picked contour 1") && e.contains("X50.000 Y40.000") && e.contains("open run"), "{e:?}");
    let collinear = Loop::new(vec![q(0.0, 0.0), q(50.0, 0.0), q(100.0, 0.0), q(100.0, 60.0), q(0.0, 60.0)]).unwrap();
    let part = Region::new(vec![collinear.clone()]).unwrap();
    assert!(plasma_profile(&[collinear], &[], &part, &params()).is_ok());
}

/// #801 (decision 4a said plainly): an ordinary inside V sharper than
/// about 30° - a 20 mm mouth at 25° - is refused, not cut with its tip
/// left; the 2 r cap leaves only 2 r of its tip unjudged.
#[test]
fn a_normal_sized_v_sharper_than_30_degrees_is_refused() {
    let q = Point2::new;
    let depth = 10.0 / (25f64.to_radians() * 0.5).tan();
    let l = Loop::new(vec![q(0.0, 0.0), q(100.0, 0.0), q(100.0, 60.0), q(60.0, 60.0), q(50.0, 60.0 - depth), q(40.0, 60.0), q(0.0, 60.0)]).unwrap();
    let part = Region::new(vec![l.clone()]).unwrap();
    let e = plasma_profile(&[l], &[], &part, &params()).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("contour C0") && e.contains("bridged"), "{e:?}");
}

/// The plate with a notch `w` wide and `drop` deep cut into its top edge.
fn notched(w: f64, drop: f64) -> Loop {
    let top = 60.0;
    Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(100.0, 0.0),
        Point2::new(100.0, top),
        Point2::new(50.0 + w * 0.5, top),
        Point2::new(50.0 + w * 0.5, top - drop),
        Point2::new(50.0 - w * 0.5, top - drop),
        Point2::new(50.0 - w * 0.5, top),
        Point2::new(0.0, top),
    ])
    .unwrap()
}

/// #1053 review: the cut hands the cover law the model-aware tolerance.
/// A notch narrower than the kerf is bridged by the offset, so its floor
/// stands off the torch by more than a kerf/2; its depth is set (by the
/// geometry alone, measured off the bridged cut) so the farthest floor
/// point sits midway between the plain and the tightened reach. The same
/// plate then cuts at full fidelity and is refused by name on a coarse
/// model - through `plasma_profile`, the path the program takes.
#[test]
fn the_cover_law_reads_the_tightened_tolerance_on_a_coarse_model() {
    let plain = PlasmaParams { lead_in: 0.0, lead_out: 0.0, overburn: 0.0, ..params() };
    let coarse = PlasmaParams { model_excess: 0.007, ..plain.clone() };
    assert!(coarse.kerf_tol() < KERF_TOL, "the coarse model tightens the tolerance");
    let w = 1.0;
    let cut_at = |drop: f64, p: &PlasmaParams| {
        let l = notched(w, drop);
        plasma_profile(std::slice::from_ref(&l), &[], &Region::new(vec![l.clone()]).unwrap(), p)
    };
    // The bridged cut does not depend on the notch's depth (its floor is
    // far below the bridge); read its chords once.
    let path = cut_at(0.3, &plain).unwrap();
    let chords: Vec<(Point2, Point2)> = path.moves.iter().flat_map(|m| m.chords(emit::FIT_TOL)).collect();
    let far = |drop: f64| {
        let (x0, x1, y0) = (50.0 - w * 0.5, 50.0 + w * 0.5, 60.0 - drop);
        (0..=100)
            .map(|k| Point2::new(x0 + (x1 - x0) * f64::from(k) / 100.0, y0))
            .map(|q| chords.iter().map(|(a, b)| laws::pt_seg(q, *a, *b)).fold(f64::INFINITY, f64::min))
            .fold(0.0, f64::max)
    };
    let target = plain.kerf + 0.5 * (KERF_TOL + coarse.kerf_tol());
    let (mut lo, mut hi) = (0.3, 2.0);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if far(mid) < target { lo = mid } else { hi = mid }
    }
    let drop = 0.5 * (lo + hi);
    assert!((far(drop) - target).abs() < 1e-6, "the floor sits between the reaches");
    assert!(cut_at(drop, &plain).is_ok(), "covered at full fidelity: {:?}", cut_at(drop, &plain).err());
    let why = cut_at(drop, &coarse).expect_err("refused on the coarse model").to_string();
    assert!(why.contains("never comes within a kerf of the part edge"), "the cover law, by name: {why}");
}
