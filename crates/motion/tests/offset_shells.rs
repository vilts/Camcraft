//! M2.5 F: the offset shell - the level ladder's rings riding the
//! model's own relief between the planes instead of the staircase.
//! The XY plan stays the plain ladder's (the islands, eroded), and
//! what these tests pin is the Z every cut vertex lands on: never
//! under the part at its decision disc, on the planes for prismatic
//! models, between them where the surface is shallow, and back at
//! the plane where it turns steep. The M2.3 sim and the verify laws
//! hold the closing verdict, tilted moves included.
//! Provenance: our own design, specced from published CAM offset-shell
//! roughing behavior, per the Clean-Room Protocol.
//!
//! Gate map (M4.4): covers camcraft_motion::cl, camcraft_motion::geo2d,
//! camcraft_motion::geometry, camcraft_motion::machining2d,
//! camcraft_motion::machining3d, camcraft_motion::stock_sim,
//! camcraft_motion::verify.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::{GenError, PocketStrategy};
use camcraft_motion::machining3d::{
    Entry3D, Pocket3DParams, pocket3d, pocket3d_offset, pocket3d_offset_with_rest, pocket3d_with_rest,
};
use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, verify};

const PI: f64 = std::f64::consts::PI;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
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

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| {
        p(
            [min[0], max[0]][i & 1],
            [min[1], max[1]][(i >> 1) & 1],
            [min[2], max[2]][(i >> 2) & 1],
        )
    };
    let quad = |a: usize, b: usize, c: usize, d: usize| -> [Triangle; 2] {
        [
            Triangle::new(v(a), v(b), v(c)),
            Triangle::new(v(a), v(c), v(d)),
        ]
    };
    let mut t = Vec::with_capacity(12);
    t.extend(quad(4, 5, 7, 6)); // +z top
    t.extend(quad(0, 2, 3, 1)); // -z bottom
    t.extend(quad(0, 1, 5, 4)); // -y
    t.extend(quad(2, 6, 7, 3)); // +y
    t.extend(quad(1, 3, 7, 5)); // +x
    t.extend(quad(2, 0, 4, 6)); // -x
    t
}

/// The slab with a 10x10 boss standing to z = 0.
fn slab_with_boss() -> Vec<Triangle> {
    let mut f = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0]);
    f.extend(box_facets([-5.0, -5.0, -10.0], [5.0, 5.0, 0.0]));
    f
}

/// The slab carrying a low step: its top sits at -7.5, strictly
/// between the -5.0 plane and that plane's own stepdown floor - the
/// exact shape of the rising-leg counterexample `machining3d::shell`
/// documents. A dive landing at the floor beside it, a ring chord
/// running past it: if the leg's interpolated Z dips under -7.5 while
/// the disc still covers the step, the sim drops the step below its
/// surface and the verifier says so.
fn stepped() -> Vec<Triangle> {
    let mut f = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -12.0]);
    f.extend(box_facets([0.0, -20.0, -12.0], [20.0, 20.0, -7.5]));
    f
}

/// The slab with a hemispherical bump: r = 5 about (0, 0, -10), apex
/// at -5. The band facets and apex fan are closed below by the slab's
/// own top plane, so every slice plane crosses a closed loop.
fn dome() -> Vec<Triangle> {
    let mut f = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0]);
    let (rad, base) = (5.0_f64, -10.0_f64);
    let (n_lat, n_lon) = (6_usize, 32_usize);
    let ring = |k: usize| -> Vec<Point3> {
        let ph = (PI / 2.0) * k as f64 / n_lat as f64;
        let (dz, rr) = (rad * ph.cos(), rad * ph.sin());
        (0..n_lon)
            .map(|j| {
                let th = 2.0 * PI * j as f64 / n_lon as f64;
                p(rr * th.cos(), rr * th.sin(), base + dz)
            })
            .collect()
    };
    let apex = p(0.0, 0.0, base + rad);
    let first = ring(1);
    for j in 0..n_lon {
        f.push(Triangle::new(apex, first[j], first[(j + 1) % n_lon]));
    }
    for k in 1..n_lat {
        let ra = ring(k);
        let rb = ring(k + 1);
        for j in 0..n_lon {
            let j2 = (j + 1) % n_lon;
            f.push(Triangle::new(ra[j], rb[j], rb[j2]));
            f.push(Triangle::new(ra[j], rb[j2], ra[j2]));
        }
    }
    f
}

fn params(entry: Entry3D, helix_radius: f64) -> Pocket3DParams {
    Pocket3DParams {
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: 8.0,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        top_z: 0.0,
        bottom_z: -10.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        retract_z: None,
        entry,
        helix_radius,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

fn tool() -> ToolGeom {
    ToolGeom {
        radius: 4.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 9.0,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    }
}

fn verify_against(facets: &[Triangle], region: &Region, path: &Toolpath) -> Vec<camcraft_motion::verify::Warning> {
    let tool = tool();
    let cx = VerifyContext {
        region,
        stock: None,
        model: Some(facets),
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 0.0,
        clearance_z: 5.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(path, &cx)
}

/// The XY endpoints of every cutting move, the path's cut skeleton.
fn cut_points(t: &Toolpath) -> Vec<Point2> {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed || m.kind == MoveKind::Plunge)
        .flat_map(|m| [Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y)])
        .collect()
}

/// The Z of every cutting move's landing point - the shell's word.
fn cut_land_zs(t: &Toolpath) -> Vec<f64> {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed || m.kind == MoveKind::Plunge)
        .map(|m| m.to.z)
        .collect()
}

/// Does XY point `q` lie on some cut segment of `skel` (the plain
/// ladder's polyline)? The shell may add vertices between the
/// ladder's, never off its lines.
fn on_a_segment(q: Point2, path: &Toolpath, tol: f64) -> bool {
    path.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed || m.kind == MoveKind::Plunge)
        .any(|m| {
            let (a, b) = (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y));
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let l2 = dx * dx + dy * dy;
            let t = if l2 == 0.0 {
                0.0
            } else {
                (((q.x - a.x) * dx + (q.y - a.y) * dy) / l2).clamp(0.0, 1.0)
            };
            (q.x - (a.x + dx * t)).hypot(q.y - (a.y + dy * t)) <= tol
        })
}

#[test]
fn the_shell_on_a_prismatic_part_never_leaves_the_planes() {
    // Every roof query answers with a flat facet top, so every tip
    // is a plane or a stepdown floor - no intermediate Z anywhere -
    // and the XY skeleton is the plain ladder's own, move for move.
    let f = slab_with_boss();
    let region = rect(-18.0, -18.0, 18.0, 18.0);
    let plain = pocket3d(&f, &region, &params(Entry3D::Plunge, 1.0)).unwrap();
    let shell = pocket3d_offset(&f, &region, &params(Entry3D::Plunge, 1.0)).unwrap();
    let planes = [0.0, -5.0, -10.0];
    for z in cut_land_zs(&shell) {
        assert!(
            planes.iter().any(|&q| (z - q).abs() < 1e-12),
            "a prismatic roof lifted a vertex off the planes: z {z}"
        );
    }
    for q in cut_points(&shell) {
        assert!(
            on_a_segment(q, &plain, 1e-9),
            "shell vertex off the ladder's skeleton: ({}, {})",
            q.x,
            q.y
        );
    }
    assert!(
        cut_land_zs(&shell).len() >= cut_land_zs(&plain).len(),
        "chord subdivision only ever adds vertices"
    );
    assert_eq!(verify_against(&f, &region, &shell), Vec::new());
    assert_eq!(verify_against(&f, &region, &plain), Vec::new());
}

#[test]
fn the_shell_rides_the_dome_and_the_verifier_agrees() {
    // The lift's existence: shallow relief puts cut vertices
    // strictly between -5.0 and the floor, where the plain ladder
    // would only ever stand them AT a plane.
    let f = dome();
    let region = rect(-18.0, -18.0, 18.0, 18.0);
    let shell = pocket3d_offset(&f, &region, &params(Entry3D::Plunge, 1.0)).unwrap();
    let zs = cut_land_zs(&shell);
    assert!(
        zs.iter().any(|&z| z > -10.0 && z < -5.0),
        "the shell lifted nothing on a dome: {zs:?}"
    );
    assert!(
        zs.iter().all(|&z| z >= -10.0),
        "a tip bit below the part's own floor: {zs:?}"
    );
    assert_eq!(verify_against(&f, &region, &shell), Vec::new());
}

#[test]
fn the_stepped_part_holds_the_rising_leg() {
    // The counterexample made flesh: dives land at the -10 floor
    // beside a step standing at -7.5, and the legs that leave them
    // rise past that step. The chord cap is what keeps every
    // interpolated height above the step's own surface.
    let f = stepped();
    let region = rect(-18.0, -18.0, 18.0, 18.0);
    let shell = pocket3d_offset(&f, &region, &params(Entry3D::Plunge, 1.0)).unwrap();
    // The lift happened (floors and step tops both appear)...
    let zs = cut_land_zs(&shell);
    assert!(
        zs.iter().any(|&z| (z + 7.5).abs() < 1e-9) && zs.iter().any(|&z| (z + 10.0).abs() < 1e-9),
        "the step's two reliefs should both show: {zs:?}"
    );
    // ...and nothing bit under either.
    assert!(zs.iter().all(|&z| z >= -7.5 - 1e-12 || z >= -10.0));
    assert_eq!(verify_against(&f, &region, &shell), Vec::new());
}

#[test]
fn the_shell_entrants_land_on_the_shell() {
    // Every entry family, over the dome: the ramp's diagonal, the
    // helix's bore (widened decision disc) and the plain dive all
    // carry the tip law; the verifier reads the tilted moves honestly.
    let f = dome();
    let region = rect(-18.0, -18.0, 18.0, 18.0);
    for (entry, hr) in [(Entry3D::Ramp, 1.0), (Entry3D::Helix, 3.0), (Entry3D::Plunge, 1.0)] {
        let t = pocket3d_offset(&f, &region, &params(entry, hr))
            .unwrap_or_else(|e| panic!("{entry:?} entry refused on the dome: {e}"));
        if entry == Entry3D::Helix {
            // The corrected dive-room law lets the widened disc LAND
            // at the -5.0 plane - and a landed bore is not yet a
            // Crossing-clean one: the shell's -5.0 rows stand lifted
            // (their tips on the dome's roof) where the descending
            // arcs cross them, a positive Z interval in the
            // verifier's eyes. The arm substitutes, and says so: an
            // axial dive shares no XY ground with anything, the one
            // motion the Crossing law cannot name.
            assert!(
                t.notes
                    .iter()
                    .any(|n| n.contains("the shell lifts legs the bore would cross")),
                "the dome's helix must substitute by name, notes: {:?}",
                t.notes
            );
        }
        assert_eq!(verify_against(&f, &region, &t), Vec::new(), "{entry:?} entry");
        assert!(
            t.moves.iter().all(|m| m.kind != MoveKind::Plunge || m.from.x == m.to.x),
            "{entry:?}: a plunge that is not axial"
        );
    }
}

#[test]
fn nothing_to_cut_is_still_refused_by_name() {
    // The shell changes tips, never targets: a model filling every
    // plane says Empty exactly as the plain ladder does.
    let f = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, 5.0]);
    let region = rect(-18.0, -18.0, 18.0, 18.0);
    let plain = pocket3d(&f, &region, &params(Entry3D::Plunge, 1.0));
    let shell = pocket3d_offset(&f, &region, &params(Entry3D::Plunge, 1.0));
    assert!(matches!(plain, Err(GenError::Empty(_))), "{plain:?}");
    assert!(matches!(shell, Err(GenError::Empty(_))), "{shell:?}");
}

#[test]
fn a_corrupt_model_is_said_by_name() {
    // The shell's one new input check: the Field boxes every facet
    // and refuses a non-finite vertex before any path exists.
    let mut f = dome();
    f.push(Triangle::new(p(0.0, 0.0, f64::NAN), p(1.0, 0.0, -9.0), p(0.0, 1.0, -9.0)));
    let region = rect(-18.0, -18.0, 18.0, 18.0);
    let e = match pocket3d_offset(&f, &region, &params(Entry3D::Plunge, 1.0)) {
        Err(e) => e,
        Ok(t) => panic!("a non-finite facet must refuse the shell, got {} moves", t.moves.len()),
    };
    assert!(format!("{e}").contains("non-finite"), "{e}");
}

#[test]
#[ignore = "heavy lane (M4.4): full-dome rest verdicts over the dexel sim - milestone close-out and pre-push"]
fn the_shell_costs_the_rest_ladder_less_to_finish() {
    // The pyramid is gone from the coarse pass, so what the rest
    // ladder still owes the dome is no more than the plain ladder's
    // bill - and the shell owes nothing where the plain ladder does.
    let f = dome();
    let region = rect(-18.0, -18.0, 18.0, 18.0);
    let (_tp, plain) = pocket3d_with_rest(&f, &region, &params(Entry3D::Plunge, 1.0)).unwrap();
    let (_ts, shell) = pocket3d_offset_with_rest(&f, &region, &params(Entry3D::Plunge, 1.0)).unwrap();
    assert!(shell.max_gouge_depth < 0.01, "the shell gouged: {shell:?}");
    assert!(
        shell.max_rest_height <= plain.max_rest_height + 1e-9,
        "shell left more standing than the staircase ladder: shell {:?} vs plain {:?}",
        shell.max_rest_height,
        plain.max_rest_height
    );
}
