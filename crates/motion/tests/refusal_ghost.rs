//! #261 part 3: a refusal raised with a path in hand hands that path
//! back as a display-only ghost. The ladders' centre-cutting refusal
//! (pocket, slot, bore, circular, contour, the 3-D ladder) carries the
//! moves built so far; the plunge law's audit of a finished path
//! carries the path it judged, beside the spot it names. The refusal
//! reads exactly as before, and the ghost is bounded and finite.
//!
//! Provenance: our own tests over our own refusal model; no vendor
//! material consulted.
//!
//! Gate map (#261): covers camcraft_motion::machining2d (gen_error,
//! ghost, pocket, slots, bore, circular, contour, contour_chain),
//! camcraft_motion::machining3d (ladder), camcraft_motion::plunge_law.
//! #871: part-way refusals (bore's per-hole pass budget, and the sites
//! listed in docs/decisions/871-refusal-ghost-rest.md) carry the moves
//! built before them.

use camcraft_motion::approach::{Approach, CENTER_CUT_REFUSAL};
use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::Point3;
use camcraft_motion::links::{Link, Links};
use camcraft_motion::machining2d::{GenError, PocketParams, PocketStrategy, SlotParams, pocket, slot};
use camcraft_motion::plunge_law::{Solid, audit};

fn not_cc(links: Links) -> Links {
    Links { approach: Approach { center_cutting: false, ..links.approach }, ..links }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).expect("loop")
}

/// The pocket with an island the centre-cutting law suite uses: its
/// zigzag rows restart inside a level, where a dive may find no room.
fn with_island() -> Region {
    let mut hole = rect(15.0, 12.0, 25.0, 18.0).points().to_vec();
    hole.reverse();
    Region::new(vec![rect(0.0, 0.0, 40.0, 30.0), Loop::new(hole).expect("hole")]).expect("region")
}

/// Every refusal of the not-centre-cutting slot and pocket sweeps.
fn refusals() -> Vec<GenError> {
    let links = [
        Links::PLANAR,
        Links { pass: Link::Clearance, ..Links::PLANAR },
        Links { level: Link::KeepDown, pass: Link::KeepDown, ..Links::PLANAR },
    ];
    let mut out = Vec::new();
    for entry in [Entry3D::Plunge, Entry3D::Helix, Entry3D::Ramp, Entry3D::Arc] {
        for l in links {
            for (w, d, so) in [(10.0, 10.0, 5.0), (12.0, 6.0, 3.0)] {
                let p = SlotParams {
                    links: not_cc(l),
                    tool_diameter: d,
                    width: w,
                    stock_to_leave: 0.0,
                    clearance_z: 5.0,
                    retract_z: None,
                    top_z: 0.0,
                    bottom_z: -6.0,
                    stepdown: 3.0,
                    stepover: so,
                    feed: 500.0,
                    plunge_feed: 100.0,
                    climb: true,
                    entry,
                    helix_radius: 1.5,
                };
                out.extend(slot(Point2::new(0.0, 0.0), Point2::new(40.0, 10.0), &p).err());
            }
            for strategy in [PocketStrategy::Zigzag, PocketStrategy::Waterline] {
                let p = PocketParams {
                    tool_diameter: 6.0,
                    stepover: 2.5,
                    stock_to_leave: 0.0,
                    clearance_z: 25.0,
                    top_z: 0.0,
                    bottom_z: -6.0,
                    stepdown: 2.0,
                    feed: 500.0,
                    plunge_feed: 100.0,
                    climb: true,
                    strategy,
                    retract_z: Some(15.0),
                    entry,
                    helix_radius: 1.5,
                    optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
                    blend_turns: false,
                    links: not_cc(l),
                };
                out.extend(pocket(&with_island(), &p).err());
            }
        }
    }
    out
}

#[test]
fn a_ladders_centre_cut_refusal_hands_back_the_moves_it_had_built() {
    let errs = refusals();
    let ghosted: Vec<&GenError> = errs.iter().filter(|e| e.ghost().is_some()).collect();
    assert!(!ghosted.is_empty(), "some sweep configuration refuses mid-ladder: {errs:?}");
    for e in ghosted {
        let g = e.ghost().expect("ghost");
        assert!(!g.is_empty() && g.len() <= camcraft_motion::machining2d::Ghost::MAX_MOVES);
        assert!(g.moves().iter().all(|m| m.is_finite()), "a ghost is finite by law");
        // The words are the refusal's own: a ghost changes no sentence.
        let s = e.to_string();
        assert!(s.starts_with("invalid machining parameters: ") && s.contains(CENTER_CUT_REFUSAL), "{s}");
        assert!(format!("{e:?}").contains(CENTER_CUT_REFUSAL), "Debug still names the law: {e:?}");
    }
}

#[test]
fn the_plunge_audit_hands_back_the_path_it_judged_beside_its_spot() {
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let moves = vec![
        Move::new(MoveKind::Rapid, p(5.0, 5.0, 10.0), p(5.0, 5.0, 2.0), None),
        Move::new(MoveKind::Plunge, p(5.0, 5.0, 2.0), p(5.0, 5.0, -3.0), Some(100.0)),
        Move::new(MoveKind::Feed, p(5.0, 5.0, -3.0), p(25.0, 5.0, -3.0), Some(500.0)),
    ];
    let t = Toolpath { moves, ..Toolpath::default() };
    let solid = Solid { top: 0.0, footprint: None };
    let e = audit(&t, false, 3.0, &solid, "the test's entry").expect_err("a plunge into solid");
    assert_eq!(e.location(), Some(p(5.0, 5.0, -3.0)), "the spot of part 2 stands");
    assert_eq!(e.ghost().map(|g| g.moves().to_vec()), Some(t.moves.clone()), "the whole judged path");
    assert!(e.to_string().contains(CENTER_CUT_REFUSAL), "{e}");
    // A centre-cutting tool passes: no refusal, so no ghost to speak of.
    assert!(audit(&t, true, 3.0, &solid, "x").is_ok());
}


fn bore_hole(x: f64, floor_z: f64) -> camcraft_motion::machining2d::BoreHole {
    camcraft_motion::machining2d::BoreHole {
        xy: Point2::new(x, 0.0),
        radius: 8.0,
        top_z: 0.0,
        floor_z,
        through: false,
        stepped: false,
    }
}

fn bore_over(holes: Vec<camcraft_motion::machining2d::BoreHole>) -> Result<Toolpath, GenError> {
    camcraft_motion::machining2d::bore(&camcraft_motion::machining2d::BoreParams {
        holes,
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        stepdown: 0.01,
        through_allowance: 0.5,
        clearance_z: 5.0,
        retract_z: Some(2.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        tolerance: 0.01,
        entry: Entry3D::Plunge,
        helix_radius: 1.5,
        links: Links::PLANAR,
    })
}

#[test]
fn a_part_way_refusal_hands_back_the_holes_bored_before_it() {
    // #871: the second hole is too deep for the stepdown's pass budget;
    // the first was bored already, and its moves ride the refusal.
    let deep = -1_001.0;
    let e = bore_over(vec![bore_hole(0.0, -0.05), bore_hole(40.0, deep)]).expect_err("the deep hole is refused");
    assert!(e.to_string().contains("passes"), "the pass-budget words stand: {e}");
    let g = e.ghost().expect("the first hole's moves ride the refusal");
    assert!(g.moves().iter().all(|m| m.is_finite()));
    // Every cutting move sits in the first hole; the refused one has none.
    let cuts: Vec<&Move> = g.moves().iter().filter(|m| m.kind != MoveKind::Rapid).collect();
    assert!(!cuts.is_empty());
    assert!(cuts.iter().all(|m| m.to.x.abs() <= 8.0 + 1e-6), "only the bored hole's cuts");
    // Refused at the first hole: nothing was built, so there is no ghost -
    // and the sentence is exactly the plain refusal's.
    let first = bore_over(vec![bore_hole(40.0, deep)]).expect_err("refused");
    assert!(first.ghost().is_none(), "no move before the refusal, no ghost");
    assert_eq!(first.to_string(), e.to_string());
    // The successful bore is the same path either way (display-only).
    assert!(bore_over(vec![bore_hole(0.0, -0.05)]).is_ok());
}
