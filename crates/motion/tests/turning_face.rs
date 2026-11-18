//! M5.2 stage A turning law tests: the facing ladder. What "right"
//! means here: every rung's cutting pass STARTS outside the skin -
//! including a rung whose station window holds no vertex, which is
//! the case that leaves silent stubble above the facing plane when
//! the band is read off window-filtered vertices (the wall's own
//! triangles lean through it; the reach sweep asks them) - each
//! pass runs through the axis, the top rung comes first, no ball
//! center ever stands below its plane's tangency, and every refusal
//! law names itself instead of emitting a path: a point nose, an
//! infeed past the nose's reach, a facing plane at or above the
//! work, an axis lying on the table, corrupt numbers, a rung-count
//! that would hang.
//!
//! Gate map (M4.4): covers camcraft_motion::turning (face kernel,
//! frame fit, reach sweep and refusals) with this crate's own
//! revolve fixture.
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::MoveKind;
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::GenError;
use camcraft_motion::turning::{facing, FacingParams};

const TAU: f64 = std::f64::consts::TAU;
const NOSE: f64 = 0.8;
const R: f64 = 5.0; // the drum's wall radius
const FACE_Z: f64 = 8.0; // the plane facing leaves
const STEP: f64 = 0.5;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The closed-solid revolve the rotary suites use (their fixture,
/// verbatim - diagonal split and all): meridian (rho, z) ascending,
/// swept about Z.
fn revolve(n_circ: usize, meridian: &[(f64, f64)]) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / n_circ as f64;
        p3(r * th.cos(), r * th.sin(), z)
    };
    let mut t = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..n_circ {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                t.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                t.push(Triangle::new(a1, a2, b2));
            } else {
                t.push(Triangle::new(a1, a2, b2));
                t.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    t
}

/// A standing drum: r 5, z -10..10, capped both ends. Its wall's
/// triangles span the whole 20 mm with their ONLY vertices on the
/// two rim rings - the exact shape a vertex-filtered band misses
/// and the reach sweep cannot.
fn drum() -> Vec<Triangle> {
    revolve(16, &[(0.0, -10.0), (R, -10.0), (R, 10.0), (0.0, 10.0)])
}

fn params(facing_z: f64, stepdown: f64, nose: f64) -> FacingParams {
    FacingParams {
        clearance_z: 20.0,
        top_z: 12.0, // claimed above the real top: the ladder stops at 10
        facing_z,
        stepdown,
        feed: 150.0,
        plunge_feed: 80.0,
        nose_radius: nose,
    }
}

fn rho(q: Point3) -> f64 {
    (q.x * q.x + q.y * q.y).sqrt()
}

/// The station a CL point rides. The drum fits EXACTLY onto world Z
/// (symmetric mesh: origin at the centroid, dir upright), so the
/// station is the height here - the fixture's own privilege, not a
/// general reading.
fn station(q: Point3) -> f64 {
    q.z
}

#[test]
fn the_ladder_walks_the_wall_at_every_rung() {
    // Facing 2 mm off the drum's top: four rungs, stations 8.0
    // through 9.5 by 0.5 (#2004: no rung AT the top 10.0 - its ball
    // band stands wholly above the work, a pass through air). The rung at 8.0 has no vertex inside its
    // window [8.0, 9.6] - only the wall's triangles lean through
    // it - so THIS is the case the old vertex filter stubbled: its
    // pass would start at nose off the axis and cut nothing beyond
    // rho 1.6, leaving an annulus standing above the facing plane.
    // The law as pinned: every pass ENDS on the axis (rho 0) and
    // STARTS outside the skin - rho at least R + NOSE, the tangency
    // of the ball with the wall its window covers.
    let t = facing(&drum(), None, &params(FACE_Z, STEP, NOSE)).expect("a drum faces");
    let mut rungs: Vec<(f64, f64)> = Vec::new(); // (station, start rho)
    for m in &t.moves {
        if m.kind == MoveKind::Feed && rho(m.to) < 1e-9 {
            let s = station(m.to) - NOSE;
            if !rungs.iter().any(|r| (r.0 - s).abs() < 1e-9) {
                rungs.push((s, rho(m.from)));
            }
        }
    }
    assert_eq!(rungs.len(), 4, "four passes end on the axis: {rungs:?}");
    for (k, (got, start)) in rungs.iter().enumerate() {
        let want = FACE_Z + (3 - k) as f64 * STEP; // top rung first
        assert!(
            (got - want).abs() < 1e-9,
            "rung {k}: pass at station {got}, ladder order broke: {rungs:?}"
        );
        assert!(
            *start > R + NOSE - 1e-9,
            "rung at station {got} started at rho {start}: past a skin it never crossed - stubble"
        );
    }
    // And the program is bounded: nothing stands below the facing
    // plane's tangency, nothing above the rapids plane's.
    for m in &t.moves {
        for q in [m.from, m.to] {
            assert!(
                station(q) >= FACE_Z + NOSE - 1e-9 && station(q) <= 20.0 + NOSE + 1e-9,
                "a CL point outside the ladder's own heights: {:?}",
                q
            );
        }
    }
}

#[test]
fn a_point_cannot_face() {
    let err = facing(&drum(), None, &params(FACE_Z, STEP, 0.0)).expect_err("nose 0 must refuse");
    assert!(format!("{err:?}").contains("cannot face"), "{err:?}");
}

#[test]
fn a_stepdown_past_the_nose_reaches_refused() {
    // The ridge law: one infeed of twice the nose already closes
    // the window; past it a sheet stands between rungs, and the
    // kernel says so at the door instead of posting a lie.
    let err = facing(&drum(), None, &params(FACE_Z, 2.0, NOSE)).expect_err("2.0 > 2*0.8");
    assert!(format!("{err:?}").contains("stepdown"), "{err:?}");
    // Exactly twice the nose is the last honest infeed: accepted.
    assert!(facing(&drum(), None, &params(FACE_Z, 1.6, NOSE)).is_ok(), "2r is lawful");
}

#[test]
fn nothing_above_the_plane_is_empty_not_wrong() {
    // The facing plane AT the work's top, and above it: an honest
    // nothing-to-cut is a verdict, never an Ok path that cuts air.
    let t = drum();
    let err = facing(&t, None, &params(10.0, STEP, NOSE)).expect_err("plane at the top");
    assert!(matches!(err, GenError::Empty(_)), "{err:?}");
    assert!(format!("{err:?}").contains("nothing to face"), "{err:?}");
    assert!(facing(&t, None, &params(12.0, STEP, NOSE)).is_err(), "plane above the top");
}

#[test]
fn an_axis_lying_on_the_table_is_refused_by_name() {
    // The drum laid down on x: turning heights ARE stations along
    // the axis, and a lying axis makes the parameters meaningless -
    // the rotary kinds own that posture, and say so.
    let t: Vec<Triangle> = drum()
        .into_iter()
        .map(|tr| {
            let s = |q: Point3| p3(q.z, q.x, q.y);
            Triangle::new(s(tr.a), s(tr.b), s(tr.c))
        })
        .collect();
    let err = facing(&t, None, &params(FACE_Z, STEP, NOSE)).expect_err("a lying axis must refuse");
    assert!(format!("{err:?}").contains("off vertical"), "{err:?}");
}

#[test]
fn corrupt_numbers_die_at_the_door() {
    // Degenerate input must error, never crash or hang: a nan
    // height is refused before a single rung is planned, an empty
    // mesh says so, and a ladder of a few tens of millions of
    // rungs is refused as the absurdity it is (the MAX_RUNGS cap),
    // not planned.
    let mut p = params(FACE_Z, STEP, NOSE);
    p.facing_z = f64::NAN;
    assert!(facing(&drum(), None, &p).is_err(), "a nan height must not reach the ladder");
    let p = params(FACE_Z, 0.0, NOSE);
    assert!(facing(&drum(), None, &p).is_err(), "a zero infeed is not a ladder");
    assert!(facing(&[], None, &p).is_err(), "no facets to face");
    let err = facing(&drum(), None, &params(FACE_Z, 1e-8, 0.8))
        .expect_err("the rung cap must fire before any loop runs");
    assert!(format!("{err:?}").contains("ladder"), "{err:?}");
    // Non-finite vertices too: profile_of's gate, before any reading.
    let bad = vec![
        Triangle::new(p3(0.0, 0.0, 10.0), p3(f64::NAN, 0.0, -10.0), p3(0.0, 0.0, -10.0)),
    ];
    assert!(facing(&bad, None, &params(FACE_Z, STEP, NOSE)).is_err(), "a corrupt mesh is named, not walked");
}

#[test]
fn a_stock_allowance_above_the_finished_part_is_faced_off() {
    // #122: the part sits at its finished length (top Z 10) and the
    // setup's bar is longer (to Z 12) and wider (r 7). Facing to the
    // part's end must climb the stock's allowance - four rungs, 10.0
    // through 11.5 (#2004: none at the bar's end 12.0) - with every pass starting outside the BAR's skin
    // and every rapid outside it too; without the stock the same plane
    // is honestly empty.
    let part = drum();
    let bar = revolve(16, &[(0.0, -10.0), (7.0, -10.0), (7.0, 12.0), (0.0, 12.0)]);
    let p = params(10.0, STEP, NOSE); // top_z 12: the stock's end
    assert!(matches!(facing(&part, None, &p), Err(GenError::Empty(_))), "no stock: nothing to face");
    let t = facing(&part, Some(&bar), &p).expect("the allowance faces");
    let mut rungs: Vec<(f64, f64)> = Vec::new();
    for m in &t.moves {
        if m.kind == MoveKind::Feed && rho(m.to) < 1e-9 {
            rungs.push((station(m.to) - NOSE, rho(m.from)));
        }
    }
    assert_eq!(rungs.len(), 4, "four passes across the 2 mm allowance: {rungs:?}");
    for (k, (s, start)) in rungs.iter().enumerate() {
        let want = 10.0 + (3 - k) as f64 * STEP;
        assert!((s - want).abs() < 1e-9, "rung {k} at {s}, want {want}: {rungs:?}");
        assert!(*start > 7.0 + NOSE - 1e-9, "rung at {s} started inside the bar (rho {start})");
    }
    // Every rapid ends outside the bar's radius, or is the lift off a
    // face just cut (#632): straight up the axis from the pass's end,
    // the nose rising into air the ladder already cleared.
    let mut cut: Option<f64> = None; // the station of the last pass
    for m in &t.moves {
        if m.kind == MoveKind::Feed && rho(m.to) < 1e-9 {
            cut = Some(station(m.to) - NOSE);
        }
        if m.kind == MoveKind::Rapid && rho(m.to) <= 7.0 + NOSE - 1e-9 {
            let s = cut.expect("no rapid inside the bar before the first pass");
            assert!(
                rho(m.from) < 1e-9 && station(m.to) - NOSE > s + 1e-9,
                "a rapid inside the bar's radius that is not a lift off its own face: {m:?}"
            );
        }
        assert!(station(m.to) >= 10.0 + NOSE - 1e-9, "a CL point below the finished end: {m:?}");
    }
}

#[test]
fn the_first_move_is_a_rapid_to_the_clearance_point() {
    // #632: the program opens with a commanded RAPID to the clearance
    // point - outside the whole work, on the rapids plane - before any
    // feed. It used to open with the plunge FEED, posted as a G1 from
    // wherever the turret stood: an unplanned, unjudged path.
    let t = facing(&drum(), None, &params(FACE_Z, STEP, NOSE)).expect("a drum faces");
    let m0 = t.moves.first().expect("a path");
    assert_eq!(m0.kind, MoveKind::Rapid, "the first move is a rapid: {m0:?}");
    assert!((station(m0.to) - (20.0 + NOSE)).abs() < 1e-9, "on the clearance plane: {m0:?}");
    assert!(rho(m0.to) > R + NOSE, "outside the work's radius: {m0:?}");
    assert!((station(m0.from) - station(m0.to)).abs() < 1e-9, "travelling along the clearance plane: {m0:?}");
    let feed = t.moves.iter().position(|m| m.kind == MoveKind::Feed).expect("a cut");
    assert_eq!(feed, 1, "the plunge follows the approach directly");
    assert!((rho(t.moves[1].from) - rho(m0.to)).abs() < 1e-9, "the plunge starts at the clearance point");
}

#[test]
fn every_pass_lifts_off_its_face_before_it_rapids_out() {
    // #632: each pass ends on the axis; the next move LIFTS the nose
    // along the axis (rho stays 0, station rises by a small, positive
    // amount no taller than the stepdown) and only then does a rapid
    // carry it out radially - at the lifted station, never at the
    // station just cut, where it dragged the nose across the fresh face.
    for (step, nose) in [(STEP, NOSE), (0.1, 0.4), (1.6, NOSE)] {
        let t = facing(&drum(), None, &params(FACE_Z, step, nose)).expect("a drum faces");
        let mut passes = 0;
        for (i, m) in t.moves.iter().enumerate() {
            if !(m.kind == MoveKind::Feed && rho(m.to) < 1e-9) {
                continue;
            }
            passes += 1;
            let s = station(m.to);
            let (up, out) = (&t.moves[i + 1], &t.moves[i + 2]);
            assert_eq!(up.kind, MoveKind::Rapid, "step {step}: the lift: {up:?}");
            assert!(rho(up.to) < 1e-9, "step {step}: the lift stays on the axis: {up:?}");
            let lift = station(up.to) - s;
            assert!(lift > 1e-6 && lift <= step + 1e-9, "step {step}: lift {lift} off the face");
            assert_eq!(out.kind, MoveKind::Rapid, "step {step}: the radial exit: {out:?}");
            assert!((station(out.to) - station(up.to)).abs() < 1e-9, "step {step}: out at the lifted station: {out:?}");
            assert!(rho(out.to) > R + nose, "step {step}: out past the work: {out:?}");
        }
        assert!(passes > 0, "step {step}: passes were walked");
        // No move slides radially along a station a pass cut, except the
        // pass itself (and its feed in).
        for m in &t.moves {
            if m.kind == MoveKind::Rapid && (station(m.from) - station(m.to)).abs() < 1e-9 && rho(m.from) < rho(m.to) {
                let s = station(m.to) - nose;
                let on_a_face = t
                    .moves
                    .iter()
                    .any(|c| c.kind == MoveKind::Feed && rho(c.to) < 1e-9 && (station(c.to) - nose - s).abs() < 1e-9);
                assert!(!on_a_face, "step {step}: a rapid out along a cut face: {m:?}");
            }
        }
    }
}

/// The ladder's passes, top first: the station of every pass that ends
/// on the axis.
fn pass_stations(t: &camcraft_motion::cl::Toolpath) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();
    for m in &t.moves {
        if m.kind == MoveKind::Feed && rho(m.to) < 1e-9 {
            let s = station(m.to) - NOSE;
            if !out.iter().any(|q| (q - s).abs() < 1e-9) {
                out.push(s);
            }
        }
    }
    out
}

#[test]
fn no_rung_faces_air_at_the_top_of_the_work_but_a_low_top_z_keeps_its_rung() {
    // #2004: the drum's top is 10. A span that is no exact multiple of
    // the stepdown keeps every rung (8.0 .. 9.5 under a top of 9.7);
    // an exact multiple drops only the rung AT the top.
    let t = facing(&drum(), None, &params(FACE_Z, STEP, NOSE)).expect("faces");
    assert_eq!(pass_stations(&t), vec![9.5, 9.0, 8.5, 8.0]);
    let odd = FacingParams { top_z: 9.7, ..params(FACE_Z, STEP, NOSE) };
    assert_eq!(pass_stations(&facing(&drum(), None, &odd).expect("faces")), vec![9.5, 9.0, 8.5, 8.0]);
    // A Top Z of 9 BELOW the work's top: material stands over the rung
    // at 9, so it is a real cut and stays.
    let low = FacingParams { top_z: 9.0, ..params(FACE_Z, STEP, NOSE) };
    assert_eq!(pass_stations(&facing(&drum(), None, &low).expect("faces")), vec![9.0, 8.5, 8.0]);
}
