//! Law (d)'s posed second reading (the swarf normal-link fix): a
//! POSED lateral rapid the column law convicts is re-read against the
//! part's solid and the open stock before the conviction stands, so
//! the air under an overhang - which a height field reads as the
//! stock standing above it - no longer convicts a leaning tool that
//! never enters the part. The law is not weakened where it exists:
//! a posed rapid whose stack passes through the ledge's own solid
//! still reads RapidInStock, a posed rapid in open stock (nothing
//! hanging over it, no cut below it) still reads RapidInStock, and
//! the upright twin of the under-ledge rapid keeps the hidden-stock
//! backstop's verdict byte for byte.
//!
//! Provenance: our own fixtures, specced from the Prime Directive and
//! the shop rule "never traverse through uncut stock"; no vendor
//! material.
//!
//! Gate map (law d posed): covers camcraft_sim::verify (rapids,
//! rapids_posed, hidden, rest, solid_index, midpose).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::{cube, Point3, Triangle};
use camcraft_sim::verify::{verify, Mode, ToolGeom, VerifyContext, Warning};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let l = Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ]);
    match l.and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("rect: {e:?}"),
    }
}

/// A post X/Y [-5, 5], Z [0, 10] carrying a ledge X [-5, 15], Y [-5, 5],
/// Z [10, 12]: the ledge overhangs X (5, 15] - the space under it is
/// outside the part and hidden from every column.
fn part() -> Vec<Triangle> {
    let mut f: Vec<Triangle> = cube(Point3::new(-5.0, -5.0, 0.0), Point3::new(5.0, 5.0, 10.0)).facets.to_vec();
    f.extend(cube(Point3::new(-5.0, -5.0, 10.0), Point3::new(15.0, 5.0, 12.0)).facets.iter().cloned());
    f
}

/// A short-flute tool with a narrow neck: 4 mm flute of r 3, a 40 mm
/// shank of r 1.5 - the stack the ledge tests lean.
fn necked() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 4.0,
        shank_radius: 1.5,
        shank_len: 40.0,
        holder_radius: 1.5,
        holder_len: 10.0,
    }
}

/// Judge `p` with a stock footprint X [-5, `x1`], Y [-5, 5] (the
/// model's own shadow at `x1` 15) under a stock top of `top`.
fn judge(p: &Toolpath, top: f64, x1: f64) -> Vec<Warning> {
    let facets = part();
    let region = rect(-5.0, -5.0, x1, 5.0);
    let tool = necked();
    let cx = VerifyContext {
        region: &region,
        stock: Some(&region),
        model: Some(&facets),
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: top,
        clearance_z: 30.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-3,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(p, &cx)
}

/// One lateral rapid along X `x0` -> 13 at Y 0, height `z`, under
/// `axis`: under the ledge, the flute's 3 mm radius clear of the post.
fn rapid(x0: f64, z: f64, axis: Option<[f64; 3]>) -> Toolpath {
    let mut m = Move::new(MoveKind::Rapid, Point3::new(x0, 0.0, z), Point3::new(13.0, 0.0, z), None);
    m.axis = axis;
    Toolpath { moves: vec![m], cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None }
}

/// A 60-degree lean toward -Y: the flute tops out 2 mm above the tip
/// and the shank leaves the ledge's Y [-5, 5] span below its underside.
const LEAN_60: [f64; 3] = [0.0, -0.866_025_403_784_438_6, 0.5];
/// A 30-degree lean toward -Y: a 4 mm flute reaches 3.46 mm up.
const LEAN_30: [f64; 3] = [0.0, -0.5, 0.866_025_403_784_438_6];

#[test]
fn a_posed_rapid_through_the_ledges_underside_names_part_and_stock_laws() {
    // Tip at Z 8 under the ledge: the flute's top at Z 11.46 stands
    // inside the ledge (Z [10, 12]) along the whole rapid.
    let w = judge(&rapid(9.0, 8.0, Some(LEAN_30)), 12.0, 15.0);
    assert_eq!(w, vec![Warning::RapidInPart { at: 0 }, Warning::RapidInStock { at: 0 }], "{w:?}");
}

/// The upright assembly meets the part; the stock law also convicts it.
fn part_and_stock_rapid(w: &[Warning]) {
    assert_eq!(w, [Warning::RapidInPart { at: 0 }, Warning::RapidInStock { at: 0 }], "{w:?}");
}

#[test]
fn a_posed_rapid_in_the_air_under_the_ledge_is_clean() {
    // Tip at Z 5: the flute tops out under Z 9.6, the shank crosses the
    // ledge's Y -5 wall plane at Z 7.9 - under the ledge, outside the
    // part, and out of the stock footprint from there on.
    let w = judge(&rapid(9.0, 5.0, Some(LEAN_60)), 12.0, 15.0);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn the_upright_twin_under_the_ledge_keeps_the_hidden_stock_verdict() {
    // The same rapid stating no pose: the column law and its hidden
    // backstop read the air under the ledge as uncut stock, as they
    // always did - a 3-axis program's verdict does not move.
    part_and_stock_rapid(&judge(&rapid(9.0, 5.0, None), 12.0, 15.0));
    // And a stated upright is the same program.
    part_and_stock_rapid(&judge(&rapid(9.0, 5.0, Some([0.0, 0.0, 1.0])), 12.0, 15.0));
}

#[test]
fn a_posed_rapid_in_open_stock_over_the_ledge_is_a_rapid_in_stock() {
    // Stock top raised to 20: the rapid at Z 15 over the ledge has
    // nothing hanging over it and no cut below it - open stock, the
    // column law exact, the lean no alibi.
    let w = judge(&rapid(9.0, 15.0, Some(LEAN_60)), 20.0, 15.0);
    assert_eq!(w, vec![Warning::RapidInStock { at: 0 }], "{w:?}");
}

#[test]
fn a_posed_rapid_whose_shank_leans_into_open_stock_is_a_rapid_in_stock() {
    // Under the ledge at Z 5 from X 11, leaning 60 degrees toward +X:
    // the flute and the shank clear the ledge's end corner (X 15, Z
    // 10), and the shank stands in the open stock of a footprint
    // widened to X 25 under a stock top of 20 - no overhang over it,
    // no cut below it.
    let w = judge(&rapid(11.0, 5.0, Some([0.866_025_403_784_438_6, 0.0, 0.5])), 20.0, 25.0);
    assert_eq!(w, vec![Warning::RapidInStock { at: 0 }], "{w:?}");
    // The same rapid with the footprint at the model's own shadow:
    // the shank leaves the stock where it leaves the ledge - clean.
    let w = judge(&rapid(11.0, 5.0, Some([0.866_025_403_784_438_6, 0.0, 0.5])), 12.0, 15.0);
    assert!(w.is_empty(), "{w:?}");
}

/// #3520: a rapid that meets the part names the part and the move, and
/// the stock law keeps its own message. Original box fixtures for the
/// shop rule that rapids must clear the part and stock.
/// Gate map (#3520): covers camcraft_sim::verify::{solid,rapids} and Warning messages.
mod part_message {
    use camcraft_sim::cl::{Cycle, CycleKind, Move, MoveKind, Toolpath};
    use camcraft_sim::geo2d::{Loop, Point2, Region};
    use camcraft_sim::geometry::{Point3, cube};
    use camcraft_sim::verify::{Cutter, Mode, SolidLaw, ToolGeom, VerifyContext, Warning, solid_hits, stack_hits, verify};

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    fn cutter() -> Cutter {
        Cutter {
            radius: 3.0, corner_r: 0.0, tip_angle: 0.0, flute_len: 20.0,
            shank_radius: 2.0, shank_len: 30.0, holder_radius: 15.0, holder_len: 40.0,
            body: None, neck: None, undercut: None,
        }
    }

    fn law() -> SolidLaw {
        SolidLaw { step: 0.25, allow: 0.01, hole_walls: false }
    }

    fn assert_part_message(warnings: &[Warning], at: usize) {
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].to_string(), format!("tool meets the part on rapid move {at}"));
        assert!(warnings[0].blocks_post(), "a rapid into the part must hold the post");
        assert_eq!(serde_json::to_value(warnings[0]).unwrap(), serde_json::json!({"rapid_in_part": {"at": at}}));
    }

    #[test]
    fn a_vertical_part_rapid_names_the_part_and_move() {
        let part = cube(p(0.0, 0.0, 0.0), p(40.0, 40.0, 10.0));
        let path = Toolpath::new(vec![Move::new(MoveKind::Rapid, p(15.0, 30.0, 5.0), p(15.0, 30.0, 32.0), None)]);
        assert_part_message(&solid_hits(&path, &part.facets, &cutter(), &law()), 0);
    }

    #[test]
    fn a_stack_retract_edge_column_names_the_part_and_move() {
        // The cutting edge starts below the slab, then rises through it.
        let slab = cube(p(5.0, 22.0, 40.0), p(25.0, 38.0, 45.0));
        let bare = Cutter { shank_len: 0.0, holder_len: 0.0, ..cutter() };
        let path = Toolpath::new(vec![
            Move::new(MoveKind::Feed, p(15.0, 10.0, 9.7), p(15.0, 30.0, 9.7), Some(500.0)),
            Move::new(MoveKind::Rapid, p(15.0, 30.0, 9.7), p(15.0, 30.0, 60.0), None),
        ]);
        assert_part_message(&stack_hits(&path, &slab.facets, &bare, &law()), 1);
    }

    #[test]
    fn a_stopped_fine_bore_holder_rapid_names_the_part_and_move() {
        // The holder starts below the slab. Its stopped retract enters it.
        let slab = cube(p(-20.0, -20.0, 100.0), p(20.0, 20.0, 105.0));
        let path = Toolpath {
            cycles: vec![Cycle {
                kind: CycleKind::FineBore, xy: Point2::new(0.0, 0.0), r_z: 20.0,
                bottom_z: 0.0, entry_z: 20.0, peck: 0.0, dwell: 0.0, first_move: 0,
            }],
            ..Toolpath::new(vec![
                Move::new(MoveKind::Plunge, p(0.0, 0.0, 20.0), p(0.0, 0.0, 0.0), Some(60.0)),
                Move::new(MoveKind::Rapid, p(0.0, 0.0, 0.0), p(0.5, 0.0, 0.0), None),
                Move::new(MoveKind::Rapid, p(0.5, 0.0, 0.0), p(0.5, 0.0, 20.0), None),
                Move::new(MoveKind::Rapid, p(0.5, 0.0, 20.0), p(0.0, 0.0, 20.0), None),
            ])
        };
        let warnings = solid_hits(&path, &slab.facets, &cutter(), &law());
        // The feed and shift back also meet the slab; isolate the stopped lift.
        let lift: Vec<_> = warnings.into_iter().filter(|w| {
            let value = serde_json::to_value(w).unwrap();
            value.as_object().unwrap().values().any(|fields| fields["at"] == 2)
        }).collect();
        assert_part_message(&lift, 2);
    }

    #[test]
    fn a_stock_rapid_keeps_its_stock_message() {
        let region = Region::new(vec![Loop::new(vec![
            Point2::new(-40.0, -40.0), Point2::new(40.0, -40.0),
            Point2::new(40.0, 40.0), Point2::new(-40.0, 40.0),
        ]).unwrap()]).unwrap();
        let tool = ToolGeom {
            radius: 2.0, nose_radius: 0.0, flute_len: 30.0, shank_radius: 2.0,
            shank_len: 20.0, holder_radius: 2.0, holder_len: 1.0,
        };
        let path = Toolpath::new(vec![Move::new(MoveKind::Rapid, p(0.0, 0.0, 35.0), p(0.0, 0.0, 20.0), None)]);
        let cx = VerifyContext {
            region: &region, stock: Some(&region), tool: &tool, mode: Mode::Area,
            stock_top_z: 25.0, clearance_z: 35.0, gouge_tol: 0.01, z_tol: 1e-3,
            ..Default::default()
        };
        let warnings = verify(&path, &cx);
        assert_eq!(warnings, vec![Warning::RapidInStock { at: 0 }]);
        assert_eq!(warnings[0].to_string(), "rapid move 0 travels through stock (below the clearance plane, or at it below the stock top)");
        assert!(warnings[0].blocks_post());
        assert_eq!(serde_json::to_value(warnings[0]).unwrap(), serde_json::json!({"rapid_in_stock": {"at": 0}}));
    }
}
