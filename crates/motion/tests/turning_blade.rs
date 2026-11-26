//! M5.2 stage C turning law tests: the blade kernels and the meridian
//! verdict they stand on. What "right" means here: a flush groove
//! leaves the judge NOTHING to say (the swept boxcar equals the model
//! band by band); a deeper cut than the model keeps is a gouge of
//! exactly the difference; a rough's own leave is silent when the
//! budget carries it and a rest when it does not; a missed pass is
//! charged the tall claim of its window - over-reported on purpose;
//! and a parting kerf is a cut, never a gouge - what parting means
//! to remove stands judged silent. The kernels themselves: centers tile
//! by the full width, the ends reach the band edges, rapids ride the
//! WHOLE bar's widest radius (a mid-bar groove under a flange may
//! not drag the blade through the flange), and every refusal names
//! its own number.
//!
//! Gate map (M4.4): covers camcraft_motion::turning (revolve
//! blade_warnings/scan laws, blade kernels, setup refusals).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::{MoveKind, PathBuilder};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::GenError;
use camcraft_motion::turning::profile::Frame;
use camcraft_motion::turning::{
    blade_program_warnings, blade_warnings, groove_finish, groove_rough, part, BladeLeg,
    GrooveFinishParams, GrooveRoughParams, PartParams,
};
use camcraft_motion::verify::Warning;

const TAU: f64 = std::f64::consts::TAU;
const TOL: f64 = 0.05;
const WIDTH: f64 = 2.0;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The closed-solid revolve the turning suites share.
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

/// A plain wall: r 6 from z -16 to 40, capped both ends. The judge's
/// zero: a kerf or band on this model is either clean or convicted.
fn wall() -> Vec<Triangle> {
    revolve(16, &[(0.0, -16.0), (6.0, -16.0), (6.0, 40.0), (0.0, 40.0)])
}

/// The wall with a finished groove: 2.4 wide, floor r 4, stations
/// -6.0 .. -3.6. Wider than the blade, narrower than two - the
/// center sweep must land exactly two plunges, end kerfs flush.
fn grooved() -> Vec<Triangle> {
    revolve(
        16,
        &[
            (0.0, -16.0),
            (6.0, -16.0),
            (6.0, -6.0),
            (4.0, -6.0),
            (4.0, -3.6),
            (6.0, -3.6),
            (6.0, 40.0),
            (0.0, 40.0),
        ],
    )
}

/// A flange over a neck: r 6 to z 2, r 3 to z 20. The window for
/// the blade tests sits on the NECK, so the whole-bar clearance
/// law has a shoulder to prove itself against.
fn flange() -> Vec<Triangle> {
    revolve(16, &[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (3.0, 2.0), (3.0, 20.0), (0.0, 20.0)])
}

/// The (station, radius) a CL point rides, read in the model's own frame.
fn sr(f: &Frame, q: Point3) -> (f64, f64) {
    f.sr(q)
}

/// The smallest rapid radius of a path - the clearance the rapids
/// actually ride (the whole-bar promise to check).
fn rapid_floor(f: &Frame, t: &camcraft_motion::cl::Toolpath) -> f64 {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid)
        .fold(f64::INFINITY, |acc, m| acc.min(sr(f, m.to).1))
}

fn groove_fin(top_z: f64, bottom_z: f64) -> GrooveFinishParams {
    GrooveFinishParams {
        tolerance: TOL,
        clearance_z: 44.0,
        top_z,
        bottom_z,
        feed: 0.0,
        plunge_feed: 60.0,
        blade_width: WIDTH,
    }
}

// ------------------------------------------------------- the verdict

#[test]
fn a_flush_groove_leaves_the_judge_silent() {
    // The kernel's own finish path, judged against the very model
    // it was computed from: band equals claim everywhere, so the
    // verdict has nothing to say. This is the test the whole law
    // is calibrated against - false positives here would grey
    // every honest groove in the app.
    let t = groove_finish(&grooved(), &groove_fin(-3.6, -6.0)).expect("the groove finishes");
    let warns = blade_warnings(&grooved(), &t, WIDTH, TOL, 0.0);
    assert!(warns.is_empty(), "a flush cut reported: {warns:?}");
}

#[test]
fn a_deeper_cut_than_the_model_keeps_is_a_gouge() {
    // The kernel finishes the DEEP model (floor 4.0); the judge
    // reads the shallow one (floor 4.5 - the same groove 0.5 shy
    // of the depth the blade actually reached). C < E by 0.5 at
    // every band station: a gouge of exactly the difference.
    let deep = grooved();
    let shallow = revolve(
        16,
        &[
            (0.0, -16.0),
            (6.0, -16.0),
            (6.0, -6.0),
            (4.5, -6.0),
            (4.5, -3.6),
            (6.0, -3.6),
            (6.0, 40.0),
            (0.0, 40.0),
        ],
    );
    let t = groove_finish(&deep, &groove_fin(-3.6, -6.0)).expect("the deep groove finishes");
    let warns = blade_warnings(&shallow, &t, WIDTH, TOL, 0.0);
    match warns.as_slice() {
        [Warning::Envelope { gouge_mm, rest_mm, .. }] => {
            assert!(*gouge_mm > 0.49 && *gouge_mm < 0.51, "gouge size: {warns:?}");
            assert!(*rest_mm <= 0.0, "a gouge must not also claim rest: {warns:?}");
        }
        other => panic!("expected one gouge verdict, got {other:?}"),
    }
}

#[test]
fn the_roughs_own_leave_is_silent_then_a_rest() {
    // One path, two budgets: the rough leaves 0.25 on the floor;
    // judged WITH the leave in the budget it says nothing (that
    // skin is the finish's appointment, not a defect); judged as
    // if leave-free, it is a rest of exactly 0.25.
    let t = groove_rough(
        &grooved(),
        &GrooveRoughParams {
            tolerance: TOL,
            clearance_z: 44.0,
            top_z: -3.6,
            bottom_z: -6.0,
            stepdown: 0.5,
            stock_to_leave: 0.25,
            feed: 0.0,
            plunge_feed: 60.0,
            blade_width: WIDTH,
        },
    )
    .expect("the groove roughs");
    let warns = blade_warnings(&grooved(), &t, WIDTH, TOL, 0.25);
    assert!(warns.is_empty(), "the leave was in the budget: {warns:?}");
    let warns = blade_warnings(&grooved(), &t, WIDTH, TOL, 0.0);
    match warns.as_slice() {
        [Warning::Envelope { gouge_mm, rest_mm, .. }] => {
            assert!(*gouge_mm <= 0.0, "a rough that left stock did not gouge: {warns:?}");
            assert!(*rest_mm > 0.24 && *rest_mm < 0.26, "rest size: {warns:?}");
        }
        other => panic!("expected one rest verdict, got {other:?}"),
    }
}

#[test]
fn the_program_scan_judges_the_part_not_the_passes() {
    // The composition law (M5.3 D): a probe measures what the
    // blades cut TOGETHER. The rough from the test above stands
    // 0.25 off the model's floor, and the scan convicts it alone
    // exactly once, blaming a move inside the rough's own
    // program range - but composed with the finish that answers
    // for it, the union floor is the model's and the scan says
    // nothing. A parting leg exempts its own band as kerf in the
    // composition too, and nothing else: a clean program measures
    // 0 at any ask, which is only an honest silence because the
    // same gate just convicted a program that was not clean.
    let rough = groove_rough(
        &grooved(),
        &GrooveRoughParams {
            tolerance: TOL,
            clearance_z: 44.0,
            top_z: -3.6,
            bottom_z: -6.0,
            stepdown: 0.5,
            stock_to_leave: 0.25,
            feed: 0.0,
            plunge_feed: 60.0,
            blade_width: WIDTH,
        },
    )
    .expect("the groove roughs");
    let fin = groove_finish(&grooved(), &groove_fin(-3.6, -6.0)).expect("the groove finishes");
    let sever = part(
        &grooved(),
        &PartParams {
            tolerance: TOL,
            clearance_z: 44.0,
            top_z: 40.0,
            parting_z: -10.0,
            stepdown: 1.5,
            feed: 0.0,
            plunge_feed: 60.0,
            blade_width: WIDTH,
            kerf: Default::default(),
        },
    )
    .expect("the bar parts");
    fn leg(t: &camcraft_motion::cl::Toolpath) -> BladeLeg<'_> {
        BladeLeg { path: t, width: WIDTH }
    }
    match blade_program_warnings(&grooved(), &[leg(&rough), leg(&sever)], TOL).as_slice() {
        [Warning::Envelope {
            at,
            gouge_mm,
            rest_mm,
        }] => {
            assert!(*gouge_mm <= 0.0, "nothing here cuts below the model: {at}");
            assert!(*rest_mm > 0.24 && *rest_mm < 0.26, "rest size: {rest_mm}");
            assert!(*at < rough.moves.len(), "blame stands in the guilty leg's own range: {at}");
        }
        other => panic!("expected one composed rest verdict, got {other:?}"),
    }
    assert!(
        blade_program_warnings(&grooved(), &[leg(&rough), leg(&fin), leg(&sever)], TOL).is_empty(),
        "a program that keeps its promise measures zero"
    );
    assert!(
        blade_program_warnings(&grooved(), &[leg(&fin), leg(&sever)], TOL).is_empty(),
        "a clean program measures zero too - the vacuity the per-op law hid"
    );
    // A leg without a blade is not a measurement: the judge that
    // cannot measure never reports silence.
    let bladeless = BladeLeg { path: &fin, width: 0.0 };
    match blade_program_warnings(&grooved(), &[leg(&rough), bladeless], TOL).as_slice() {
        [Warning::Gouge { at: 0 }] => {}
        other => panic!("a zero-width leg must refuse, got {other:?}"),
    }
}

#[test]
fn a_missed_pass_is_charged_its_windows_tall_claim() {
    // Hand-built proof of the over-report law, on a stepped pocket:
    // floor 5.5 from -6 to -2, floor 5.0 from -2 to 2, wall above
    // and below both. Two disjoint plunges (centers -4.0 and 1.0,
    // bands [-5,-3] and [0,2]) each land exactly ON their own
    // floor, so nothing gouges; the stretch (-3,0) stands inside
    // the passes' hull window (-5,2) covered by NOTHING. Where it
    // carries the lower floor the law charges REF - E = 0.5 - the
    // hull's TALLER claim, not the gap's own chord, because a
    // pass missing entirely must not buy the silence the tall
    // region promises. (On the FLAT groove model no such gap has
    // anything shorter to rest by - which is why the proof needs
    // the step.) Silence would be the silent-wrong-answer the
    // Prime Directive names; the law over-reports on purpose.
    let stepped = revolve(
        16,
        &[
            (0.0, -16.0),
            (6.0, -16.0),
            (6.0, -6.0),
            (5.5, -6.0),
            (5.5, -2.0),
            (5.0, -2.0),
            (5.0, 2.0),
            (6.0, 2.0),
            (6.0, 40.0),
            (0.0, 40.0),
        ],
    );
    let f = Frame::about(&stepped).expect("upright");
    let mut b = PathBuilder::new(f.point(6.05, -4.0));
    b.push(MoveKind::Rapid, f.point(6.05, -4.0), None);
    b.push(MoveKind::Feed, f.point(5.5, -4.0), Some(60.0));
    b.push(MoveKind::Rapid, f.point(6.05, -4.0), None);
    b.push(MoveKind::Rapid, f.point(6.05, 1.0), None);
    b.push(MoveKind::Feed, f.point(5.0, 1.0), Some(60.0));
    b.push(MoveKind::Rapid, f.point(6.05, 1.0), None);
    let t = b.finish();
    let warns = blade_warnings(&stepped, &t, WIDTH, TOL, 0.0);
    match warns.as_slice() {
        [Warning::Envelope { gouge_mm, rest_mm, .. }] => {
            assert!(*gouge_mm <= 0.0, "each plunge landed on its own floor: {warns:?}");
            assert!(
                *rest_mm > 0.49 && *rest_mm < 0.51,
                "the missed stretch should be charged the tall claim: {warns:?}"
            );
        }
        other => panic!("expected one rest verdict for the missed pass, got {other:?}"),
    }
}

#[test]
fn a_parting_kerf_is_a_cut_not_a_gouge() {
    // The separation law: a feed that reaches the axis (tip 0)
    // OPENS the material - the verdict reads the kerf silent,
    // inside its whole band, ladders and all. The kernel's own
    // part path across a plain wall must report ZERO warnings, or
    // every honest partop in the app greys its cache.
    let t = part(
        &wall(),
        &PartParams {
            tolerance: TOL,
            clearance_z: 44.0,
            top_z: 40.0,
            parting_z: 0.0,
            stepdown: 1.5,
            feed: 0.0,
            plunge_feed: 60.0,
            blade_width: WIDTH,
            kerf: Default::default(),
        },
    )
    .expect("the bar parts");
    let warns = blade_warnings(&wall(), &t, WIDTH, TOL, 0.0);
    assert!(warns.is_empty(), "the kerf was convicted: {warns:?}");
}

// ---------------------------------------------------------- kernels

#[test]
fn the_finish_walks_the_band_to_its_floor() {
    // Centers tile by the FULL width with the end kerfs reaching
    // the band edges: window 2.4 wide, blade 2.0 - centers -5.0
    // and -4.6 (the last step lands on hi_c even though the width
    // does not divide the run). Each center plunges once, straight
    // down, to the models own floor at 4.0; every other move is a
    // rapid.
    let f = Frame::about(&grooved()).expect("upright");
    let t = groove_finish(&grooved(), &groove_fin(-3.6, -6.0)).expect("finishes");
    let centers: Vec<f64> = {
        let mut v: Vec<f64> = Vec::new();
        for m in t.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
            let (a, b) = (sr(&f, m.from), sr(&f, m.to));
            assert!((a.0 - b.0).abs() < 1e-9, "a blade feed stays at its station: {a:?} {b:?}");
            assert!(a.1 > 4.0 + 1e-9 && (b.1 - 4.0).abs() < 1e-9, "one plunge to the floor: {a:?} {b:?}");
            assert_eq!(m.feed, Some(60.0), "the plunge is the only feed, at the plunge number");
            v.push(b.0);
        }
        v
    };
    assert_eq!(centers.len(), 2, "a 2.4 band eats two 2.0-wide kerfs: {centers:?}");
    assert!((centers[0] + 5.0).abs() < 1e-9, "the first center stands at s_lo + half: {centers:?}");
    assert!((centers[1] + 4.6).abs() < 1e-9, "the last center lands on hi_c: {centers:?}");
    let rapids: Vec<&_> = t.moves.iter().filter(|m| m.kind == MoveKind::Rapid).collect();
    assert!(rapids.iter().all(|m| sr(&f, m.to).1 > 4.0 + 1e-9), "a rapid never stands in the cut");
    let (open, close) = (sr(&f, rapids[0].to).1, sr(&f, t.moves.last().expect("ends clear").to).1);
    assert!((open - close).abs() < 1e-9, "opens and closes on the same clear radius: {open} {close}");
}

#[test]
fn the_rapids_clear_the_whole_bar_not_just_the_window() {
    // The mid-bar groove in the flank: the window stands on the
    // neck (r 3), but the bar's own widest claim is the flange at
    // 6 - so every rapid must ride 6 + tol, or the blade would
    // drag through the shoulder between centers.
    let f = Frame::about(&flange()).expect("upright");
    let t = groove_rough(
        &flange(),
        &GrooveRoughParams {
            tolerance: TOL,
            clearance_z: 24.0,
            top_z: 8.0,
            bottom_z: 5.0,
            stepdown: 0.5,
            stock_to_leave: 0.0,
            feed: 0.0,
            plunge_feed: 60.0,
            blade_width: 1.0,
        },
    )
    .expect("the neck grooves");
    assert!(
        rapid_floor(&f, &t) >= 6.0 - 1e-9,
        "rapids rode under the flange: {}",
        rapid_floor(&f, &t)
    );
}

#[test]
fn the_rough_ladders_and_stops_at_the_leave_line() {
    // On a FLAT floor (the groove's 4.0 band), every center's base
    // is the floor itself, so each ladder is one rung to 4.0 +
    // leave - and the plunges' tips all stand exactly there.
    let f = Frame::about(&grooved()).expect("upright");
    let t = groove_rough(
        &grooved(),
        &GrooveRoughParams {
            tolerance: TOL,
            clearance_z: 44.0,
            top_z: -3.6,
            bottom_z: -6.0,
            stepdown: 0.5,
            stock_to_leave: 0.25,
            feed: 0.0,
            plunge_feed: 60.0,
            blade_width: WIDTH,
        },
    )
    .expect("roughs");
    let mut tips = 0;
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        let (_, r) = sr(&f, m.to);
        assert!((r - 4.25).abs() < 1e-9, "the ladder's floor is the leave line: {r}");
        tips += 1;
    }
    assert_eq!(tips, 2, "one rung per center on a flat floor");
}

#[test]
fn parting_ladders_to_the_axis_in_one_center() {
    // ref_top 6, stepdown 1.5 - tips 4.5, 3.0, 1.5, 0.0, and the
    // through-axis rung is the last feed: the blade's tip stands ON
    // the axis, the kerf's whole column falls above it. One center,
    // half a blade below the parting plane (#3452: the plane is the
    // part's end face, the kerf stands on the chuck side of it).
    let f = Frame::about(&wall()).expect("upright");
    let t = part(
        &wall(),
        &PartParams {
            tolerance: TOL,
            clearance_z: 44.0,
            top_z: 40.0,
            parting_z: 0.0,
            stepdown: 1.5,
            feed: 0.0,
            plunge_feed: 60.0,
            blade_width: WIDTH,
            kerf: Default::default(),
        },
    )
    .expect("parts");
    let feeds: Vec<_> = t.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    assert_eq!(feeds.len(), 4, "the 6 of column eats four 1.5 rungs");
    for m in &feeds {
        let (a, b) = (sr(&f, m.from), sr(&f, m.to));
        assert!((a.0 - b.0).abs() < 1e-9, "one center: {a:?} {b:?}");
        assert!((a.0 - (f.station_of(0.0) - WIDTH * 0.5)).abs() < 1e-6, "the center stands half a blade below z 0: {}", a.0);
    }
    assert!((sr(&f, feeds[0].to).1 - 4.5).abs() < 1e-9, "the first rung steps the stepdown");
    assert!(sr(&f, feeds[3].to).1.abs() < 1e-9, "the last rung stands on the axis");
    let last = t.moves.last().expect("a path ends");
    assert_eq!(last.kind, MoveKind::Rapid);
    assert!(sr(&f, last.to).1 > 6.0, "it closes with the blade clear of the kerf");
}

// --------------------------------------------------------- refusals

#[test]
fn every_refusal_names_itself() {
    // Degenerate or unlawful input errors, loudly and by word -
    // never a path, never a crash, never a hang.

    // Narrower than the blade, by its own numbers.
    let err = groove_finish(&grooved(), &groove_fin(-5.0, -6.0)).expect_err("1.0 of window cannot hold a 2.0 blade");
    assert!(format!("{err:?}").contains("narrower than the blade"), "{err:?}");
    assert!(format!("{err:?}").contains("1.000"), "the window length speaks: {err:?}");
    assert!(format!("{err:?}").contains("2.000"), "and the width: {err:?}");

    // A window holding no length of bar.
    let err = groove_finish(&grooved(), &groove_fin(45.0, 44.0)).expect_err("above the bar");
    assert!(matches!(err, GenError::Empty(_)), "{err:?}");
    assert!(format!("{err:?}").contains("holds no length of the bar"), "{err:?}");

    // Zero and negative width; not-a-number.
    let mut p = groove_fin(-3.6, -6.0);
    p.blade_width = 0.0;
    assert!(groove_finish(&grooved(), &p)
        .err()
        .is_some_and(|e| format!("{e:?}").contains("the kerf's width IS the tool")));
    let mut p = groove_fin(-3.6, -6.0);
    p.top_z = f64::NAN;
    assert!(p.blade_width.is_finite());
    assert!(groove_finish(&grooved(), &p).is_err(), "a nan height is refused");
    assert!(groove_finish(&[], &groove_fin(-3.6, -6.0)).is_err(), "no facets to groove");

    // A parting plane off the end of the bar.
    let err = part(
        &wall(),
        &PartParams {
            tolerance: TOL,
            clearance_z: 44.0,
            top_z: 40.0,
            parting_z: 50.0,
            stepdown: 1.5,
            feed: 0.0,
            plunge_feed: 60.0,
            blade_width: WIDTH,
            kerf: Default::default(),
        },
    )
    .expect_err("nothing stands at z 50");
    // #3452: the whole wall stands below z 50, on the chuck side - no
    // part stands past the plane.
    assert!(format!("{err:?}").contains("stands wholly on the chuck side"), "{err:?}");

    // The center sweep's own cap: a hair-wide blade in a long run.
    let err = groove_rough(
        &wall(),
        &GrooveRoughParams {
            tolerance: TOL,
            clearance_z: 44.0,
            top_z: 40.0,
            bottom_z: -15.5,
            stepdown: 1.0,
            stock_to_leave: 0.0,
            feed: 0.0,
            plunge_feed: 60.0,
            blade_width: 0.0004,
        },
    )
    .expect_err("55 of run on a 0.0004 blade is not one operation");
    assert!(format!("{err:?}").contains("center sweep"), "{err:?}");

    // The rough's doors: leave is a size, the ladder has a cap.
    let mut r = GrooveRoughParams {
        tolerance: TOL,
        clearance_z: 44.0,
        top_z: -3.6,
        bottom_z: -6.0,
        stepdown: 0.5,
        stock_to_leave: 0.25,
        feed: 0.0,
        plunge_feed: 60.0,
        blade_width: WIDTH,
    };
    r.stepdown = 0.0;
    assert!(groove_rough(&grooved(), &r).err().is_some_and(|e| format!("{e:?}").contains("stepdown")));
    r.stepdown = 0.5;
    r.stock_to_leave = -0.1;
    assert!(groove_rough(&grooved(), &r).err().is_some_and(|e| format!("{e:?}").contains("floor")));
}
