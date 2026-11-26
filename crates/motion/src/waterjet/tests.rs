//! The waterjet profile's laws (#1883): inner-first order, kerf side,
//! leads with the pierce in the waste, tabs as uncut gaps, the
//! tight-hole rule, open-run sides, arc linearization and refusals.

use super::*;
use crate::cl::{MoveKind, Toolpath};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).unwrap()
}

fn circle(cx: f64, cy: f64, r: f64) -> Loop {
    Loop::new(
        (0..96)
            .map(|k| {
                let t = std::f64::consts::TAU * f64::from(k) / 96.0;
                Point2::new(cx + r * t.cos(), cy + r * t.sin())
            })
            .collect(),
    )
    .unwrap()
}

fn params() -> WaterjetParams {
    WaterjetParams {
        kerf: 1.0,
        control_comp: false,
        reversed: false,
        lead_shape: LeadShape::Arc,
        lead_in: 3.0,
        lead_out: 2.0,
        open_side: OpenSide::Center,
        tabs: None,
        pierce_time: 1.5,
        abrasive_delay: 0.5,
        linearize: None,
        feed: 300.0,
        clearance_z: 10.0,
        cut_z: 0.0,
        model_excess: 0.0,
    }
}

/// A 100 x 60 plate with a Ø30 hole and a 10 x 10 square hole.
fn plate() -> (Vec<Loop>, Region) {
    let loops = vec![rect(0.0, 0.0, 100.0, 60.0), circle(30.0, 30.0, 15.0), rect(70.0, 25.0, 80.0, 35.0)];
    let part = Region::new(loops.clone()).unwrap();
    (loops, part)
}

/// Each pierce: the rapid that descends to the cut plane.
fn pierces(path: &Toolpath, cut_z: f64) -> Vec<Point2> {
    path.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid && m.to.z == cut_z && m.from.z > cut_z)
        .map(|m| Point2::new(m.to.x, m.to.y))
        .collect()
}

fn feeds(path: &Toolpath) -> Vec<Point2> {
    path.moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| Point2::new(m.to.x, m.to.y)).collect()
}

#[test]
fn holes_are_cut_before_the_outline_and_every_pierce_is_in_the_waste() {
    let (loops, part) = plate();
    let p = params();
    let path = waterjet_profile(&loops, &[], &part, &p).unwrap();
    let ps = pierces(&path, p.cut_z);
    assert_eq!(ps.len(), 3, "one pierce per contour: {ps:?}");
    // The two holes first (their pierces stand inside the holes), the
    // outline last (its pierce stands outside the plate).
    let outline = Region::new(vec![rect(0.0, 0.0, 100.0, 60.0)]).unwrap();
    assert!(outline.contains(ps[0]) && outline.contains(ps[1]), "holes first: {ps:?}");
    assert!(!outline.contains(ps[2]), "the outline last, pierced outside: {:?}", ps[2]);
    for q in &ps {
        assert!(!part.contains(*q), "a pierce on the part at {q:?}");
    }
    // The kerf side: every lit point stands at least r - tol off the part.
    let grown = part.offset(0.5 - KERF_TOL).unwrap();
    for q in feeds(&path) {
        assert!(!grown.contains(q), "a lit point reaches the part at {q:?}");
    }
    assert!(path.has_arcs(), "arc leads and the round hole ride as arcs");
    assert!(path.notes.iter().any(|n| n.contains("cut inner first (2 hole(s), 0 tab(s)), 3 pierce(s)")), "{:?}", path.notes);
}

#[test]
fn straight_leads_lay_no_arc_on_a_square_part() {
    let loops = vec![rect(0.0, 0.0, 50.0, 50.0)];
    let part = Region::new(loops.clone()).unwrap();
    let p = WaterjetParams { lead_shape: LeadShape::Straight, ..params() };
    let path = waterjet_profile(&loops, &[], &part, &p).unwrap();
    let lit: Vec<_> = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    let (first, last) = (lit[0], lit[lit.len() - 1]);
    assert!(first.arc.is_none() && last.arc.is_none(), "straight lead-in and lead-out: {first:?} {last:?}");
    let ps = pierces(&path, p.cut_z);
    // The lead-in is perpendicular: the pierce stands 3 mm + r square
    // off the middle of one edge (the longest edge's middle is the start).
    assert_eq!(ps.len(), 1);
    let q = ps[0];
    let off = [-q.x, q.x - 50.0, -q.y, q.y - 50.0].into_iter().fold(f64::NEG_INFINITY, f64::max);
    assert!((off - 3.5).abs() < 1e-6, "the pierce stands r + 3 mm square off an edge: {q:?}");
}

#[test]
fn tabs_leave_uncut_gaps_and_each_stretch_is_pierced_and_led_in() {
    let loops = vec![rect(0.0, 0.0, 100.0, 60.0)];
    let part = Region::new(loops.clone()).unwrap();
    let at = vec![Point2::new(50.0, 0.0), Point2::new(50.0, 60.0)];
    // 6 mm tabs: wide enough that the leads beside a gap cannot reach
    // its middle, so the cover law must know the bridge is meant.
    let p = WaterjetParams { tabs: Some(JetTabs { count: 2, width: 6.0, at }), ..params() };
    let path = waterjet_profile(&loops, &[], &part, &p).unwrap();
    let ps = pierces(&path, p.cut_z);
    assert_eq!(ps.len(), 2, "two stretches between two tabs, each pierced: {ps:?}");
    for q in &ps {
        assert!(!part.contains(*q), "a stretch pierced on the part at {q:?}");
    }
    // Each 6 mm bridge stays: no lit chord comes within the kerf radius
    // of the part edge along the tab (x 47..53 on both long edges).
    let chords: Vec<(Point2, Point2)> =
        path.moves.iter().filter(|m| m.kind == MoveKind::Feed).flat_map(|m| m.chords(0.005)).collect();
    for k in 0..=59 {
        let x = 47.05 + 0.1 * f64::from(k);
        for edge in [Point2::new(x, 0.0), Point2::new(x, 60.0)] {
            let d = chords.iter().map(|(a, b)| crate::sheet::laws::pt_seg(edge, *a, *b)).fold(f64::INFINITY, f64::min);
            assert!(d > 0.5 - 1e-6, "the jet cuts the tab at {edge:?} ({d:.4} mm)");
        }
    }
    assert!(path.notes.iter().any(|n| n.contains("2 tab(s)), 2 pierce(s)")), "{:?}", path.notes);
}

/// Does the material under a tab join the part to the sheet beyond the
/// leads? The kerf of every lit chord is rastered in a window around the
/// tab (u along the edge, v out along the waste normal `n` from the edge
/// point `foot`), and the uncut cells are flood-filled from inside the
/// part. The tab holds when the fill reaches the window's far side.
fn tab_joins_sheet(path: &Toolpath, foot: Point2, n: Point2, r: f64) -> bool {
    use crate::sheet::laws::pt_seg;
    let chords: Vec<(Point2, Point2)> =
        path.moves.iter().filter(|m| m.kind == MoveKind::Feed).flat_map(|m| m.chords(0.005)).collect();
    let (h, u0, v0, nu, nv) = (0.025, -6.0, -2.0, 481usize, 441usize);
    let t = Point2::new(-n.y, n.x);
    let at = |i: usize, j: usize| {
        let (u, v) = (u0 + h * i as f64, v0 + h * j as f64);
        Point2::new(foot.x + t.x * u + n.x * v, foot.y + t.y * u + n.y * v)
    };
    let open: Vec<bool> = (0..nu * nv)
        .map(|c| {
            let q = at(c % nu, c / nu);
            chords.iter().map(|(a, b)| pt_seg(q, *a, *b)).fold(f64::INFINITY, f64::min) > r
        })
        .collect();
    let start = (nu / 2) + nu * ((-1.0 - v0) / h) as usize;
    let mut seen = vec![false; nu * nv];
    let mut stack = vec![start];
    while let Some(c) = stack.pop() {
        if seen[c] || !open[c] {
            continue;
        }
        seen[c] = true;
        let (i, j) = (c % nu, c / nu);
        if j == nv - 1 {
            return true;
        }
        if i > 0 {
            stack.push(c - 1);
        }
        if i + 1 < nu {
            stack.push(c + 1);
        }
        if j > 0 {
            stack.push(c - nu);
        }
        stack.push(c + nu);
    }
    false
}

#[test]
fn arc_leads_beside_a_tab_leave_the_tab_joined_to_the_sheet() {
    // 2 mm tabs with 3 mm / 2 mm arc leads: an arc lead-in from behind
    // a stretch start and an arc lead-out past a stretch end both reach
    // under the gap, and their kerfs would cut the tab free. The leads
    // beside a tab are laid straight, so each tab still holds.
    let loops = vec![rect(0.0, 0.0, 100.0, 60.0)];
    let part = Region::new(loops.clone()).unwrap();
    let at = vec![Point2::new(50.0, 0.0), Point2::new(50.0, 60.0)];
    let p = WaterjetParams { tabs: Some(JetTabs { count: 2, width: 2.0, at }), ..params() };
    let path = waterjet_profile(&loops, &[], &part, &p).unwrap();
    let r = p.kerf * 0.5;
    for (foot, n) in [(Point2::new(50.0, 0.0), Point2::new(0.0, -1.0)), (Point2::new(50.0, 60.0), Point2::new(0.0, 1.0))] {
        assert!(tab_joins_sheet(&path, foot, n, r), "the tab at {foot:?} is cut free of the sheet");
    }
    assert!(path.notes.iter().any(|n| n.contains("(tabbed) is led in and out straight")), "{:?}", path.notes);
}

#[test]
fn the_bridge_law_refuses_a_kerf_across_a_tab() {
    // A tab at X50 on the bottom edge of a 100 x 60 plate, kerf 1 mm.
    let edge = [Point2::new(0.0, 0.0), Point2::new(100.0, 0.0), Point2::new(100.0, 60.0), Point2::new(0.0, 60.0)];
    let gap = tabs::Gap { no: 1, lo: 0.0, hi: 3.0, mid: Point2::new(50.0, -0.5), half: 1.5 };
    let reach = tabs::bridge_reach(1.0, 3.0, 2.0);
    // Two straight leads beside the gap leave the bridge whole.
    let ok = [
        (Point2::new(48.5, -3.5), Point2::new(48.5, -0.5)),
        (Point2::new(51.5, -0.5), Point2::new(51.5, -2.5)),
    ];
    assert!(tabs::bridge_law(&[gap], &edge, &ok, 0.5, reach).is_ok());
    // A lead that crosses under the gap cuts the bridge: refused.
    let cut = [ok[0], (Point2::new(51.5, -0.5), Point2::new(49.8, -1.8))];
    let (no, q) = tabs::bridge_law(&[gap], &edge, &cut, 0.5, reach).unwrap_err();
    assert_eq!(no, 1);
    assert!((q.x - 50.0).abs() < 1e-9 && q.y < 0.0, "{q:?}");
}

#[test]
fn a_tight_hole_is_pierced_at_its_centre() {
    // A Ø4 hole: the 3 mm arc lead-in has no room even halved four
    // times (the ring is Ø3 with a 1 mm kerf), so it is centre-pierced.
    let loops = vec![rect(0.0, 0.0, 40.0, 40.0), circle(20.0, 20.0, 2.0)];
    let part = Region::new(loops.clone()).unwrap();
    let p = WaterjetParams { lead_in: 20.0, ..params() };
    let path = waterjet_profile(&loops, &[], &part, &p).unwrap();
    let ps = pierces(&path, p.cut_z);
    assert!((ps[0].x - 20.0).abs() < 0.05 && (ps[0].y - 20.0).abs() < 0.05, "centre pierce: {:?}", ps[0]);
    assert!(path.notes.iter().any(|n| n.contains("tight hole")), "{:?}", path.notes);
}

#[test]
fn linearize_sends_every_arc_as_chords() {
    let (loops, part) = plate();
    let p = WaterjetParams { linearize: Some(0.01), ..params() };
    let path = waterjet_profile(&loops, &[], &part, &p).unwrap();
    assert!(!path.has_arcs(), "no arc survives linearization");
    let grown = part.offset(0.5 - KERF_TOL).unwrap();
    for q in feeds(&path) {
        assert!(!grown.contains(q), "a chord point reaches the part at {q:?}");
    }
    assert!(path.notes.iter().any(|n| n.contains("arcs sent as chords")));
}

#[test]
fn an_open_run_rides_its_stated_side() {
    let run = vec![Point2::new(0.0, 0.0), Point2::new(20.0, 0.0)];
    for (side, y) in [(OpenSide::Center, 0.0), (OpenSide::Left, 0.5), (OpenSide::Right, -0.5)] {
        let p = WaterjetParams { open_side: side, ..params() };
        let path = waterjet_profile(&[], std::slice::from_ref(&run), &Region::empty(), &p).unwrap();
        let f = feeds(&path);
        assert!(f.iter().all(|q| (q.y - y).abs() < 1e-9), "{side:?}: {f:?}");
    }
}

#[test]
fn control_compensation_records_a_comp_span_per_untabbed_contour() {
    let (loops, part) = plate();
    let p = WaterjetParams { control_comp: true, lead_in: 3.0, lead_out: 3.0, ..params() };
    let path = waterjet_profile(&loops, &[], &part, &p).unwrap();
    assert_eq!(path.comps.len(), 3, "one G41/G42 span per contour");
}

#[test]
fn bad_input_is_refused_by_name() {
    let (loops, part) = plate();
    let cases: Vec<(WaterjetParams, &str)> = vec![
        (WaterjetParams { kerf: 0.0, ..params() }, "kerf width must be a positive"),
        (WaterjetParams { feed: f64::NAN, ..params() }, "cut feed must be a positive"),
        (WaterjetParams { pierce_time: -1.0, ..params() }, "pierce time must be zero or more"),
        (WaterjetParams { linearize: Some(0.0), ..params() }, "linearization tolerance"),
        (WaterjetParams { control_comp: true, lead_in: 0.2, ..params() }, "needs a straight lead-in"),
        (
            WaterjetParams { tabs: Some(JetTabs { count: 2, width: 2.0, at: vec![Point2::new(50.0, 0.0)] }), ..params() },
            "tab count is 2 but 1 tab position",
        ),
        (
            WaterjetParams { tabs: Some(JetTabs { count: 1, width: 2.0, at: vec![Point2::new(50.0, 30.0)] }), ..params() },
            "place it on a contour's edge",
        ),
        (
            WaterjetParams {
                tabs: Some(JetTabs { count: 2, width: 4.0, at: vec![Point2::new(50.0, 0.0), Point2::new(52.0, 0.0)] }),
                ..params()
            },
            "tabs 1 and 2 overlap",
        ),
    ];
    for (p, want) in cases {
        let e = waterjet_profile(&loops, &[], &part, &p).unwrap_err().to_string();
        assert!(e.contains("waterjet profile") && e.contains(want), "want {want:?}, got {e}");
    }
    let e = waterjet_profile(&[], &[], &part, &params()).unwrap_err().to_string();
    assert!(e.contains("no contour to cut"), "{e}");
}

#[test]
fn a_slot_narrower_than_the_kerf_is_refused_not_bridged() {
    // A 0.6 mm notch in the outline: the 1 mm kerf bridges it.
    let notch = Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(20.0, 0.0),
        Point2::new(20.0, 10.0),
        Point2::new(10.3, 10.0),
        Point2::new(10.3, 5.0),
        Point2::new(9.7, 5.0),
        Point2::new(9.7, 10.0),
        Point2::new(0.0, 10.0),
    ])
    .unwrap();
    let part = Region::new(vec![notch.clone()]).unwrap();
    let e = waterjet_profile(&[notch], &[], &part, &params()).unwrap_err().to_string();
    assert!(e.contains("the jet never comes within a kerf"), "{e}");
}
