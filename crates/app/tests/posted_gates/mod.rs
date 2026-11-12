//! The posted goldens no depth law judges (#1000), each run through the
//! gate the user's post would run for its kind: the slot, trochoidal
//! slot and trace goldens through the region laws `verify` applies at
//! generation (their kind's `Mode`, against the region the operation
//! layer itself builds for them - the authored stadium, the part's
//! section - with no model, as `depth_law_of` gives them none), and the
//! plasma plate through the 2D plasma laws the plasma post clause runs
//! (`posting_plasma::plasma_clause`: its refusals block the post, its
//! warnings ride the verdict line). Every program is first proved
//! byte-identical to its committed CL golden.
//!
//! Provenance: our own harness over our own goldens; no vendor material.

use flecs_ecs::prelude::World;

use crate::common::gen_to_cltext;
use crate::common::solid_part::{mill, plate, rect, region};

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{AddTool, CreateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::{Point2, Region};
use camcraft_app::geometry::Mesh;
use camcraft_app::machining2d::{SlotParams, TraceParams, TrochoidParams, TrochoidReturn, slot, trace, trochoidal_slot};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};
use camcraft_app::verify::plasma::{PlasmaFinding, PlasmaProgram, judge};
use camcraft_app::world::new_world;

fn golden(name: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
    std::fs::read_to_string(&p).expect("golden").replace("\r\n", "\n")
}

/// Any catalog end mill: the regions below never read the tool.
const TOOL: &str = "EM-0600-4FL";

/// A world holding one operation of `params` over `src` (an 80 x 60 x
/// 20 stock box, or the part `mesh`), and that operation's region as
/// the operation layer builds it (`operation::region_of`) with the
/// kind's verify mode.
fn op_region(params: OpParams, mesh: Option<Mesh>) -> (Region, Mode) {
    let world: World = new_world();
    let mut h = History::default();
    let src = match mesh {
        Some(m) => {
            h.execute(&world, ImportModel::from_mesh("part", m));
            SrcRef::Entity("part".into())
        }
        None => {
            let stock = SetStock::new(&world, StockShape::Box { dx: 80.0, dy: 60.0, dz: 20.0 }, Location::default());
            h.execute(&world, stock.expect("stock"));
            SrcRef::Stock
        }
    };
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "Op1", TOOL, src, params).expect("create"));
    let op = model::operation_of(&world, &model::find_by_name(&world, "Op1").expect("op")).expect("operation");
    let mode = op.params.kind().info().mode;
    (operation::region_of(&world, &op).expect("the operation's region"), mode)
}

/// The region laws, as generation runs them for a kind with no depth law.
fn region_law(t: &Toolpath, region: &Region, tool: &ToolGeom, mode: Mode, top: f64, clear: f64, leave: f64) -> Vec<Warning> {
    verify(
        t,
        &VerifyContext {
            region,
            stock: None,
            model: None,
            tool,
            cutter: None,
            mode,
            stock_top_z: top,
            clearance_z: clear,
            stock_to_leave: leave,
            gouge_tol: 0.01,
            model_excess: 0.0,
            z_tol: 1e-6,
            finish_tol: None,
            fixtures: &[],
            spindle: None,
        },
    )
}

fn slot_op(a: Point2, b: Point2, width: f64) -> OpParams {
    OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a,
        b,
        width,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(5.0),
        retract_z: None,
        top_z: Height::fixed(0.0),
        bottom_z: Height::fixed(-4.0),
        stepdown: 2.0,
        stepover: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
    }
}

fn slot_params(d: f64, width: f64, bottom: f64, stepdown: f64, stepover: f64) -> SlotParams {
    SlotParams {
        links: camcraft_app::links::Links::PLANAR,
        tool_diameter: d,
        width,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: bottom,
        stepdown,
        stepover,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
    }
}

/// (golden, verdict) for the slot, trochoidal slot and trace goldens.
pub fn region_verdicts() -> Vec<(&'static str, Vec<Warning>)> {
    let mut out = Vec::new();
    let mut check = |name: &'static str, t: &Toolpath, w: Vec<Warning>| {
        assert_eq!(gen_to_cltext(t), golden(name), "{name}: the regenerated program is the golden");
        out.push((name, w));
    };
    // golden_cl's two slots: (0,0)-(60,0) width 10 with a D10, and
    // (-10,0)-(50,0) width 20 with a D6, judged against their stadiums.
    for (name, a, b, d, width, bottom, stepdown, stepover) in [
        ("slot_fullwidth_centerline.cl", Point2::new(0.0, 0.0), Point2::new(60.0, 0.0), 10.0, 10.0, -6.0, 3.0, 5.0),
        ("slot_wide_channel_rows.cl", Point2::new(-10.0, 0.0), Point2::new(50.0, 0.0), 6.0, 20.0, -4.0, 4.0, 4.0),
    ] {
        let t = slot(a, b, &slot_params(d, width, bottom, stepdown, stepover)).expect("slot");
        let (reg, mode) = op_region(slot_op(a, b, width), None);
        check(name, &t, region_law(&t, &reg, &mill(d), mode, 0.0, 5.0, 0.0));
    }
    // The trochoidal slot: a D6 in a 10 mm channel (0,0)-(12,0) at Z -2.
    let (a, b) = (Point2::new(0.0, 0.0), Point2::new(12.0, 0.0));
    let p = TrochoidParams {
        tool_diameter: 6.0,
        width: 10.0,
        loop_width: None,
        step: 1.5,
        max_engagement: 90.0,
        ret: TrochoidReturn::Microlift { height: 0.3 },
        smoothing: 0.5,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: Some(1.0),
        top_z: -2.0,
        bottom_z: -2.0,
        stepdown: 2.0,
        feed: 800.0,
        return_feed: 2000.0,
        plunge_feed: 200.0,
        climb: true,
    };
    let t = trochoidal_slot(&[a, b], &p).expect("trochoid");
    let op = OpParams::TrochoidalSlot {
        a,
        b,
        width: 10.0,
        loop_width: None,
        step: 1.5,
        max_engagement: 90.0,
        ret: TrochoidReturn::Microlift { height: 0.3 },
        smoothing: 0.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(5.0),
        retract_z: Some(Height::fixed(1.0)),
        top_z: Height::fixed(0.0),
        bottom_z: Height::fixed(-2.0),
        stepdown: 2.0,
        feed: 800.0,
        return_feed: 2000.0,
        plunge_feed: 200.0,
        climb: true,
    };
    let (reg, mode) = op_region(op, None);
    check("trochoid_slot_small.cl", &t, region_law(&t, &reg, &mill(6.0), mode, 0.0, 5.0, 0.0));
    // The trace: the 40 x 20 plate with its 8 x 8 through hole, its
    // section the region the trace rides; a D3 end mill engraving.
    let (outer, hole) = (rect(0.0, 0.0, 40.0, 20.0), rect(16.0, 6.0, 24.0, 14.0));
    let p = TraceParams { clearance_z: 5.0, retract_z: None, top_z: 0.0, bottom_z: -2.0, stepdown: 1.0, feed: 500.0, plunge_feed: 100.0, climb: true };
    let t = trace(&region(&[outer.clone(), hole.clone()]), &p).expect("trace");
    let mut part = Vec::new();
    plate(&outer, Some((&hole, -4.0)), -4.0, 0.0, &mut part);
    let op = OpParams::Trace {
        clearance_z: Height::fixed(5.0),
        retract_z: None,
        top_z: Height::fixed(0.0),
        bottom_z: Height::fixed(-2.0),
        stepdown: 1.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        tilt: None,
    };
    let (reg, mode) = op_region(op, Some(Mesh::from_facets(part)));
    check("trace_plate_with_hole.cl", &t, region_law(&t, &reg, &mill(3.0), mode, 0.0, 5.0, 0.0));
    out
}

/// The wide-channel slot's program judged against the stadium of a
/// channel 2 mm narrower than its own: the region laws must convict it.
pub fn narrowed_slot_verdict() -> Vec<Warning> {
    let (a, b) = (Point2::new(-10.0, 0.0), Point2::new(50.0, 0.0));
    let t = slot(a, b, &slot_params(6.0, 20.0, -4.0, 4.0, 4.0)).expect("slot");
    let (reg, mode) = op_region(slot_op(a, b, 18.0), None);
    region_law(&t, &reg, &mill(6.0), mode, 0.0, 5.0, 0.0)
}

/// The plasma plate's CL golden (the program both its posted goldens,
/// inch and mm, come from), judged by the plasma laws.
pub fn plasma_findings() -> Vec<PlasmaFinding> {
    let t = camcraft_app::cl::text::from_text(&golden("plasma_plate_with_holes.cl")).expect("the plasma golden reads");
    judge(&PlasmaProgram::from_toolpath(&t).expect("the plasma laws read the golden"))
}
