//! #1036: the milled Bore ride's way into an undrilled hole with a cutter
//! that is not centre-cutting. The Bore arm hands the tool's
//! `center_cutting` to the planar ladder (`lk.approach.center_cutting`,
//! M11.3 B-D, 874507a8) and every descent of that ladder asks the
//! centre-cutting law (515525c4): a plunge into the level's solid becomes
//! a helix or a ramp along the first cut, or the operation is refused by
//! name. These tests pin that the law covers the Bore ride: an undrilled
//! hole in solid stock is entered by a ramp along the radial sweep, never
//! an axial plunge into solid, and a hole too tight for any ramp or helix
//! is refused by name. A centre-cutting cutter keeps its axial plunge.
//!
//! Provenance: standard CNC entry practice (a cutter without centre
//! teeth ramps or helixes into solid, it never plunges) per public
//! machining handbooks; the laws are this project's (M11.3). No vendor
//! material consulted.
//!
//! Gate map (#1036): covers app::operation (generate, the Bore arm),
//! motion::machining2d (bore, descent), motion::plunge_law.

use flecs_ecs::prelude::World;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, History, SetStock};
use camcraft_app::drilling::Hole;
use camcraft_app::geo2d::Point2;
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;
use camcraft_motion::approach::{CENTER_CUT_REFUSAL, NOT_CENTER_CUTTING};
use camcraft_motion::plunge_law::{first_solid_plunge, Solid};

/// A solid 60 x 60 x 20 block (nothing drilled) and an 8 mm end mill
/// twice: "CC" as catalogued, "NC" the same body not centre-cutting.
fn world() -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == "EM-0800-4FL").map(|(_, t)| t).expect("catalog entry");
    h.execute(&world, AddTool::new(&world, "CC", t.clone()).expect("tool"));
    h.execute(&world, AddTool::new(&world, "NC", Tool { center_cutting: false, ..t }).expect("tool"));
    world
}

fn bore(radius: f64) -> OpParams {
    OpParams::Bore {
        links: camcraft_app::links::Links::PLANAR,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        stock_to_leave: 0.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 150.0,
        climb: true,
        tolerance: 0.01,
        holes: vec![Hole { xy: Point2::new(15.0, 15.0), radius, entry_z: 20.0, floor_z: 5.0, through: false, stepped: false }],
    }
}

fn generate(world: &World, tool: &str, params: OpParams) -> Result<Toolpath, String> {
    let op = Operation {
        tool: tool.into(),
        src: SrcRef::Stock,
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    operation::generate(world, &op, "B").map(|(p, _)| p).map_err(|e| e.to_string())
}

/// The block's solid: everything under its top, no footprint (the
/// whole hole is undrilled stock).
const SOLID: Solid = Solid { top: 20.0, footprint: None };

#[test]
fn a_cutter_that_is_not_centre_cutting_ramps_into_an_undrilled_bore_never_plunges() {
    let world = world();
    let path = generate(&world, "NC", bore(10.0)).expect("the ramp stands in for the plunge");
    assert!(path.all_finite());
    // The after-the-fact audit finds no axial descent onto an uncut column.
    assert_eq!(first_solid_plunge(&path, 4.0, &SOLID), None, "an axial plunge into solid stock");
    assert!(
        path.notes.iter().any(|n| n.starts_with(NOT_CENTER_CUTTING) && n.contains("ramp along the first cut")),
        "the substitution is named: {:?}",
        path.notes
    );
    // Every Plunge move it does post lands on a column the ride already cut.
    let plunges = path.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count();
    assert!(plunges > 0, "the ring re-entries are plunges into cut columns");
    // The ride still reaches the floor of the hole.
    let low = path.moves.iter().filter(|m| m.kind != MoveKind::Rapid).map(|m| m.to.z).fold(f64::INFINITY, f64::min);
    assert!((low - 5.0).abs() < 1e-6, "the ride bottoms at the floor: {low}");
}

#[test]
fn a_centre_cutting_cutter_keeps_its_axial_plunge() {
    let world = world();
    let path = generate(&world, "CC", bore(10.0)).expect("cc bore");
    // The same audit convicts the centre-cutting path - it plunges on the
    // hole's axis, which its centre teeth may - so the NC pass above is
    // the law's work, not a blind audit.
    assert!(first_solid_plunge(&path, 4.0, &SOLID).is_some(), "the centre-cutting ride dives on the axis");
    assert!(!path.notes.iter().any(|n| n.starts_with(NOT_CENTER_CUTTING)), "{:?}", path.notes);
}

#[test]
fn an_undrilled_hole_too_tight_for_a_ramp_or_helix_is_refused_by_name() {
    let world = world();
    // An 8 mm cutter in a 9 mm hole: the centre disk is 0.5 mm, no helix
    // fits and the radial sweep is too short to ramp the 5 mm rung.
    let msg = generate(&world, "NC", bore(4.5)).err().unwrap_or_default();
    assert!(msg.contains(CENTER_CUT_REFUSAL), "refused by the centre-cutting law: {msg}");
    // The centre-cutting cutter bores the same hole.
    assert!(generate(&world, "CC", bore(4.5)).is_ok());
}
