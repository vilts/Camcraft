//! #2012: the depth ladder has two modes. The default keeps the remainder
//! in the last step (`top 20, bottom 2, step 5` cuts `[15, 10, 5, 2]`).
//! The opt-in equal-steps mode (`Approach::equal_steps`) cuts the same
//! number of levels, each step the span divided by that count (`[15.5,
//! 11, 6.5, 2]`). This file pins the levels of the two modes, on the law
//! and on each generator that reads the mode.
//!
//! Provenance: our own regression; dividing a depth into equal passes at
//! or under the maximum depth of cut is general machining knowledge.
//!
//! Gate map: covers camcraft_motion::machining2d (equal_steps, levels,
//! pocket, pocket_avoid, adaptive_troch, contour, slots, bore, circular) and
//! camcraft_motion::machining3d (entry).

use camcraft_motion::approach::Approach;
use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::Links;
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::{
    BoreHole, BoreParams, CircularParams, ContourParams, CornerRelief, CornerReliefKind, Lead,
    PocketParams, PocketStrategy, Side, SlotParams, adaptive_trochoidal, bore, circular, contour,
    cut_levels, cut_levels_for, equal_stepdown, pocket, pocket_avoid_with_relief,
    pocket_relief_plan, slot,
};
use camcraft_motion::machining3d::{Pocket3DParams, pocket3d, pocket3d_with_rest};
use camcraft_motion::tools::ToolGeom;
use camcraft_motion::verify::{AvoidItem, AvoidSpec, Cutter};

/// The issue's example: 18 mm of depth, a 5 mm stepdown.
const TOP: f64 = 20.0;
const BOTTOM: f64 = 2.0;
const STEP: f64 = 5.0;
const REMAINDER: [f64; 4] = [15.0, 10.0, 5.0, 2.0];
const EQUAL: [f64; 4] = [15.5, 11.0, 6.5, 2.0];

fn links(base: Links, equal: bool) -> Links {
    Links { approach: Approach { equal_steps: equal, ..base.approach }, ..base }
}

fn square(x0: f64, y0: f64, s: f64) -> Region {
    let l = Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x0 + s, y0),
        Point2::new(x0 + s, y0 + s),
        Point2::new(x0, y0 + s),
    ]);
    Region::new(vec![l.expect("loop")]).expect("region")
}

/// The planes the path cuts at: the Z of every level feed move (a feed
/// that keeps its Z), highest first, without repeats.
fn cut_planes(t: &Toolpath) -> Vec<f64> {
    let mut zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && (m.from.z - m.to.z).abs() < 1e-9)
        .map(|m| m.to.z)
        .collect();
    zs.sort_by(|a, b| b.total_cmp(a));
    zs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    zs
}

/// The path cuts at exactly the planes of its mode.
fn two_modes(what: &str, run: impl Fn(bool) -> Toolpath) {
    for (equal, want) in [(false, REMAINDER), (true, EQUAL)] {
        let got = cut_planes(&run(equal));
        assert_eq!(got.len(), want.len(), "{what}, equal {equal}: {got:?}");
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 1e-6, "{what}, equal {equal}: {got:?} is not {want:?}");
        }
    }
}

#[test]
fn the_default_ladder_keeps_the_remainder_and_equal_steps_is_opt_in() {
    assert!(!Approach::default().equal_steps, "the default is the remainder ladder");
    let on = Approach { equal_steps: true, ..Approach::DEFAULT };
    assert_eq!(cut_levels(TOP, BOTTOM, STEP).ok(), Some(REMAINDER.to_vec()));
    assert_eq!(cut_levels_for(TOP, &Approach::DEFAULT, BOTTOM, STEP).ok(), Some(REMAINDER.to_vec()));
    assert_eq!(cut_levels_for(TOP, &on, BOTTOM, STEP).ok(), Some(EQUAL.to_vec()));
}

#[test]
fn equal_steps_keep_the_level_count_and_never_pass_the_stepdown() {
    let on = Approach { equal_steps: true, ..Approach::DEFAULT };
    for (t, b, s) in [(20.0, 2.0, 5.0), (20.0, 0.0, 5.0), (13.0, 0.0, 5.0), (13.0, 0.0, 50.0), (10.0, 0.0, 10.0), (7.3, -4.1, 0.37)] {
        let rem = cut_levels(t, b, s).expect("remainder");
        let eq = cut_levels_for(t, &on, b, s).expect("equal");
        assert_eq!(eq.len(), rem.len(), "{t} {b} {s}: {eq:?}");
        assert_eq!(eq.last().copied(), Some(b), "{t} {b} {s}: the last level is Bottom Z exactly");
        let step = (t - b) / eq.len() as f64;
        let mut above = t;
        for z in &eq {
            assert!(((above - z) - step).abs() < 1e-9, "{t} {b} {s}: unequal step in {eq:?}");
            assert!(above - z <= s + 1e-9, "{t} {b} {s}: a step over the stepdown in {eq:?}");
            above = *z;
        }
    }
    // A depth that the stepdown divides: the two modes agree.
    assert_eq!(cut_levels_for(20.0, &on, 0.0, 5.0).ok(), Some(vec![15.0, 10.0, 5.0, 0.0]));
    assert_eq!(equal_stepdown(20.0, 2.0, 5.0), 4.5);
}

#[test]
fn equal_steps_keep_the_level_at_top_z_under_stock() {
    // #2004: stock over Top Z keeps the plane at Top Z; the steps under it
    // are equal.
    let a = Approach { stock_top: Some(13.0), ..Approach::DEFAULT };
    assert_eq!(cut_levels_for(10.0, &a, 1.0, 4.0).ok(), Some(vec![10.0, 6.0, 2.0, 1.0]));
    let on = Approach { equal_steps: true, ..a };
    assert_eq!(cut_levels_for(10.0, &on, 1.0, 4.0).ok(), Some(vec![10.0, 7.0, 4.0, 1.0]));
}

#[test]
fn equal_steps_refuse_what_the_remainder_ladder_refuses() {
    let on = Approach { equal_steps: true, ..Approach::DEFAULT };
    assert!(cut_levels_for(10.0, &on, 0.0, 0.0).is_err());
    assert!(cut_levels_for(10.0, &on, 0.0, -1.0).is_err());
    assert!(cut_levels_for(f64::NAN, &on, 0.0, 1.0).is_err());
    assert!(cut_levels_for(10.0, &on, 0.0, f64::NAN).is_err());
    assert!(cut_levels_for(1e9, &on, 0.0, 1e-9).is_err());
    // A span of zero (or less) stays the one bottom plane.
    assert_eq!(cut_levels_for(1.0, &on, 1.0, 5.0).ok(), Some(vec![1.0]));
    assert_eq!(cut_levels_for(0.0, &on, 1.0, 5.0).ok(), Some(vec![1.0]));
}

fn pp(strategy: PocketStrategy, equal: bool) -> PocketParams {
    PocketParams {
        blend_radius: None,
        tool_diameter: 6.0,
        stepover: 2.4,
        stock_to_leave: 0.0,
        clearance_z: 30.0,
        retract_z: Some(25.0),
        top_z: TOP,
        bottom_z: BOTTOM,
        stepdown: STEP,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        strategy,
        entry: Entry3D::Plunge,
        helix_radius: 2.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links: links(Links::PLANAR, equal),
    }
}

#[test]
fn a_pocket_cuts_the_levels_of_its_mode() {
    let reg = square(0.0, 0.0, 24.0);
    for s in [PocketStrategy::OffsetRings, PocketStrategy::Adaptive] {
        two_modes(&format!("2D pocket {s:?}"), |equal| pocket(&reg, &pp(s, equal)).unwrap_or_else(|e| panic!("{s:?}: {e}")));
    }
}

#[test]
fn a_pocket_with_keepouts_and_relief_cuts_the_levels_of_its_mode() {
    let reg = square(0.0, 0.0, 24.0);
    let relief = CornerRelief {
        kind: CornerReliefKind::Dogbone,
        ..Default::default()
    };
    let plan = pocket_relief_plan(&reg, 6.0, 0.0, &relief).expect("relief plan");
    assert!(!plan.reliefs.is_empty());
    // A remote protected edge selects the keep-out ladder without blocking a cut.
    let avoid = AvoidSpec {
        items: vec![AvoidItem {
            label: "remote edge".into(),
            tris: vec![],
            edges: vec![([100.0, 100.0, BOTTOM], [100.0, 101.0, TOP])],
        }],
        cutter: Cutter::flat(&ToolGeom {
            radius: 3.0,
            nose_radius: 0.0,
            flute_len: 8.0,
            shank_radius: 4.5,
            shank_len: 10.0,
            holder_radius: 12.0,
            holder_len: 20.0,
        }),
        clearance: 1.0,
    };
    assert!(!avoid.is_empty());
    two_modes("pocket with keep-outs and relief", |equal| {
        let path = pocket_avoid_with_relief(
            &reg,
            &pp(PocketStrategy::OffsetRings, equal),
            &avoid,
            &relief,
        )
        .expect("pocket with keep-outs and relief");
        for relief in &plan.reliefs {
            assert!(
                path.moves.iter().any(|m| {
                    (m.to.x - relief.end.x).hypot(m.to.y - relief.end.y) < 1e-2
                        && (m.to.z - BOTTOM).abs() < 1e-6
                }),
                "missing corner relief at the bottom plane"
            );
        }
        path
    });
}

#[test]
fn a_trochoidal_adaptive_cuts_the_levels_of_its_mode() {
    let reg = square(0.0, 0.0, 24.0);
    for (equal, want, other) in [(false, REMAINDER, EQUAL), (true, EQUAL, REMAINDER)] {
        let t = adaptive_trochoidal(&reg, &pp(PocketStrategy::Adaptive, equal)).expect("trochoidal");
        // The returns between loops ride a microlift over each level (a
        // feed that keeps its Z, but no cut): read the levels themselves.
        let planes = cut_planes(&t);
        let has = |z: f64| planes.iter().any(|q| (q - z).abs() < 1e-6);
        for z in want {
            assert!(has(z), "equal {equal}: level {z} missing in {planes:?}");
        }
        for z in &other[..3] {
            assert!(!has(*z), "equal {equal}: level {z} of the other mode in {planes:?}");
        }
    }
}

#[test]
fn a_contour_cuts_the_levels_of_its_mode() {
    two_modes("2D contour", |equal| {
        let p = ContourParams {
            tool_diameter: 6.0,
            stock_to_leave: 0.0,
            clearance_z: 30.0,
            retract_z: 25.0,
            top_z: TOP,
            bottom_z: BOTTOM,
            stepdown: STEP,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            side: Side::Outside,
            lead: Lead::None,
            tolerance: 0.01,
            tab_count: 0,
            tab_width: 0.0,
            tab_height: 0.0,
            entry: Entry3D::Plunge,
            helix_radius: 1.0,
            blend_turns: false,
            blend_radius: None,
            links: links(Links::PLANAR, equal),
            picks: Default::default(),
        };
        contour(&square(0.0, 0.0, 30.0), &p).expect("contour")
    });
}

#[test]
fn a_slot_cuts_the_levels_of_its_mode() {
    two_modes("slot", |equal| {
        let p = SlotParams {
            blend_turns: false,
            blend_radius: None,
            links: links(Links::PLANAR, equal),
            tool_diameter: 10.0,
            width: 10.0,
            stock_to_leave: 0.0,
            clearance_z: 30.0,
            retract_z: None,
            top_z: TOP,
            bottom_z: BOTTOM,
            stepdown: STEP,
            stepover: 5.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            entry: Entry3D::Plunge,
            helix_radius: 1.0,
        };
        slot(Point2::new(0.0, 0.0), Point2::new(40.0, 0.0), &p).expect("slot")
    });
}

#[test]
fn a_bore_cuts_the_levels_of_its_mode() {
    two_modes("bore", |equal| {
        let p = BoreParams {
            holes: vec![BoreHole { xy: Point2::new(0.0, 0.0), radius: 8.0, top_z: TOP, floor_z: BOTTOM, through: false, stepped: false }],
            tool_diameter: 6.0,
            corner_radius: 0.0,
            stock_to_leave: 0.0,
            stepdown: STEP,
            through_allowance: 0.0,
            clearance_z: 30.0,
            retract_z: None,
            feed: 500.0,
            plunge_feed: 250.0,
            climb: true,
            tolerance: 0.01,
            entry: Entry3D::Plunge,
            helix_radius: 1.0,
            links: links(Links::PLANAR, equal),
        };
        bore(&p).expect("bore")
    });
}

#[test]
fn a_circular_cuts_the_levels_of_its_mode() {
    two_modes("circular", |equal| {
        let p = CircularParams {
            pass_start: Default::default(),
            links: links(Links::PLANAR, equal),
            center: Point2::new(0.0, 0.0),
            radius: 10.0,
            vault: false,
            bottom_z: BOTTOM,
            top_z: TOP,
            tool_diameter: 6.0,
            stock_to_leave: 0.0,
            stepover: 2.0,
            stepdown: STEP,
            clearance_z: 30.0,
            retract_z: None,
            feed: 500.0,
            plunge_feed: 250.0,
            climb: true,
            entry: Entry3D::Plunge,
            helix_radius: 1.0,
        };
        circular(&square(-12.0, -12.0, 24.0), &p).expect("circular")
    });
}

/// A flat plate under the pocket, its top at Z -1: no part material
/// stands at or above any plane of the ladder.
fn plate() -> Vec<Triangle> {
    let (lo, hi) = ([-30.0, -30.0, -5.0], [70.0, 70.0, -1.0]);
    let v = |i: usize| Point3::new([lo[0], hi[0]][i & 1], [lo[1], hi[1]][(i >> 1) & 1], [lo[2], hi[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)]
        .into_iter()
        .flatten()
        .collect()
}

fn p3d(equal: bool) -> Pocket3DParams {
    Pocket3DParams {
        pass_start: Default::default(),
        links: links(Links::ROUGH3D, equal),
        tool_diameter: 6.0,
        stepover: 2.4,
        stock_to_leave: 0.0,
        clearance_z: 30.0,
        top_z: TOP,
        bottom_z: BOTTOM,
        stepdown: STEP,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::OffsetRings,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 1.5,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
    }
}

#[test]
fn a_3d_pocket_cuts_the_levels_of_its_mode() {
    two_modes("3D pocket", |equal| {
        pocket3d(&plate(), &square(0.0, 0.0, 40.0), &p3d(equal)).expect("3D pocket")
    });
}

/// The rest pocket's coarse leg reads the mode too, through the #2088
/// flat-floor level law. The plate leaves no rest material above the
/// floor, so the path cuts only the coarse planes.
#[test]
fn a_3d_rest_pocket_cuts_the_coarse_levels_of_its_mode() {
    two_modes("3D rest pocket", |equal| {
        pocket3d_with_rest(&plate(), &square(0.0, 0.0, 40.0), &p3d(equal)).expect("3D rest").0
    });
}
