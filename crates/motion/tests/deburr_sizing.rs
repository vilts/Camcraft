//! #1890 deburr sizing tests measure the chord and its perpendicular
//! depth from actual cutter intersections with the two fixture faces.
//! Passes stay inside the tip window and the final declared EdgeBreak.
//! A Round profile on the sharp cone refuses by name.
//!
//! Gate map (#1890): covers camcraft_motion::finishing3d (deburr,
//! deburr_size). Lane shape: every law here runs in seconds.

use camcraft_motion::cl::{EdgeBreak, MoveKind, Toolpath};
use camcraft_motion::finishing3d::{deburr, BreakMode, BreakProfile, DeburrBreak, DeburrParams, UnreachableEdges};
use camcraft_motion::geometry::{Point3, Triangle};

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A 90-deg cone (the setback is the drop), a 1 mm chord, and the tip
/// window z -3..0: only edges at z = 0 are in reach.
fn params(sizing: DeburrBreak) -> DeburrParams {
    DeburrParams {
        width: 1.0,
        tip_angle_deg: 90.0,
        clearance_z: 20.0,
        top_z: 0.0,
        bottom_z: -3.0,
        feed: 500.0,
        plunge_feed: 100.0,
        min_edge_angle_deg: 0.0,
        exclude_holes: false,
        hole_min_diameter: 0.0,
        hole_max_diameter: 0.0,
        unreachable: UnreachableEdges::SkipWithNote,
        sizing,
    }
}

/// Extrude a CCW (y, z) polygon along +x into a closed solid.
fn prism(poly: &[(f64, f64)], x_len: f64) -> Vec<Triangle> {
    let v = |side: f64, (y, z): (f64, f64)| p3(side, y, z);
    let mut t = Vec::new();
    for i in 0..poly.len() {
        let j = (i + 1) % poly.len();
        let (a, b) = (v(0.0, poly[i]), v(0.0, poly[j]));
        let (c, d) = (v(x_len, poly[j]), v(x_len, poly[i]));
        t.push(Triangle::new(a, b, c));
        t.push(Triangle::new(a, c, d));
    }
    for side in [0.0, x_len] {
        for i in 1..poly.len() - 1 {
            let (a, b, c) = (v(side, poly[0]), v(side, poly[i]), v(side, poly[i + 1]));
            t.push(if side > 0.0 { Triangle::new(a, b, c) } else { Triangle::new(a, c, b) });
        }
    }
    t
}

/// A block whose flat top (z = 0, y 5..20) meets a face that falls 60
/// deg below level at y = 5: a 120-degree wedge along x. The edge at
/// y = 20 is a 90-degree wedge.
fn wedge_block() -> Vec<Triangle> {
    let run = 10.0 / 60f64.to_radians().tan();
    prism(&[(5.0 - run, -10.0), (20.0, -10.0), (20.0, 0.0), (5.0, 0.0)], 30.0)
}

/// The declared break that runs along the long line y = `y`, z = 0.
fn break_on(t: &Toolpath, y: f64) -> &EdgeBreak {
    let on = |q: &Point3| (q.y - y).abs() < 1e-9 && q.z.abs() < 1e-9;
    t.breaks
        .iter()
        .find(|b| b.edge.windows(2).any(|s| on(&s[0]) && on(&s[1]) && (s[0].x - s[1].x).abs() > 29.0))
        .unwrap_or_else(|| panic!("no break on y = {y}: {:?}", t.breaks))
}

/// Intersect the actual emitted cone floor z=-drop+abs(y) with the
/// fixture faces. These equations use neither the size law nor setback().
fn contact_chord_and_depth(t: &Toolpath, edge_y: f64) -> (f64, f64) {
    let m = t
        .moves
        .iter()
        .find(|m| {
            m.kind == MoveKind::Feed
                && (m.from.y - edge_y).abs() < 1e-9
                && (m.to.y - edge_y).abs() < 1e-9
                && (m.to.x - m.from.x).abs() > 29.0
        })
        .expect("a cut along the fixture edge");
    let drop = -m.to.z;
    assert!(drop > 0.0);
    let a = (drop, 0.0);
    let b = if edge_y == 5.0 {
        let y = -drop / (1.0 + 3f64.sqrt());
        (y, 3f64.sqrt() * y)
    } else {
        (0.0, -drop)
    };
    let chord = (a.0 - b.0).hypot(a.1 - b.1);
    let depth = (a.0 * b.1 - a.1 * b.0).abs() / chord;
    (chord, depth)
}

#[test]
fn constant_width_measures_actual_cone_contacts_on_120_and_90_degree_wedges() {
    let t = deburr(&wedge_block(), &params(DeburrBreak::default())).expect("the top edges ride");
    for edge_y in [5.0, 20.0] {
        let (chord, _) = contact_chord_and_depth(&t, edge_y);
        assert!(
            (chord - 1.0).abs() < 1e-9,
            "actual cone contact chord {chord}, requested 1 at y={edge_y}"
        );
    }
}

#[test]
fn constant_depth_measures_actual_cone_contacts_on_120_and_90_degree_wedges() {
    let sizing = DeburrBreak {
        mode: BreakMode::ConstantDepth,
        depth: 0.5,
        ..DeburrBreak::default()
    };
    let t = deburr(&wedge_block(), &params(sizing)).expect("the top edges ride");
    for edge_y in [5.0, 20.0] {
        let (_, depth) = contact_chord_and_depth(&t, edge_y);
        assert!(
            (depth - 0.5).abs() < 1e-9,
            "actual perpendicular chord depth {depth}, requested 0.5 at y={edge_y}"
        );
    }
}

#[test]
fn multiple_passes_refuse_a_window_that_only_contains_the_final_tip() {
    let mut p = params(DeburrBreak {
        passes: 3,
        ..DeburrBreak::default()
    });
    // Keep the final tip inside despite independent contact rounding.
    p.top_z = -p.drop() + 1e-8;
    p.bottom_z = -p.drop() - 1e-8;
    p.clearance_z = p.top_z;
    let error = deburr(&wedge_block(), &p).expect_err("the outer pass leaves the tip window");
    assert!(
        error.to_string().contains("a pass leaves the tip window"),
        "{error}"
    );
}

#[test]
fn passes_step_from_the_outer_break_to_the_final_one_inside_the_declared_break() {
    let one = deburr(&wedge_block(), &params(DeburrBreak::default())).expect("one pass");
    let three = deburr(&wedge_block(), &params(DeburrBreak { passes: 3, ..DeburrBreak::default() })).expect("three passes");
    let count = |t: &Toolpath| t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count();
    assert_eq!(count(&three), 3 * count(&one), "each run rides three times");
    assert_eq!(three.breaks, one.breaks, "the passes declare the final break only");
    // The 120-deg run: plunges at drop/3, 2drop/3, drop, outer to final.
    let s = break_on(&three, 5.0).depth;
    // (Its plunges are the ones whose next feed stays on y = 5; the
    // 90-deg run starts at the same corner and turns to y = 20.)
    let on_line: Vec<f64> = three
        .moves
        .windows(2)
        .filter(|w| w[0].kind == MoveKind::Plunge && (w[0].to.y - 5.0).abs() < 1e-9 && (w[1].to.y - 5.0).abs() < 1e-9)
        .map(|w| -w[0].to.z)
        .collect();
    assert_eq!(on_line.len(), 3, "{on_line:?}");
    for (k, d) in on_line.iter().enumerate() {
        let want = s * (k + 1) as f64 / 3.0;
        assert!((d - want).abs() < 1e-9, "pass {k}: drop {d}, want {want}");
    }
    // Every cutting tip stands on or above the declared bevel.
    for m in three.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        let floor = three.breaks.iter().filter_map(|b| b.floor_at(m.to.x, m.to.y)).map(|(f, _)| f).fold(f64::INFINITY, f64::min);
        assert!(m.to.z >= floor - 1e-9, "tip {:?} under the declared floor {floor}", m.to);
    }
}

#[test]
fn a_round_profile_on_a_sharp_cone_refuses_by_name() {
    let sizing = DeburrBreak { profile: BreakProfile::Round, ..DeburrBreak::default() };
    let e = deburr(&wedge_block(), &params(sizing)).expect_err("a sharp cone cuts no round");
    let msg = e.to_string();
    assert!(msg.contains("Round break profile needs a ball-tipped cutter"), "{msg}");
    let e = deburr(&wedge_block(), &params(DeburrBreak { passes: 0, ..DeburrBreak::default() })).expect_err("no pass");
    assert!(e.to_string().contains("passes 1 to"), "{e}");
}

#[test]
fn the_size_law_names_the_wedges_it_reaches_and_its_bound() {
    let w = DeburrBreak::default();
    let d = DeburrBreak { mode: BreakMode::ConstantDepth, depth: 0.5, ..DeburrBreak::default() };
    assert!(w.setback(1.0, 20.0).is_none(), "a wedge sharper than 30 deg is out of Constant Width's range");
    assert!(d.setback(1.0, 160.0).is_none(), "a wedge flatter than 150 deg is out of Constant Depth's range");
    // The bound is the largest setback each mode can give.
    assert!((w.max_setback(1.0) - 1.0 / (2.0 * 15f64.to_radians().sin())).abs() < 1e-12);
    assert!((d.max_setback(1.0) - 0.5 / 75f64.to_radians().cos()).abs() < 1e-12);
    for theta in [30.0, 60.0, 90.0, 120.0, 179.0] {
        assert!(w.setback(1.0, theta).is_some_and(|s| s <= w.max_setback(1.0) + 1e-12), "{theta}");
    }
}

#[test]
fn a_real_bottom_face_without_cone_contact_refuses_by_name() {
    let mut p = params(DeburrBreak::default());
    p.top_z = -10.0;
    p.bottom_z = -12.0;
    let error = deburr(&wedge_block(), &p).expect_err("the cutter cannot contact the underside");
    assert!(error.to_string().contains("unsupported face orientation"), "{error}");
}

#[test]
fn clearance_normalizes_the_numeric_skin_before_any_traverse() {
    let mut p = params(DeburrBreak::default());
    p.top_z = -p.drop();
    p.bottom_z = p.top_z;
    p.clearance_z = p.top_z;
    let t = deburr(&wedge_block(), &p).expect("the single final tip fits the exact window");
    assert_eq!(t.moves.last().expect("moves").to.z, p.clearance_z, "finish exactly at clearance");
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Rapid && (m.from.x != m.to.x || m.from.y != m.to.y)) {
        assert_eq!(m.from.z, p.clearance_z, "a traverse starts at clearance");
        assert_eq!(m.to.z, p.clearance_z, "a traverse stays at clearance");
    }
}

#[test]
fn an_edge_at_the_window_floor_is_excluded_before_contact_sizing() {
    let curtain = vec![
        Triangle::new(p3(0.0, 0.0, -10.0), p3(10.0, 0.0, -10.0), p3(10.0, 0.0, 0.0)),
        Triangle::new(p3(0.0, 0.0, -10.0), p3(10.0, 0.0, 0.0), p3(0.0, 0.0, 0.0)),
    ];
    let mut p = params(DeburrBreak::default());
    p.bottom_z = -10.0;
    for mesh in [wedge_block(), curtain] {
        let t = deburr(&mesh, &p).expect("every positive drop puts a floor edge below the window");
        assert!(!t.breaks.is_empty(), "the supported top edges still ride");
        assert!(t.breaks.iter().all(|b| b.edge.iter().all(|q| q.z == 0.0)), "only the top edges ride: {:?}", t.breaks);
    }
}
