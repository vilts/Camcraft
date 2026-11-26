//! #220 slice 1: the Trochoidal slot operation - created and edited
//! through the command history (undo / redo), generated against the
//! stock with the verifier silent on its loops, posted, and filed
//! through the project round trip; a channel narrower than the tool is
//! refused by name at generation, a microlift above the clearance plane
//! at command creation and edit (#774).
//!
//! Provenance: our own fixtures (a 6 mm end mill in a 10 mm channel).
//!
//! Gate map (#220): covers app::operation (trochoid_op, kinds, params,
//! generate), app::project (trochoid DTO), app::commands (create / edit /
//! undo), motion::machining2d (trochoid_slot).

use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::machining2d::TrochoidReturn;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpType, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;
use camcraft_app::{posting, project};
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-0600-4FL";

fn trochoid(width: f64) -> OpParams {
    OpParams::TrochoidalSlot {
        a: Point2::new(-15.0, 0.0),
        b: Point2::new(15.0, 0.0),
        width,
        loop_width: None,
        step: 0.8,
        max_engagement: 90.0,
        ret: TrochoidReturn::Microlift { height: 0.3 },
        smoothing: 0.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: Some(Height::fixed(22.0)),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 3.0,
        feed: 800.0,
        return_feed: 2000.0,
        plunge_feed: 200.0,
        climb: true,
        picks: Default::default(),
    }
}

fn job(width: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 80.0, dy: 60.0, dz: 20.0 }, Location::default());
    h.execute(&world, stock.expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "Troch1", TOOL, SrcRef::Stock, trochoid(width)).expect("create"));
    (world, h)
}

fn params(world: &World) -> Option<OpParams> {
    let e = model::find_by_name(world, "Troch1")?;
    model::operation_of(world, &e).map(|o| o.params)
}

#[test]
fn a_trochoidal_slot_creates_and_edits_through_the_history() {
    let (world, mut h) = job(10.0);
    assert_eq!(params(&world).map(|p| p.kind()), Some(OpType::TrochoidalSlot));
    let before = params(&world).expect("op");
    let mut edited = before.clone();
    if let OpParams::TrochoidalSlot { step, ret, max_engagement, .. } = &mut edited {
        *step = 0.5;
        *ret = TrochoidReturn::Retract;
        *max_engagement = 70.0;
    }
    h.execute(&world, EditOperationParams::new(&world, "Troch1", edited.clone()).expect("edit"));
    assert_eq!(params(&world), Some(edited.clone()));
    assert!(h.undo(&world));
    assert_eq!(params(&world), Some(before.clone()), "undo restores the old numbers");
    assert!(h.redo(&world));
    assert_eq!(params(&world), Some(edited));
    // Undo the edit and the create: the op is gone; redo brings it back.
    assert!(h.undo(&world));
    assert!(h.undo(&world));
    assert!(params(&world).is_none(), "undo of the create removes the op");
    assert!(h.redo(&world));
    assert_eq!(params(&world), Some(before));
}

#[test]
fn a_trochoidal_slot_generates_verifies_clean_and_posts() {
    use camcraft_app::cl::MoveKind;

    let (world, mut h) = job(10.0);
    h.execute(&world, GenerateOperation::new(&world, "Troch1").expect("generate"));
    let e = model::find_by_name(&world, "Troch1").expect("op");
    let c = model::toolpath_of(&world, &e).expect("cached path");
    assert!(c.path.has_arcs() && !c.path.moves.is_empty());
    let bad: Vec<&Warning> = c.warnings.iter().filter(|w| w.blocks_post()).collect();
    assert!(bad.is_empty(), "{bad:?}");
    assert!(
        !c.warnings.iter().any(|w| matches!(w, Warning::Crossing { .. } | Warning::Gouge { .. })),
        "{:?}",
        c.warnings
    );
    assert!(c.path.notes.iter().any(|n| n.starts_with("trochoidal slot:")), "{:?}", c.path.notes);
    // The loops' floor arcs cross the channel near the entry, middle and exit
    // in that order. Exclude the entry bore, the end cap and lifted retraces.
    let mut across = c.path.moves.iter().filter(|m| {
        m.kind == MoveKind::Feed
            && m.arc.is_some()
            && (m.from.z - 14.0).abs() < 1e-6
            && (m.to.z - 14.0).abs() < 1e-6
            && m.from.y * m.to.y < -2.25
    });
    for section in [-13.0..-8.0, -3.0..3.0, 8.0..13.0] {
        assert!(
            across.any(|m| section.contains(&m.to.x)),
            "no successive floor loop across the slot at {section:?}"
        );
    }
    let nc = std::env::temp_dir().join(format!("camcraft-trochoid-{}.nc", std::process::id()));
    posting::post_setup(&world, &nc).expect("a clean trochoid posts");
    let text = std::fs::read_to_string(&nc).expect("nc");
    assert!(text.contains("G03"), "the loops post as arcs");
    let _ = std::fs::remove_file(&nc);
}

#[test]
fn a_channel_narrower_than_the_tool_is_refused_by_name() {
    let (world, _h) = job(5.0);
    let err = GenerateOperation::new(&world, "Troch1").map(|_| ()).expect_err("refused").to_string();
    assert!(err.contains("too narrow") && err.contains("Slot"), "refused by name: {err}");
}

#[test]
fn the_trochoidal_slot_round_trips_through_the_project_file() {
    let (world, mut h) = job(10.0);
    let mut p = trochoid(10.0);
    if let OpParams::TrochoidalSlot { loop_width, ret, .. } = &mut p {
        *loop_width = Some(3.0);
        *ret = TrochoidReturn::Microlift { height: 0.4 };
    }
    h.execute(&world, EditOperationParams::new(&world, "Troch1", p.clone()).expect("edit"));
    let path = std::env::temp_dir().join(format!("camcraft_trochoid_rt_{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("\"trochoidal_slot\""), "the params carry their tag");
    assert!(!text.contains("\"pass_start\""), "the project stores no pass order for this operation");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    assert_eq!(params(&world2), Some(p));
    let _ = std::fs::remove_file(&path);
}

/// #774: a microlift taller than the clearance above the top (here 10 mm:
/// clearance 30, top 20) would feed the return above the clearance
/// plane, so it is refused by name at creation and at an edit (which
/// leaves the op as it was); a lift right at the bound is accepted.
#[test]
fn a_microlift_above_the_clearance_plane_is_refused_at_the_door() {
    let (world, mut h) = job(10.0);
    let tall = |height: f64| {
        let mut p = trochoid(10.0);
        if let OpParams::TrochoidalSlot { ret, .. } = &mut p {
            *ret = TrochoidReturn::Microlift { height };
        }
        p
    };
    let err = CreateOperation::new(&world, "Troch2", TOOL, SrcRef::Stock, tall(100.0))
        .map(|_| ())
        .expect_err("refused at creation")
        .to_string();
    assert!(err.contains("microlift") && err.contains("clearance plane") && err.contains("10.000"), "{err}");
    let err = EditOperationParams::new(&world, "Troch1", tall(10.5)).map(|_| ()).expect_err("refused at edit").to_string();
    assert!(err.contains("microlift"), "{err}");
    assert_eq!(params(&world), Some(trochoid(10.0)), "the refused edit leaves the op alone");
    h.execute(&world, EditOperationParams::new(&world, "Troch1", tall(10.0)).expect("a lift at the bound is fine"));
    assert_eq!(params(&world), Some(tall(10.0)));
}
