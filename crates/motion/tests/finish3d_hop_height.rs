//! #3627: the plain hop between two 3D finish runs. A run that ends HIGH
//! and a next run that starts LOW, with the retract plane between the
//! two, must never take the tool down at the end of the high run (the
//! tool rests on the surface there), and its move across must not pass
//! through the part. Fixture: the stepped pocket of `finish3d.rs` (top
//! annulus at z 10, floor at z 5), Contour pattern (outer passes first);
//! and a mesa (top at z 10 on a base at z 5) cut with `Pass starts at:
//! Inside` (#3362), where the inner pass is the high one.
//!
//! Gate map (M4.4): covers camcraft_motion::finishing3d, camcraft_motion::verify.

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::finishing3d::{contour, contour_from, waterline, Finish3DParams, FinishPattern, GapLink};
use camcraft_motion::geo2d::Region;
use camcraft_motion::geometry::{slice, Point3, Triangle};
use camcraft_motion::machining2d::PassStart;
use camcraft_motion::verify::{verify, Mode, ToolGeom, VerifyContext, Warning};

const R: f64 = 3.0;
const TOL: f64 = 0.1;
const TOP: f64 = 10.0;
const FLOOR: f64 = 5.0;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The stepped pocket of `finish3d.rs`: a 24 x 24 block, top at 10,
/// with a 12 x 12 pocket down to a floor at 5.
fn step_pocket() -> Vec<Triangle> {
    let quad = |p: [Point3; 4]| vec![Triangle::new(p[0], p[1], p[2]), Triangle::new(p[0], p[2], p[3])];
    let (t, f) = (TOP, FLOOR);
    let mut facets = Vec::new();
    facets.extend(quad([p3(-12.0, -12.0, t), p3(12.0, -12.0, t), p3(12.0, -6.0, t), p3(-12.0, -6.0, t)]));
    facets.extend(quad([p3(-12.0, 6.0, t), p3(12.0, 6.0, t), p3(12.0, 12.0, t), p3(-12.0, 12.0, t)]));
    facets.extend(quad([p3(-12.0, -6.0, t), p3(-6.0, -6.0, t), p3(-6.0, 6.0, t), p3(-12.0, 6.0, t)]));
    facets.extend(quad([p3(6.0, -6.0, t), p3(12.0, -6.0, t), p3(12.0, 6.0, t), p3(6.0, 6.0, t)]));
    facets.extend(quad([p3(6.0, -6.0, f), p3(-6.0, -6.0, f), p3(-6.0, -6.0, t), p3(6.0, -6.0, t)]));
    facets.extend(quad([p3(6.0, 6.0, f), p3(6.0, -6.0, f), p3(6.0, -6.0, t), p3(6.0, 6.0, t)]));
    facets.extend(quad([p3(-6.0, 6.0, f), p3(6.0, 6.0, f), p3(6.0, 6.0, t), p3(-6.0, 6.0, t)]));
    facets.extend(quad([p3(-6.0, -6.0, f), p3(-6.0, 6.0, f), p3(-6.0, 6.0, t), p3(-6.0, -6.0, t)]));
    facets.extend(quad([p3(-6.0, -6.0, f), p3(6.0, -6.0, f), p3(6.0, 6.0, f), p3(-6.0, 6.0, f)]));
    facets.extend(quad([p3(-12.0, -12.0, 0.0), p3(12.0, -12.0, 0.0), p3(12.0, -12.0, t), p3(-12.0, -12.0, t)]));
    facets.extend(quad([p3(12.0, -12.0, 0.0), p3(12.0, 12.0, 0.0), p3(12.0, 12.0, t), p3(12.0, -12.0, t)]));
    facets.extend(quad([p3(12.0, 12.0, 0.0), p3(-12.0, 12.0, 0.0), p3(-12.0, 12.0, t), p3(12.0, 12.0, t)]));
    facets.extend(quad([p3(-12.0, 12.0, 0.0), p3(-12.0, -12.0, 0.0), p3(-12.0, -12.0, t), p3(-12.0, 12.0, t)]));
    facets
}

fn params(retract_z: Option<f64>) -> Finish3DParams {
    Finish3DParams {
        gap_link: GapLink::Jump,
        pattern: FinishPattern::Contour,
        stepover: None,
        tool_diameter: 2.0 * R,
        tolerance: TOL,
        clearance_z: 25.0,
        retract_z,
        top_z: TOP,
        bottom_z: FLOOR,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
        waterlines: true,
    }
}

/// Each place where the path goes DOWN at the XY where a feed ended:
/// the previous move is a Feed (the tool rests on the surface at its
/// end) and the next move keeps that XY and loses Z. Returns the three
/// moves: the end of the run, the move down, and the move after it.
fn descents_at_run_end(t: &Toolpath) -> Vec<(Move, Move, Move)> {
    let m = &t.moves;
    let mut out = Vec::new();
    for i in 1..m.len().saturating_sub(1) {
        let (a, d) = (&m[i - 1], &m[i]);
        let same_xy = (a.to.x - d.to.x).hypot(a.to.y - d.to.y) < 1e-9;
        if a.kind == MoveKind::Feed && same_xy && d.to.z < a.to.z - 1e-6 && m[i + 1].kind == MoveKind::Rapid {
            out.push((*a, *d, m[i + 1]));
        }
    }
    out
}

/// Each Rapid that crosses XY over the block below its top. The block
/// is (-12..12)^2 at z 10; the mouth of the pocket (-6..6)^2 is open
/// down to the floor. A rapid in the mouth, a vertical rapid, or a
/// rapid outside the block's footprint is not reported.
fn rapids_through_block(t: &Toolpath) -> Vec<(Point3, Point3)> {
    let over_block = |x: f64, y: f64| {
        let on_block = x.abs() < 12.0 + R && y.abs() < 12.0 + R;
        let in_mouth = x.abs() < 6.0 - R + 1e-6 && y.abs() < 6.0 - R + 1e-6;
        on_block && !in_mouth
    };
    let mut out = Vec::new();
    for w in t.moves.windows(2) {
        let (a, b) = (w[0].to, w[1].to);
        let vertical = (a.x - b.x).hypot(a.y - b.y) < 1e-9;
        if w[1].kind != MoveKind::Rapid || vertical || a.z.min(b.z) >= TOP - 1e-6 {
            continue;
        }
        let crosses = (0..=16).any(|k| {
            let s = f64::from(k) / 16.0;
            over_block(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s)
        });
        if crosses {
            out.push((a, b));
        }
    }
    out
}

fn verify_finish(facets: &[Triangle], t: &Toolpath) -> Vec<Warning> {
    verify_at(facets, t, TOP)
}

fn verify_at(facets: &[Triangle], t: &Toolpath, stock_top: f64) -> Vec<Warning> {
    let region: Region = slice::project(facets, 64).expect("fixture silhouette");
    let tool = ToolGeom {
        radius: R,
        nose_radius: 0.0,
        flute_len: 10.0,
        shank_radius: 2.9,
        shank_len: 40.0,
        holder_radius: 4.9,
        holder_len: 30.0,
    };
    let cx = VerifyContext {
        region: &region,
        stock: None,
        model: Some(facets),
        tool: &tool,
        cutter: None,
        mode: Mode::Envelope,
        stock_top_z: stock_top,
        clearance_z: 25.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: Some(TOL),
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(t, &cx).into_iter().filter(|w| w.blocks_post()).collect()
}

/// The fault case: retract at 7, between the floor (5) and the top
/// (10). The hop from the last pass on the top to the first pass on
/// the floor must not go down at the top, and must not cross the block
/// below its top.
#[test]
fn a_retract_below_the_high_run_never_takes_the_tool_into_the_part() {
    let f = step_pocket();
    let t = contour(&f, &params(Some(7.0))).expect("pocket contour");
    let downs = descents_at_run_end(&t);
    for (a, d, n) in &downs {
        eprintln!("run end {:?} -> {:?} {:?} -> {:?} {:?}", a.to, d.kind, d.to, n.kind, n.to);
    }
    let through = rapids_through_block(&t);
    for (a, b) in &through {
        eprintln!("rapid below the top through the block: {a:?} -> {b:?}");
    }
    // The hop from the last top pass to the first floor pass: the move
    // after the run end at (7.352421, -7.352421, 10) (#3627's reading).
    if let Some(i) = t.moves.iter().position(|m| m.kind == MoveKind::Feed && (m.to.x - 7.352421).abs() < 1e-6 && (m.to.y + 7.352421).abs() < 1e-6) {
        for m in t.moves.iter().skip(i).take(4) {
            eprintln!("hop: {:?} {:?}", m.kind, m.to);
        }
    }
    let blocking = verify_finish(&f, &t);
    assert_eq!(blocking, Vec::new(), "the verifier finds the hop");
    assert!(
        downs.iter().all(|(a, d, _)| a.to.z < TOP - 1e-6 || d.to.z >= TOP - 1e-6),
        "the hop takes the tool down at the end of a run on the top: {downs:?}"
    );
    assert_eq!(through, Vec::new(), "a rapid crosses the block below its top");
}

/// The usual setup: no retract plane (the hop goes to clearance) and a
/// retract plane above the part. Both are clean.
#[test]
fn a_retract_above_the_part_hops_clean() {
    let f = step_pocket();
    for rz in [None, Some(15.0)] {
        let t = contour(&f, &params(rz)).expect("pocket contour");
        assert!(
            descents_at_run_end(&t).iter().all(|(a, d, _)| d.to.z >= a.to.z - 1e-6),
            "retract {rz:?}: a descent at a run end"
        );
        assert_eq!(rapids_through_block(&t), Vec::new(), "retract {rz:?}");
        assert_eq!(verify_finish(&f, &t), Vec::new(), "retract {rz:?}: the verifier agrees");
    }
}

/// A convex part: a mesa, top at 10 over (-4..4)^2, on a base at 5 over
/// (-12..12)^2, outer walls down to 0.
fn mesa() -> Vec<Triangle> {
    let quad = |p: [Point3; 4]| vec![Triangle::new(p[0], p[1], p[2]), Triangle::new(p[0], p[2], p[3])];
    let (t, b) = (TOP, FLOOR);
    let mut f = Vec::new();
    f.extend(quad([p3(-4.0, -4.0, t), p3(4.0, -4.0, t), p3(4.0, 4.0, t), p3(-4.0, 4.0, t)]));
    f.extend(quad([p3(-4.0, -4.0, b), p3(4.0, -4.0, b), p3(4.0, -4.0, t), p3(-4.0, -4.0, t)]));
    f.extend(quad([p3(4.0, -4.0, b), p3(4.0, 4.0, b), p3(4.0, 4.0, t), p3(4.0, -4.0, t)]));
    f.extend(quad([p3(4.0, 4.0, b), p3(-4.0, 4.0, b), p3(-4.0, 4.0, t), p3(4.0, 4.0, t)]));
    f.extend(quad([p3(-4.0, 4.0, b), p3(-4.0, -4.0, b), p3(-4.0, -4.0, t), p3(-4.0, 4.0, t)]));
    f.extend(quad([p3(-12.0, -12.0, b), p3(12.0, -12.0, b), p3(12.0, -4.0, b), p3(-12.0, -4.0, b)]));
    f.extend(quad([p3(-12.0, 4.0, b), p3(12.0, 4.0, b), p3(12.0, 12.0, b), p3(-12.0, 12.0, b)]));
    f.extend(quad([p3(-12.0, -4.0, b), p3(-4.0, -4.0, b), p3(-4.0, 4.0, b), p3(-12.0, 4.0, b)]));
    f.extend(quad([p3(4.0, -4.0, b), p3(12.0, -4.0, b), p3(12.0, 4.0, b), p3(4.0, 4.0, b)]));
    f.extend(quad([p3(-12.0, -12.0, 0.0), p3(12.0, -12.0, 0.0), p3(12.0, -12.0, b), p3(-12.0, -12.0, b)]));
    f.extend(quad([p3(12.0, -12.0, 0.0), p3(12.0, 12.0, 0.0), p3(12.0, 12.0, b), p3(12.0, -12.0, b)]));
    f.extend(quad([p3(12.0, 12.0, 0.0), p3(-12.0, 12.0, 0.0), p3(-12.0, 12.0, b), p3(12.0, 12.0, b)]));
    f.extend(quad([p3(-12.0, 12.0, 0.0), p3(-12.0, -12.0, 0.0), p3(-12.0, -12.0, b), p3(-12.0, 12.0, b)]));
    f
}

/// Each Rapid that crosses XY over the mesa (-4..4)^2, padded by the
/// ball radius, below its top.
fn rapids_through_mesa(t: &Toolpath) -> Vec<(Point3, Point3)> {
    let mut out = Vec::new();
    for w in t.moves.windows(2) {
        let (a, b) = (w[0].to, w[1].to);
        let vertical = (a.x - b.x).hypot(a.y - b.y) < 1e-9;
        if w[1].kind != MoveKind::Rapid || vertical || a.z.min(b.z) >= TOP - 1e-6 {
            continue;
        }
        let crosses = (0..=16).any(|k| {
            let s = f64::from(k) / 16.0;
            let (x, y) = (a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s);
            x.abs() < 4.0 + R - 1e-6 && y.abs() < 4.0 + R - 1e-6
        });
        if crosses {
            out.push((a, b));
        }
    }
    out
}

/// `Pass starts at: Inside` (#3362) on a convex part: the inner pass on
/// the mesa top (z 10) is cut first, the next pass on the base (z 5) is
/// lower. A retract at 7 is below the inner pass. The hop must not go
/// down at the mesa top, nor cross the mesa below its top.
#[test]
fn inside_first_on_a_convex_part_with_a_low_retract_never_digs_in() {
    let f = mesa();
    let t = contour_from(&f, &params(Some(7.0)), PassStart::Inside).expect("mesa contour");
    let downs = descents_at_run_end(&t);
    assert!(
        downs.iter().all(|(a, d, _)| a.to.z < TOP - 1e-6 || d.to.z >= TOP - 1e-6),
        "the hop takes the tool down at the end of a run on the mesa top: {downs:?}"
    );
    assert_eq!(rapids_through_mesa(&t), Vec::new(), "a rapid crosses the mesa below its top");
    // No gouge. The verifier still holds the post for the hops between
    // the base passes at the retract plane (z 7): it reads the stock as
    // uncut up to its top (z 10), so a rapid below that is "in stock".
    // That is its own conservative law, not this hop: each such rapid
    // is above the base and off the mesa.
    for w in verify_at(&f, &t, TOP) {
        let Warning::RapidInStock { at } = w else {
            panic!("blocking finding other than a rapid in stock: {w:?}");
        };
        let (a, b) = (t.moves[at - 1].to, t.moves[at].to);
        let off_mesa = |q: Point3| q.x.abs() >= 4.0 + R - 1e-6 || q.y.abs() >= 4.0 + R - 1e-6;
        assert!(
            a.z >= FLOOR && b.z >= FLOOR && off_mesa(a) && off_mesa(b),
            "rapid in stock {at}: {a:?} -> {b:?} is not a base hop"
        );
    }
}

/// #3633: two staggered bosses of different sizes set the order and
/// direction of the link between the two rings on the Z5 level.
fn waterline_bosses() -> Vec<Triangle> {
    let mut facets = Vec::new();
    for (xmin, xmax, ymin, ymax) in [(-19.0, -5.0, -16.0, -4.0), (7.0, 17.0, 4.0, 16.0)] {
        let corners = [
            p3(xmin, ymin, 0.0),
            p3(xmax, ymin, 0.0),
            p3(xmax, ymax, 0.0),
            p3(xmin, ymax, 0.0),
            p3(xmin, ymin, TOP),
            p3(xmax, ymin, TOP),
            p3(xmax, ymax, TOP),
            p3(xmin, ymax, TOP),
        ];
        for [a, b, c, d] in [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]] {
            facets.push(Triangle::new(corners[a], corners[b], corners[c]));
            facets.push(Triangle::new(corners[a], corners[c], corners[d]));
        }
    }
    facets
}

fn waterline_params(retract: f64) -> Finish3DParams {
    let mut p = params(Some(retract));
    p.pattern = FinishPattern::Waterline;
    p.top_z = FLOOR;
    // Keep both rings closed so the verifier sees just the inter-boss rapid.
    p.tolerance = 0.06;
    p
}

#[test]
fn waterline_ring_hop_clears_the_boss_and_keeps_a_safe_retract() {
    let facets = waterline_bosses();
    let clearance = waterline_params(7.0).clearance_z;
    let mut chords = Vec::new();
    for (retract, expected_z) in [(7.0, clearance), (13.0, 13.0)] {
        let path = waterline(&facets, &waterline_params(retract)).expect("two waterline rings");
        assert_eq!(verify_finish(&facets, &path), Vec::new(), "retract Z{retract}");
        let hops: Vec<_> = path
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Rapid && m.from.x < -1.0 && m.to.x > 1.0)
            .collect();
        assert_eq!(hops.len(), 1, "one rapid between two rings");
        let (a, b) = (hops[0].from, hops[0].to);
        assert_eq!((a.z, b.z), (expected_z, expected_z), "retract Z{retract}");
        chords.push((a.x, a.y, b.x, b.y));
    }
    assert_eq!(chords[0], chords[1], "only the height of the link changes");
}

#[test]
fn waterline_keeps_a_low_hop_that_clears_the_model() {
    let mut facets = waterline_bosses();
    let p = waterline_params(13.0);
    let baseline = waterline(&facets, &p).expect("two waterline rings");
    // This facet raises the model top above the retract plane without
    // reaching the ring level or the link between the bosses.
    facets.push(Triangle::new(p3(80.0, 80.0, 20.0), p3(90.0, 80.0, 20.0), p3(80.0, 90.0, 20.0)));
    let path = waterline(&facets, &p).expect("two waterline rings below the distant facet");
    assert_eq!(path.moves, baseline.moves);
}
