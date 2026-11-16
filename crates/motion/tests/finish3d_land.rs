//! #1196: the land model - a 3D finish over a plate with bolt holes,
//! planned on a drive surface whose closed openings are lidded, flies
//! over the holes instead of diving down their walls, and is still
//! judged against the REAL plate.
//!
//! Gate map (#1196): covers camcraft_motion::finishing3d (fill).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::finishing3d::{
    fill_openings, parallel, waterline, Finish3DParams, FinishPattern,
};
use camcraft_motion::geometry::{Point3, Triangle};

const R: f64 = 1.5; // a 3 mm ball, as in the report
const TOL: f64 = 0.05;
const TOP: f64 = 8.0;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn quad(out: &mut Vec<Triangle>, p: [Point3; 4]) {
    out.push(Triangle::new(p[0], p[1], p[2]));
    out.push(Triangle::new(p[0], p[2], p[3]));
}

/// A stepped block on an XY grid of cells: each cell stands to its
/// height (`None` = through), with top, bottom and every wall where a
/// neighbour (or the outside) stands lower.
fn block(xs: &[f64], ys: &[f64], h: &dyn Fn(usize, usize) -> Option<f64>) -> Vec<Triangle> {
    let (nx, ny) = (xs.len() - 1, ys.len() - 1);
    let at = |i: isize, j: isize| {
        if i < 0 || j < 0 || i >= nx as isize || j >= ny as isize {
            None
        } else {
            h(i as usize, j as usize)
        }
    };
    let mut f = Vec::new();
    for j in 0..ny {
        for i in 0..nx {
            let Some(z) = h(i, j) else { continue };
            let (x0, x1, y0, y1) = (xs[i], xs[i + 1], ys[j], ys[j + 1]);
            quad(&mut f, [p3(x0, y0, z), p3(x1, y0, z), p3(x1, y1, z), p3(x0, y1, z)]);
            quad(&mut f, [p3(x0, y0, 0.0), p3(x0, y1, 0.0), p3(x1, y1, 0.0), p3(x1, y0, 0.0)]);
            let (ii, jj) = (i as isize, j as isize);
            let sides = [
                (at(ii - 1, jj), [p3(x0, y1, 0.0), p3(x0, y0, 0.0)]),
                (at(ii + 1, jj), [p3(x1, y0, 0.0), p3(x1, y1, 0.0)]),
                (at(ii, jj - 1), [p3(x0, y0, 0.0), p3(x1, y0, 0.0)]),
                (at(ii, jj + 1), [p3(x1, y1, 0.0), p3(x0, y1, 0.0)]),
            ];
            for (n, [a, b]) in sides {
                let lo = n.unwrap_or(0.0);
                if lo < z {
                    let (a0, b0) = (p3(a.x, a.y, lo), p3(b.x, b.y, lo));
                    quad(&mut f, [a0, b0, p3(b.x, b.y, z), p3(a.x, a.y, z)]);
                }
            }
        }
    }
    f
}

/// 40 x 20 x 8 plate, two 3.4 mm square through holes centred at
/// (10, 10) and (30, 10) - a 3 mm ball fits inside each.
fn holed_plate() -> Vec<Triangle> {
    let xs = [0.0, 8.3, 11.7, 28.3, 31.7, 40.0];
    let ys = [0.0, 8.3, 11.7, 20.0];
    block(&xs, &ys, &|i, j| (!(j == 1 && (i == 1 || i == 3))).then_some(TOP))
}

/// Same plate with one 3.4 mm blind pocket (floor at z 4) at (10, 10).
fn pocketed_plate() -> Vec<Triangle> {
    let xs = [0.0, 8.3, 11.7, 40.0];
    let ys = [0.0, 8.3, 11.7, 20.0];
    block(&xs, &ys, &|i, j| Some(if i == 1 && j == 1 { 4.0 } else { TOP }))
}

/// A 3.4 mm slot running the plate's full width: open to the edge.
fn slotted_plate() -> Vec<Triangle> {
    let xs = [0.0, 18.3, 21.7, 40.0];
    let ys = [0.0, 20.0];
    block(&xs, &ys, &|i, _| Some(if i == 1 { 4.0 } else { TOP }))
}

fn fp(pattern: FinishPattern) -> Finish3DParams {
    Finish3DParams {
        pattern,
        stepover: None,
        tool_diameter: 2.0 * R,
        tolerance: TOL,
        clearance_z: 20.0,
        retract_z: None,
        top_z: TOP,
        bottom_z: 0.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
    }
}

/// Within half a millimetre of a hole's axis: no ball resting on the
/// rim reaches here at the top's height.
fn centre(q: Point3) -> bool {
    [10.0, 30.0].iter().any(|cx| (q.x - cx).abs() < 0.5 && (q.y - 10.0).abs() < 0.5)
}

fn in_hole(q: Point3) -> bool {
    [10.0, 30.0].iter().any(|cx| (q.x - cx).abs() < 1.7 && (q.y - 10.0).abs() < 1.7)
}

/// Feed endpoints and midpoints - every place the tip is promised.
fn tips(t: &Toolpath) -> Vec<Point3> {
    t.moves
        .iter()
        .filter(|m| matches!(m.kind, MoveKind::Feed))
        .flat_map(|m| {
            let mid = p3(0.5 * (m.from.x + m.to.x), 0.5 * (m.from.y + m.to.y), 0.5 * (m.from.z + m.to.z));
            [m.from, m.to].into_iter().chain(m.arc.is_none().then_some(mid))
        })
        .collect()
}

/// Ericson's closest point on a triangle, distance only.
fn pt_tri(p: Point3, t: &Triangle) -> f64 {
    let (ab, ac, ap) = (t.b - t.a, t.c - t.a, p - t.a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    let n = |u: Point3| u.dot(u).sqrt();
    let ax = |o: Point3, u: Point3, s: f64| p3(o.x + u.x * s, o.y + u.y * s, o.z + u.z * s);
    if d1 <= 0.0 && d2 <= 0.0 {
        return n(ap);
    }
    let bp = p - t.b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return n(bp);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return n(p - ax(t.a, ab, d1 / (d1 - d3)));
    }
    let cp = p - t.c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return n(cp);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return n(p - ax(t.a, ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return n(p - ax(t.b, t.c - t.b, (d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let den = 1.0 / (va + vb + vc);
    n(p - ax(ax(t.a, ab, vb * den), ac, vc * den))
}

/// The deepest bite of the ball into `model` over the path's tips.
fn deepest_bite(t: &Toolpath, model: &[Triangle]) -> f64 {
    tips(t)
        .into_iter()
        .map(|q| {
            let c = p3(q.x, q.y, q.z + R);
            let d = model.iter().map(|f| pt_tri(c, f)).fold(f64::INFINITY, f64::min);
            R - d
        })
        .fold(f64::NEG_INFINITY, f64::max)
}

fn sink(t: &Toolpath, dz: f64) -> Toolpath {
    let mut s = t.clone();
    for m in &mut s.moves {
        m.from.z -= dz;
        m.to.z -= dz;
    }
    s
}

#[test]
fn land_finish_flies_over_the_bolt_holes_and_verifies_on_the_real_plate() {
    let plate = holed_plate();
    let filled = fill_openings(&plate, 5.0, TOL).expect("fill");
    assert_eq!(filled.openings, 2, "both bolt holes are closed openings");
    assert_eq!(&filled.facets[..plate.len()], &plate[..], "the model is kept, untouched and first");
    assert!(filled.note(5.0).starts_with("land: 2 closed opening(s)"), "{}", filled.note(5.0));
    for pattern in [FinishPattern::Waterline, FinishPattern::Parallel] {
        let make = |f: &[Triangle]| match pattern {
            FinishPattern::Waterline => waterline(f, &fp(pattern)),
            _ => parallel(f, &fp(pattern)),
        };
        let land = make(&filled.facets).expect("land path");
        let dive = tips(&land).into_iter().filter(|q| in_hole(*q) && q.z < TOP - TOL).count();
        assert_eq!(dive, 0, "{pattern:?}: no wall dive with the fill on");
        // The verdict reads the real plate, not the lids.
        let bite = deepest_bite(&land, &plate);
        assert!(bite <= TOL, "{pattern:?}: the land path bites the real plate by {bite}");
        // And a real gouge under the land is still convicted on it.
        let sunk = deepest_bite(&sink(&land, 0.5), &plate);
        assert!(sunk > 0.4, "{pattern:?}: a sunk land path must be convicted, bite {sunk}");
    }
    // The raster crosses each mouth uninterrupted at the land, where
    // the plain finish breaks its lines over the holes.
    let over = |t: &Toolpath| {
        tips(t).into_iter().filter(|q| centre(*q) && (q.z - TOP).abs() <= TOL).count()
    };
    let on = parallel(&filled.facets, &fp(FinishPattern::Parallel)).expect("land path");
    let off = parallel(&plate, &fp(FinishPattern::Parallel)).expect("plain path");
    assert!(over(&on) > 0, "the land raster must fly over the mouths");
    assert_eq!(over(&off), 0, "the plain finish is the model's own surface");
}

#[test]
fn a_blind_pocket_is_lidded_and_a_gouge_into_its_floor_is_convicted() {
    let plate = pocketed_plate();
    let filled = fill_openings(&plate, 5.0, TOL).expect("fill");
    assert_eq!(filled.openings, 1);
    let land = parallel(&filled.facets, &fp(FinishPattern::Parallel)).expect("land path");
    assert!(deepest_bite(&land, &plate) <= TOL);
    // Off, the ball goes down to the pocket floor; on, it never does.
    let deep = |t: &Toolpath| tips(t).into_iter().any(|q| in_hole(q) && q.z < TOP - 1.0);
    let off = parallel(&plate, &fp(FinishPattern::Parallel)).expect("plain path");
    assert!(deep(&off), "the plain finish dives to the pocket floor");
    assert!(!deep(&land), "the land finish never descends into the pocket");
    // A plunge planted over the pocket down past its floor: the lid
    // would hide it from a drive-surface judge; the real one sees it.
    let mut planted = land.clone();
    let at = p3(10.0, 10.0, TOP);
    planted.moves.push(camcraft_motion::cl::Move::new(MoveKind::Feed, at, p3(10.0, 10.0, 3.0), Some(100.0)));
    assert!(deepest_bite(&planted, &plate) > 0.9, "the floor gouge must be convicted");
}

#[test]
fn an_opening_wider_than_the_width_or_open_to_the_edge_is_not_filled() {
    let plate = holed_plate();
    let narrow = fill_openings(&plate, 3.0, TOL).expect("fill");
    assert_eq!(narrow.openings, 0, "a 3 mm disk enters a 3.4 mm hole");
    assert_eq!(narrow.facets, plate, "nothing filled means the model alone");
    let slot = slotted_plate();
    let open = fill_openings(&slot, 10.0, TOL).expect("fill");
    assert_eq!(open.openings, 0, "a slot running out to the edge is open");
}

#[test]
fn nonsense_widths_are_refused_by_name() {
    let plate = holed_plate();
    for w in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let e = fill_openings(&plate, w, TOL).expect_err("refused");
        assert!(format!("{e:?}").contains("fill-openings width"), "{e:?}");
    }
    let e = fill_openings(&plate, 1e-4, TOL).expect_err("too fine");
    assert!(format!("{e:?}").contains("too fine"), "{e:?}");
    assert!(fill_openings(&[], 5.0, TOL).is_err());
}


/// #1196 review: Flow honours the lids. Its seeds and march ground
/// every column on the same ball support as Parallel (`plunge_support`
/// over every facet it is given) and its flanks ride the waterline
/// rings, so on the land drive it never descends into a lidded pocket,
/// unlike Pencil/Corner, whose crease walk the app refuses with the
/// fill on. Off, the same pass reaches the pocket floor. The pocket is
/// 8 mm square (floor z 4), wide enough for flow's lines to land on
/// its floor; the 10 mm width lids it.
#[test]
fn flow_honours_the_lids_and_never_descends_into_a_lidded_pocket() {
    use camcraft_motion::multiaxis::flow3d;
    let xs = [0.0, 6.0, 14.0, 40.0];
    let ys = [0.0, 6.0, 14.0, 20.0];
    let plate = block(&xs, &ys, &|i, j| Some(if i == 1 && j == 1 { 4.0 } else { TOP }));
    let filled = fill_openings(&plate, 10.0, TOL).expect("fill");
    assert_eq!(filled.openings, 1);
    let deep = |t: &Toolpath| {
        tips(t).into_iter().any(|q| (q.x - 10.0).abs() < 4.0 && (q.y - 10.0).abs() < 4.0 && q.z < TOP - 1.0)
    };
    let off = flow3d(&plate, &fp(FinishPattern::Flow)).expect("plain flow");
    assert!(deep(&off), "the plain flow reaches the pocket floor");
    let land = flow3d(&filled.facets, &fp(FinishPattern::Flow)).expect("land flow");
    assert!(!deep(&land), "the land flow never descends into the lidded pocket");
    assert!(deepest_bite(&land, &plate) <= TOL, "the land flow verifies on the real plate");
}

#[test]
fn the_note_says_so_when_nothing_was_filled() {
    let plate = holed_plate();
    let narrow = fill_openings(&plate, 3.0, TOL).expect("fill");
    assert_eq!(narrow.openings, 0);
    let n = narrow.note(3.0);
    assert!(n.contains("no closed opening narrower than 3 mm found") && !n.contains("flown over"), "{n}");
}

/// The land model's churn (#1286 follow-up): its roof is not flat - it
/// falls a little over every opening - so the closing's lid levels are
/// not equal across one lid. Emitting each cell its own level tiled
/// the lid as steps, and a ball crossing a step edge seats on neither
/// plane: support goes None there and every step costs a retract and
/// a plunge (166 feeds near the bolt holes on the report's model,
/// against 22 with the fill off). The lid of a closed opening must be
/// ONE plane: here a pocket under a shallow descending roof, whose
/// lid cells close a hair apart, must come out coplanar.
#[test]
fn a_pocket_under_a_descending_roof_gets_one_flat_lid() {
    let xs: Vec<f64> = (0..=20).map(|i| 2.0 * i as f64).collect();
    let ys = [0.0, 9.0, 11.0, 20.0];
    let plate = block(&xs, &ys, &|i, j| {
        Some(if i == 5 && j == 1 {
            4.0
        } else {
            8.0 - 0.01 * 2.0 * i as f64
        })
    });
    let filled = fill_openings(&plate, 5.0, TOL).expect("fill");
    assert_eq!(filled.openings, 1, "the pocket is closed under its roof, stepped or not");
    let mut zs: Vec<f64> = filled.facets[plate.len()..]
        .iter()
        .flat_map(|f| [f.a.z, f.b.z, f.c.z])
        .collect();
    zs.sort_by(|a, b| a.total_cmp(b));
    zs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    assert_eq!(zs.len(), 1, "one lid, one plane - the steps at {zs:?} are what churns the ball");
    assert!(zs[0] > 7.7 && zs[0] < TOP, "the plane rides the lid's highest closing, not below its roof and not above it, at {}", zs[0]);
    let land = parallel(&filled.facets, &fp(FinishPattern::Parallel)).expect("land path");
    assert!(
        !tips(&land).into_iter().any(|q| (q.x - 11.0).abs() < 1.0 && (q.y - 10.0).abs() < 1.0 && q.z < TOP - 1.0),
        "the land never descends into the pocket"
    );
    assert!(deepest_bite(&land, &plate) <= TOL, "the land verifies on the real roof");
}
