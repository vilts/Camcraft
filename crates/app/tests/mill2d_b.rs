//! M2.4 B acceptance: the 2D mill's three new names over the
//! kernels' existing laws. Engrave is the trace, named - byte-for-
//! byte, with the digest's head tag the only difference. The 2D
//! Chamfer posts the derived corner and its congruence doors live in
//! the app's dispatch where the Tool is visible, so the refusals
//! must name the tool the user mounted. Circular is the pocket
//! family's heights on an authored disk - flat rings and the vault's
//! paraboloid - and the wire carries all three under the v43 rungs.
//!
//! Provenance: catalog naming per Autodesk's published HSMWorks
//! user documentation (behavior only); the cone ride is machine-tool
//! handbook geometry written clean-room (the chamfer mill's tangent
//! generatrix), the vault a stated paraboloid of revolution.
//!
//! Gate map (M4.4): covers app::commands, app::geometry, motion::machining2d,
//! app::model, app::operation, app::project, app::schema, geometry::geometry::stock,
//! tools::tools, app::world.

mod common;

use flecs_ecs::prelude::World;

use common::gen_to_cltext;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::geo2d::Point2;
use camcraft_app::machining2d::Side;
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, KindInfo, OpParams, OpStatus, OpType, SrcRef};
use camcraft_app::project;
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool, ToolKind};
use camcraft_app::world::new_world;

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn add_tool(world: &World, history: &mut History, tool: Tool, name: &str) {
    let cmd = AddTool::new(world, name, tool);
    history.execute(world, cmd.expect("tool"));
}

/// The job every equivalence test runs: 60x60x20 box stock (z 0..20)
/// and the named tool - the region is the stock's own silhouette, so
/// the twins and the trace can disagree about nothing but their name.
fn job_tool(tag: &str, tool: Tool, tool_name: &str, params: OpParams) -> (World, History, Toolpath) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
            Location::default(),
        )
        .expect("box stock"),
    );
    add_tool(&world, &mut history, tool, tool_name);
    history.execute(
        &world,
        CreateOperation::new(&world, tag, tool_name, SrcRef::Stock, params).expect("create"),
    );
    let cmd = GenerateOperation::new(&world, tag).expect("generate");
    history.execute(&world, cmd);
    let e = model::find_by_name(&world, tag).expect("op exists");
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok, "{tag} reads Stale against its own lone world");
    let path = model::toolpath_of(&world, &e).expect("cached").path;
    (world, history, path)
}

fn job(tag: &str, params: OpParams) -> (World, History, Toolpath) {
    job_tool(tag, catalog_tool("EM-0800-4FL"), "EM-0800-4FL", params)
}

fn text(params: OpParams, tag: &str) -> String {
    let (_w, _h, path) = job(tag, params);
    gen_to_cltext(&path)
}

fn engrave() -> OpParams {
    OpParams::Engrave {
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(10.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        tilt: None,
    }
}

fn trace() -> OpParams {
    OpParams::Trace {
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(10.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        tilt: None,
    }
}

fn chamfer(width: f64) -> OpParams {
    OpParams::Chamfer2D {
        width,
        angle_deg: 45.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        tolerance: 0.01,
        tilt: None,
    }
}

fn circular(vault: bool, stepover: f64, radius: f64) -> OpParams {
    OpParams::Circular {
        links: camcraft_app::links::Links::PLANAR,
        center: Point2::new(0.0, 0.0),
        radius,
        vault,
        stepover,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(10.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
    }
}

/// A world with stock and one mounted tool, no operation: the door
/// tests call `operation::generate` directly so the refusal arrives
/// from the dispatch, not from a command's veto.
fn door_world(tool: Tool, name: &str) -> World {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default())
            .expect("box stock"),
    );
    add_tool(&world, &mut history, tool, name);
    world
}

fn refuse(world: &World, tool_name: &str, params: OpParams) -> String {
    let op = Operation {
        tool: tool_name.into(),
        src: SrcRef::Stock,
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    match operation::generate(world, &op, "") {
        Ok((path, _)) => panic!("a door was meant to refuse, got {} moves", path.moves.len()),
        Err(e) => e.to_string(),
    }
}

#[test]
fn registry_rows_hold_the_three_names_after_the_twins() {
    // The row law: tag == index across the table, the trio sits in
    // the 2D mill's run right behind 2D Offset, and the labels are
    // the catalog's words (M6.2's ribbon draws them straight).
    for (i, row) in KindInfo::ALL.iter().enumerate() {
        assert_eq!(row.tag as usize, i, "row {}: tag says {}", i, row.tag);
    }
    let (e, c, r) = (
        OpType::Engrave.ordinal(),
        OpType::Chamfer2D.ordinal(),
        OpType::Circular.ordinal(),
    );
    assert_eq!((e, c, r), (7, 8, 9), "the trio follows the twins");
    assert_eq!((KindInfo::ALL[e].label, KindInfo::ALL[c].label, KindInfo::ALL[r].label), ("Engrave", "2D Chamfer", "Circular"));
    assert_eq!(KindInfo::ALL[e].ty, OpType::Engrave);
    assert_eq!(KindInfo::ALL[4].ty, OpType::Trace, "the engrave's twin is still row 4");
}

#[test]
fn engrave_is_the_trace_named() {
    // The twin law, stated twice: the same numbers under the other
    // name write the same program byte for byte - and the digest
    // still separates the rows, because the kind tag rides its head.
    assert_eq!(
        text(engrave(), "ENG"),
        text(trace(), "TRC"),
        "the name must not move the G-code"
    );
    // The stamp digests the WORLD's inputs too - tool and stock must
    // be mounted for it to resolve at all (a bare world's stamp is
    // None, and None equals None without saying anything).
    let w = door_world(catalog_tool("EM-0800-4FL"), "EM-0800-4FL");
    let src = SrcRef::Stock;
    let st = |p: &OpParams| operation::current_stamp(&w, "EM-0800-4FL", &src, p, "X", None);
    assert!(st(&trace()).is_some(), "a mounted job stamps");
    assert_ne!(st(&trace()), st(&engrave()), "the twins are different rows of the table");
    assert_eq!(st(&engrave()), st(&engrave()), "the same promise, the same digest");
}

#[test]
fn the_retype_carries_between_trace_and_engrave() {
    // Every field the twins share is carried (floats cannot ride a
    // pattern - destructured and compared); the seed's own numbers
    // (width, the authored disk) are the new kind's, not a lie
    // imported from the old one.
    let OpParams::Engrave { feed, stepdown, top_z, bottom_z, climb, retract_z, tilt, .. } =
        operation::retype_params(&trace(), OpType::Engrave.ordinal())
    else {
        panic!("the engrave row seeds an engrave");
    };
    assert_eq!((feed, stepdown, climb), (800.0, 5.0, true), "trace's numbers ride the engrave row");
    assert_eq!((top_z, bottom_z), (Height::fixed(20.0), Height::fixed(10.0)));
    assert!(retract_z.is_none() && tilt.is_none(), "the trace's empties stay empty");
    let back = operation::retype_params(&engrave(), OpType::Trace.ordinal());
    assert!(matches!(back, OpParams::Trace { climb: true, .. }), "and back the same way");
    // The trio is not a clique: a trace retyped to the chamfer row
    // takes the shop's seed land, not the trace's depth (the floor
    // is derived there - nothing to carry).
    let OpParams::Chamfer2D { width, angle_deg, tolerance, clearance_z, top_z, .. } =
        operation::retype_params(&trace(), OpType::Chamfer2D.ordinal())
    else {
        panic!("the chamfer row seeds a chamfer");
    };
    assert_eq!((width, angle_deg, tolerance), (0.8, 45.0, 0.01), "the land is the seed's, not the trace's");
    assert_eq!((clearance_z, top_z), (Height::fixed(25.0), Height::fixed(20.0)), "the heights it HAS ride");
    let OpParams::Circular { radius, vault, stock_to_leave, center, .. } =
        operation::retype_params(&trace(), OpType::Circular.ordinal())
    else {
        panic!("the circular row seeds a circular");
    };
    assert_eq!((radius, vault, stock_to_leave), (25.0, false, 0.0), "the disk is authored, not carried");
    assert_eq!(center, Point2::new(0.0, 0.0));
}

#[test]
fn the_chamfer_doors_refuse_by_name() {
    // The congruence doors live where the Tool is visible (M2.4 B):
    // each refusal must name the mounted tool and the number that
    // betrays it - never a silent adaptation.
    let em = catalog_tool("EM-0800-4FL");
    let w = door_world(em.clone(), "EM-0800-4FL");
    let msg = refuse(&w, "EM-0800-4FL", chamfer(2.0));
    assert!(msg.contains("needs a chamfer mill") && msg.contains("'EM-0800-4FL' is a flat end mill"), "{msg}");

    // A cone, but the wrong cone: 60 degrees cannot ride a 45 face.
    let w = door_world(taper(8.0, 60.0, 20.0), "TAPER-60");
    let msg = refuse(&w, "TAPER-60", chamfer(2.0));
    assert!(msg.contains("a 45-degree chamfer takes a 90-degree cone") && msg.contains("has 60 degrees"), "{msg}");

    // The reach door: the shop's 4-wide chamfer mill against a 6
    // land - the disc cannot stand that far off the axis.
    let w = door_world(catalog_tool("CM-0400-90"), "CM-0400-90");
    let msg = refuse(&w, "CM-0400-90", chamfer(3.0));
    assert!(msg.contains("is only 4 mm across") && msg.contains("reaches 3 mm from the wall"), "{msg}");

    // The flute door: a wide enough cone whose flutes stop short of
    // the corner depth.
    let w = door_world(taper(8.0, 90.0, 3.0), "TAPER-SHORT");
    let msg = refuse(&w, "TAPER-SHORT", chamfer(4.0));
    // The derived depth speaks as a tenth, never as float bits -
    // tan(45 degrees) would otherwise say "3.9999999999999996 mm".
    assert!(msg.contains("is 3 mm of flute, short of the 4.0 mm depth"), "{msg}");
}

fn taper(diameter: f64, tip_angle: f64, flute_len: f64) -> Tool {
    Tool {
        kind: ToolKind::Taper,
        diameter,
        corner_r: 0.0,
        tip_angle,
        pitch: 0.0,
        nose_radius: 0.0,
        width: 0.0,
        flute_len,
        shank_d: diameter,
        shank_len: 40.0,
        holder_d: 44.0,
        holder_len: 35.0,
        material_group: "aluminum".into(), center_cutting: true,
        rpm: 8000.0,
        feed: 1500.0,
        tool_material: None,
        flutes: None,
        orientation: None,
        turning: None,
    }
}

#[test]
fn the_chamfer_rides_the_corner_the_door_let_through() {
    // The positive case, over the real command path: the 6-wide 90
    // cone on a 2 land at 45 degrees posts z = 20 - 2*tan(45) = 18
    // for every feed move of the stock's boundary circuit.
    let (w, _h, path) = job_tool("CHM", catalog_tool("CM-0600-90"), "CM-0600-90", chamfer(2.0));
    assert!(path.is_chained() && path.all_finite());
    let feeds: Vec<_> = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    assert!(!feeds.is_empty());
    for m in &feeds {
        assert!((m.to.z - 18.0).abs() < 1e-9, "the corner ride stays at 18: {m:?}");
    }
    // The verdict, not just the ride: the envelope scan must find
    // this cone clean on this stock - no gouge claim, no rest
    // complaint (M2.5 E B law restated for the chamfer's congruent
    // kernel: a warned ride reads Ok to the pane and posts anyway,
    // so the warning list is the only surface this regression
    // class shows on).
    let e = model::find_by_name(&w, "CHM").expect("op");
    let cached = model::toolpath_of(&w, &e).expect("cached");
    assert!(
        cached.warnings.is_empty(),
        "the chamfer ride drew a verdict: {:?}",
        cached.warnings
    );
}

#[test]
fn circular_flat_and_vault_over_the_stock_law() {
    // Flat: every feed move rides a ring of the authored disk - the
    // first at R - r, each next a stepover in, none outside the
    // cutter-center circle.
    let (_w, _h, path) = job("CIR", circular(false, 3.0, 25.0));
    for m in path.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        let rho = m.to.x.hypot(m.to.y);
        assert!(rho <= 21.0 + 1e-9 && rho > 0.0, "off-disk ring: {m:?}");
    }
    // Vault: the same rings stand at or above the paraboloid under
    // their own centers (the kernel's never-gouge law, end to end).
    let (_w, _h, vpath) = job("CIRV", circular(true, 3.0, 25.0));
    for m in vpath.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        let rho = m.to.x.hypot(m.to.y);
        let bowl = 10.0 + 10.0 * (rho / 25.0).min(1.0).powi(2);
        assert!(m.to.z >= bowl - 1e-9, "a vault cut below the dome: {m:?}");
    }
}

#[test]
fn the_circular_doors_refuse_by_name() {
    // The disk states its own reach doors through the app's path.
    let w = door_world(catalog_tool("EM-0800-4FL"), "EM-0800-4FL");
    let msg = refuse(&w, "EM-0800-4FL", circular(false, 5.0, 25.0));
    assert!(msg.contains("stepover must stay within the cutter radius"), "{msg}");
    let msg = refuse(&w, "EM-0800-4FL", circular(false, 3.0, 3.0));
    assert!(msg.contains("the cutter has no room inside the circle"), "{msg}");
}

#[test]
fn the_open_run_refusal_names_the_engrave_too() {
    // M2.4 A proved the twins inherit the closed-chain law; the
    // engrave twin inherits it from the trace's catch-all - and the
    // refusal speaks the kind the user picked, not its twin's name.
    use camcraft_app::geometry::feature;
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, catalog_tool("EM-0600-4FL"), "EM-0600-4FL");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    let e = model::find_by_name(&world, "part").expect("part");
    let f = model::features_of(&world, &e).expect("features");
    let open = feature::chains(&f)
        .into_iter()
        .find(|c| !c.closed)
        .map(|c| FeatureSel::Chain(c.id))
        .expect("box chains are open runs");
    let op = Operation {
        tool: "EM-0600-4FL".into(),
        src: SrcRef::Selection(vec![GeoSel { entity: "part".to_owned(), feature: open }]),
        params: engrave(),
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    let err = match operation::generate(&world, &op, "") {
        Ok((path, _)) => panic!("engrave generated from an open run: {} moves", path.moves.len()),
        Err(e) => e.to_string(),
    };
    // Engrave is trace-class, so the trace arm speaks - and it says
    // THIS kind's name, not the twin's (the M2.4 B refusal fix).
    assert!(err.contains("open run") && err.contains("Engrave machines a closed boundary"), "{err}");
}

#[test]
fn the_new_rows_round_trip_through_the_project_file() {
    // The wire law: each variant carries its own tag, and a load
    // lands every number exactly - the PostDto law, three new names.
    for (key, params, tool) in [
        ("\"engrave\"", engrave(), ("EM-0800-4FL", catalog_tool("EM-0800-4FL"))),
        ("\"chamfer2_d\"", chamfer(2.0), ("CM-0600-90", catalog_tool("CM-0600-90"))),
        ("\"circular\"", circular(true, 3.0, 25.0), ("EM-0800-4FL", catalog_tool("EM-0800-4FL"))),
    ] {
        let path = std::env::temp_dir().join(format!(
            "camcraft_m24b_rt_{}_{}.camproj",
            std::process::id(),
            &key[1..key.len() - 1]
        ));
        let world = new_world();
        let mut history = History::default();
        history.execute(
            &world,
            SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default())
                .expect("box stock"),
        );
        add_tool(&world, &mut history, tool.1, tool.0);
        history.execute(
            &world,
            CreateOperation::new(&world, "OP", tool.0, SrcRef::Stock, params.clone()).expect("create"),
        );
        project::save(&world, &path).expect("save");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains(key), "the params carry their tag {key}");
        let world2 = new_world();
        project::load(&world2, &path).expect("load");
        let e = model::find_by_name(&world2, "OP").expect("op");
        let loaded = model::operation_of(&world2, &e).expect("operation component");
        assert_eq!(loaded.params, params, "round trip must land every number of {key}");
        let _ = std::fs::remove_file(&path);
    }
}
