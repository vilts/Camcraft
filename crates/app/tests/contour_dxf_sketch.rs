//! #1864: 2D Contour on DXF sketches - whole-plate nesting, G2/G3 arcs,
//! layer/chain picking, per-chain side and direction overrides, open-chain
//! safety, and project v74 roundtrip.
//!
//! Gate map (M4.4): covers app::operation (contour_sketch, contour_sketch_pick,
//! job, generate, types), app::project (file, area, params, wire), app::sketch_model.
//! Provenance: our own fixtures over our own commands, stock sim, and post
//! processor; DXF geometry uses public Autodesk ASCII DXF group codes.

use std::path::{Path, PathBuf};

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, Command, CommandError, CreateOperation, DxfOptions, EditOperationParams,
    GenerateOperation, History, ImportDxf, SetOperationDoNotTouch, SetStock,
};
use camcraft_app::geometry::sketch_chains::ChainId;
use camcraft_app::links::{Approach, Compensation, Links};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{
    self, ChainOverride, ChainSide, FeatureSel, GeoSel, Height, HeightReference, OpParams,
    OpStatus, OpType, SrcRef,
};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::sketch;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0400-2FL"; // 4 mm endmill -> tool radius 2 mm

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-1864-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// Plate DXF (world XY when stock sits at origin):
/// - Layer "cut": outer rectangle 0..120 x 0..80 (closed)
/// - Layer "holes":
///   - Circle radius 8 centered at (25, 55) (closed, Hole)
///   - Circle radius 6 centered at (95, 55) (closed, Hole)
///   - Line-arc stadium slot R=5 centered at y=25, x=45..75 (closed, Hole)
/// - Layer "open": open line (20, 10) -> (100, 10) (open)
fn plate_dxf() -> &'static str {
    "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
     0\nLWPOLYLINE\n8\ncut\n90\n4\n70\n1\n10\n0\n20\n0\n10\n120\n20\n0\n10\n120\n20\n80\n10\n0\n20\n80\n\
     0\nCIRCLE\n8\nholes\n10\n25\n20\n55\n40\n8\n\
     0\nCIRCLE\n8\nholes\n10\n95\n20\n55\n40\n6\n\
     0\nLINE\n8\nholes\n10\n45\n20\n20\n11\n75\n21\n20\n\
     0\nARC\n8\nholes\n10\n75\n20\n25\n40\n5\n50\n270\n51\n90\n\
     0\nLINE\n8\nholes\n10\n75\n20\n30\n11\n45\n21\n30\n\
     0\nARC\n8\nholes\n10\n45\n20\n25\n40\n5\n50\n90\n51\n270\n\
     0\nLINE\n8\nopen\n10\n20\n20\n10\n11\n100\n21\n10\n\
     0\nENDSEC\n0\nEOF\n"
}

fn write_dxf(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).expect("write dxf");
    path
}

fn run(world: &World, history: &mut History, cmd: Result<impl Command + 'static, CommandError>, what: &str) {
    history.execute(world, cmd.unwrap_or_else(|e| panic!("{what}: {e}")));
}

fn sheet_world(tag: &str) -> (PathBuf, World, History) {
    let dir = tmp_dir(tag);
    let world = new_world();
    let mut history = History::default();
    let shape = StockShape::Box { dx: 300.0, dy: 240.0, dz: 10.0 };
    run(&world, &mut history, SetStock::new(&world, shape, Location::default()), "sheet stock");
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    run(&world, &mut history, AddTool::new(&world, TOOL, tool), "tool");
    let path = write_dxf(&dir, "plate.dxf", plate_dxf());
    run(&world, &mut history, ImportDxf::new(&world, None, &path, &DxfOptions::default()), "import dxf");
    (dir, world, history)
}

fn stock_contour_params() -> OpParams {
    let mut params = operation::seed_params(OpType::Contour.ordinal());
    if let OpParams::Contour {
        clearance_z,
        retract_z,
        top_z,
        bottom_z,
        stepdown,
        lead,
        tolerance,
        ..
    } = &mut params
    {
        *clearance_z = Height::of(HeightReference::TopOfStock, 10.0);
        *retract_z = Height::of(HeightReference::TopOfStock, 5.0);
        *top_z = Height::of(HeightReference::TopOfStock, 0.0);
        *bottom_z = Height::of(HeightReference::TopOfStock, -2.0);
        *stepdown = 2.0;
        *lead = Lead::None;
        *tolerance = 0.005;
    }
    params
}

struct PlateIds {
    outer: ChainId,
    big_circle: ChainId,
    small_circle: ChainId,
    open_line: ChainId,
}

fn plate_ids(world: &World) -> PlateIds {
    let (_, sk) = sketch::find(world, "plate").expect("plate sketch");
    let cut = sk.data.chains().by_layer("cut");
    assert_eq!(cut.len(), 1);
    let mut big_circle = None;
    let mut small_circle = None;
    for ch in sk.data.chains().by_layer("holes") {
        let len = ch.length();
        if (len - std::f64::consts::TAU * 8.0).abs() < 0.1 {
            big_circle = Some(ch.id());
        } else if (len - std::f64::consts::TAU * 6.0).abs() < 0.1 {
            small_circle = Some(ch.id());
        }
    }
    let open = sk.data.chains().by_layer("open");
    assert_eq!(open.len(), 1);
    PlateIds {
        outer: cut[0].id(),
        big_circle: big_circle.expect("R8 circle"),
        small_circle: small_circle.expect("R6 circle"),
        open_line: open[0].id(),
    }
}

#[test]
fn whole_plate_auto_nesting_cuts_outer_outside_holes_inside_and_preserves_g2_g3_arcs() {
    let (dir, world, mut history) = sheet_world("plate-auto");
    let params = stock_contour_params();
    run(&world, &mut history, CreateOperation::new(&world, "Cut", TOOL, SrcRef::Entity("plate".into()), params), "create contour");
    run(&world, &mut history, GenerateOperation::new(&world, "Cut"), "generate contour on plate");

    let op_entity = model::find_by_name(&world, "Cut").expect("Cut op");
    assert_eq!(operation::status_of(&world, &op_entity), OpStatus::Ok);
    let cache = model::toolpath_of(&world, &op_entity).expect("cached toolpath");
    assert!(cache.warnings.is_empty(), "clean plate verify warnings: {:?}", cache.warnings);

    // Outer rectangle (0..120 x 0..80) offset outward by 2mm tool radius -> -2..122 x -2..82.
    let feeds: Vec<_> = cache
        .path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.to.z - 8.0).abs() < 1e-6)
        .collect();
    assert!(!feeds.is_empty());
    let min_x = feeds.iter().map(|m| m.to.x).fold(f64::INFINITY, f64::min);
    let max_x = feeds.iter().map(|m| m.to.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = feeds.iter().map(|m| m.to.y).fold(f64::INFINITY, f64::min);
    let max_y = feeds.iter().map(|m| m.to.y).fold(f64::NEG_INFINITY, f64::max);
    assert!((min_x - (-2.0)).abs() < 0.02, "outer left x={min_x}");
    assert!((max_x - 122.0).abs() < 0.02, "outer right x={max_x}");
    assert!((min_y - (-2.0)).abs() < 0.02, "outer bottom y={min_y}");
    assert!((max_y - 82.0).abs() < 0.02, "outer top y={max_y}");

    // Open line (y=10, x=20..100) runs along centerline at y = 10.0.
    let open_pts: Vec<_> = feeds
        .iter()
        .filter(|m| m.arc.is_none() && (m.to.y - 10.0).abs() < 1e-4 && m.to.x >= 19.9 && m.to.x <= 100.1)
        .collect();
    assert!(!open_pts.is_empty(), "open chain must cut along y=10 centerline");

    // True CL arcs for R8 circle @ (25, 55) -> offset Inside -> R=6.0, R6 circle @ (95, 55) -> R=4.0,
    // and stadium slot R=5 endcap @ (75, 25)/(45, 25) -> offset Inside -> R=3.0!
    let arcs: Vec<_> = feeds
        .iter()
        .filter_map(|m| m.arc.map(|a| (a, a.radius_sweep(m.from, m.to).0)))
        .collect();
    assert!(!arcs.is_empty(), "expected true arc moves in CL toolpath");
    let has_big = arcs.iter().any(|(a, r)| {
        (a.center.x - 25.0).abs() < 0.02 && (a.center.y - 55.0).abs() < 0.02 && (*r - 6.0).abs() < 0.02
    });
    let has_small = arcs.iter().any(|(a, r)| {
        (a.center.x - 95.0).abs() < 0.02 && (a.center.y - 55.0).abs() < 0.02 && (*r - 4.0).abs() < 0.02
    });
    let has_slot_cap = arcs.iter().any(|(a, r)| {
        (a.center.x - 75.0).abs() < 0.02 && (a.center.y - 25.0).abs() < 0.02 && (*r - 3.0).abs() < 0.02
    });
    assert!(has_big, "R8 circle should cut Inside at R=6: {arcs:?}");
    assert!(has_small, "R6 circle should cut Inside at R=4: {arcs:?}");
    assert!(has_slot_cap, "R5 slot arc should cut Inside at R=3: {arcs:?}");

    // Post to Fanuc CPS and verify true G2/G3 blocks appear in NC output!
    let fanuc_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/posts/fanuc.cps");
    let fanuc_cps = std::fs::read_to_string(&fanuc_path).expect("read fanuc.cps");
    let nc_path = dir.join("plate.nc");
    let posted = posting::post_script(&world, &nc_path, &fanuc_cps).expect("post Fanuc NC");
    assert!(!posted.program.is_empty());
    let has_arc_gcode = posted
        .program
        .lines()
        .any(|l| l.contains("G2 ") || l.contains("G3 ") || l.contains("G02") || l.contains("G03"));
    assert!(has_arc_gcode, "Fanuc G-code should emit G2/G3 arcs:\n{}", posted.program);

    // Stock simulation runs on DXF sketch contour without solid model: removes material cleanly, 0 gouges.
    let sim = simulate_document(&world).expect("simulate DXF plate without solid model");
    assert_eq!(sim.dev.gouge_cells, 0, "no gouges on sketch contour stock sim");
    assert!(sim.surface.verts.iter().any(|v| (v.z - 8.0).abs() < 0.1), "material removed at Z=8");
}

#[test]
fn layer_and_chain_picks_per_chain_overrides_undo_and_project_v74_roundtrip() {
    let (dir, world, mut history) = sheet_world("overrides");
    let ids = plate_ids(&world);

    // Pick layer "holes" plus open_line explicitly (outer rect is excluded!).
    let picks = vec![
        GeoSel { entity: "plate".into(), feature: FeatureSel::SketchLayer("holes".into()) },
        GeoSel { entity: "plate".into(), feature: FeatureSel::SketchChain(ids.open_line) },
    ];
    let mut params = stock_contour_params();
    run(&world, &mut history, CreateOperation::new(&world, "Cut", TOOL, SrcRef::Selection(picks), params.clone()), "create");
    run(&world, &mut history, GenerateOperation::new(&world, "Cut"), "initial gen");
    let op_e = model::find_by_name(&world, "Cut").expect("op");
    assert_eq!(operation::status_of(&world, &op_e), OpStatus::Ok);

    // Flip big_circle to Outside (R8 -> cut radius 10.0!), force small_circle Inside (R6 -> 4.0) with reverse=true,
    // and reverse open_line direction.
    if let OpParams::Contour { chain_overrides, .. } = &mut params {
        *chain_overrides = vec![
            ChainOverride {
                entity: "plate".into(),
                chain: ids.big_circle,
                side: Some(ChainSide::Outside),
                reverse: false,
            },
            ChainOverride {
                entity: "plate".into(),
                chain: ids.small_circle,
                side: Some(ChainSide::Inside),
                reverse: true,
            },
            ChainOverride {
                entity: "plate".into(),
                chain: ids.open_line,
                side: None,
                reverse: true,
            },
        ];
    }
    run(&world, &mut history, EditOperationParams::new(&world, "Cut", params.clone()), "edit overrides");
    assert_eq!(operation::status_of(&world, &op_e), OpStatus::Stale, "editing chain_overrides turns op Stale");
    assert!(history.undo(&world));
    assert_eq!(operation::status_of(&world, &op_e), OpStatus::Ok);
    assert!(history.redo(&world));
    assert_eq!(operation::status_of(&world, &op_e), OpStatus::Stale);

    run(&world, &mut history, GenerateOperation::new(&world, "Cut"), "gen overrides");
    let cache = model::toolpath_of(&world, &op_e).expect("cache");
    let arcs: Vec<_> = cache
        .path
        .moves
        .iter()
        .filter_map(|m| m.arc.map(|a| (a, a.radius_sweep(m.from, m.to).0)))
        .collect();
    let big_outside = arcs.iter().any(|(a, r)| {
        (a.center.x - 25.0).abs() < 0.02 && (a.center.y - 55.0).abs() < 0.02 && (*r - 10.0).abs() < 0.02
    });
    let small_still_inside = arcs.iter().any(|(a, r)| {
        (a.center.x - 95.0).abs() < 0.02 && (a.center.y - 55.0).abs() < 0.02 && (*r - 4.0).abs() < 0.02
    });
    assert!(big_outside, "big_circle overridden to Outside -> R=10: {arcs:?}");
    assert!(small_still_inside, "small_circle Inside -> R=4: {arcs:?}");
    let max_x = cache.path.moves.iter().filter(|m| m.kind != MoveKind::Rapid).map(|m| m.to.x).fold(f64::NEG_INFINITY, f64::max);
    assert!(max_x < 105.0, "unselected outer rect ({:?}) must not be machined: max_x={max_x}", ids.outer);

    // Save v74 project, reopen into fresh world, and verify SketchLayer, SketchChain & chain_overrides roundtrip intact.
    let proj_path = dir.join("plate_v74.camproj");
    project::save(&world, &proj_path).expect("save v74");
    let saved_json = std::fs::read_to_string(&proj_path).expect("read v74 json");
    let v: Value = serde_json::from_str(&saved_json).expect("parse json");
    assert_eq!(v["version"], json!(project::PROJECT_VERSION));
    // #1864 on top of #2055: v73 is the do_not_touch format. A v73 reader
    // must refuse sketch picks and chain overrides, so they need v74.
    assert!(v["version"].as_u64().is_some_and(|n| n >= 74), "sketch picks need the v74 format barrier");

    let fresh = new_world();
    project::open(&fresh, &saved_json).expect("open v74");
    let op_fresh_e = model::find_by_name(&fresh, "Cut").expect("reopened op");
    assert_eq!(operation::status_of(&fresh, &op_fresh_e), OpStatus::Ok, "saved toolpath stamp stays fresh on reopen");
    let op_fresh = model::operation_of(&fresh, &op_fresh_e).expect("op component");
    let OpParams::Contour { chain_overrides, .. } = op_fresh.params else { panic!("Contour") };
    assert_eq!(chain_overrides.len(), 3);
}

#[test]
fn do_not_touch_refuses_a_sketch_chain_or_layer_by_name() {
    // #1864 on top of #2055: protection guards model faces and edge chains.
    // A DXF sketch pick is refused at the command, by name, not skipped.
    let (_dir, world, mut history) = sheet_world("dnt-sketch");
    let ids = plate_ids(&world);
    let params = operation::seed_params(OpType::Offset2D.ordinal());
    run(&world, &mut history, CreateOperation::new(&world, "Off", TOOL, SrcRef::Stock, params), "create");
    for feature in [FeatureSel::SketchChain(ids.big_circle), FeatureSel::SketchLayer("holes".into())] {
        let dnt = operation::DoNotTouch { clearance: 1.0, picks: vec![GeoSel { entity: "plate".into(), feature }] };
        let err = SetOperationDoNotTouch::new(&world, "Off", dnt).err().expect("sketch pick must be refused");
        let msg = err.to_string();
        assert!(msg.contains("sketch pick") && msg.contains("plate"), "{msg}");
    }
}

#[test]
fn open_chain_safety_refuses_side_compensation_and_g41_by_name() {
    let (_dir, world, mut history) = sheet_world("open-safety");
    let ids = plate_ids(&world);
    let open_pick = vec![GeoSel { entity: "plate".into(), feature: FeatureSel::SketchChain(ids.open_line) }];

    let mut p_inside = stock_contour_params();
    if let OpParams::Contour { side, .. } = &mut p_inside {
        *side = Side::Inside;
    }
    run(&world, &mut history, CreateOperation::new(&world, "OpInside", TOOL, SrcRef::Selection(open_pick.clone()), p_inside), "create");
    let err = GenerateOperation::new(&world, "OpInside").err().expect("Side::Inside must refuse on open sketch chain");
    let msg = err.to_string();
    assert!(msg.contains("open sketch chain") && msg.contains("plate"), "{msg}");

    let mut p_ov = stock_contour_params();
    if let OpParams::Contour { chain_overrides, .. } = &mut p_ov {
        *chain_overrides = vec![ChainOverride {
            entity: "plate".into(),
            chain: ids.open_line,
            side: Some(ChainSide::Outside),
            reverse: false,
        }];
    }
    run(&world, &mut history, CreateOperation::new(&world, "OpOv", TOOL, SrcRef::Selection(open_pick.clone()), p_ov), "create");
    let err = GenerateOperation::new(&world, "OpOv").err().expect("per-chain side must refuse on open chain");
    let msg = err.to_string();
    assert!(msg.contains("open sketch chain") && msg.contains("Outside"), "{msg}");

    for comp in [Compensation::Control, Compensation::Wear] {
        let mut p_comp = stock_contour_params();
        if let OpParams::Contour { links, .. } = &mut p_comp {
            *links = Links { approach: Approach { compensation: comp, ..Approach::DEFAULT }, ..Links::PLANAR };
        }
        let op_name = format!("OpComp_{comp:?}");
        run(&world, &mut history, CreateOperation::new(&world, &op_name, TOOL, SrcRef::Selection(open_pick.clone()), p_comp), "create");
        let err = GenerateOperation::new(&world, &op_name).err().expect("G41/G42 must refuse on open sketch chain");
        let msg = err.to_string();
        assert!(msg.contains("does not ride open sketch chain"), "{msg}");
    }

    let p_fwd = stock_contour_params();
    let mut p_rev = stock_contour_params();
    if let OpParams::Contour { chain_overrides, .. } = &mut p_rev {
        *chain_overrides = vec![ChainOverride {
            entity: "plate".into(),
            chain: ids.open_line,
            side: None,
            reverse: true,
        }];
    }
    run(&world, &mut history, CreateOperation::new(&world, "Fwd", TOOL, SrcRef::Selection(open_pick.clone()), p_fwd), "fwd");
    run(&world, &mut history, GenerateOperation::new(&world, "Fwd"), "gen fwd");
    run(&world, &mut history, CreateOperation::new(&world, "Rev", TOOL, SrcRef::Selection(open_pick), p_rev), "rev");
    run(&world, &mut history, GenerateOperation::new(&world, "Rev"), "gen rev");
    let f_moves = &model::toolpath_of(&world, &model::find_by_name(&world, "Fwd").unwrap()).unwrap().path.moves;
    let r_moves = &model::toolpath_of(&world, &model::find_by_name(&world, "Rev").unwrap()).unwrap().path.moves;
    let first_feed_x = |moves: &[camcraft_app::cl::Move]| {
        moves.iter().find(|m| m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < 1e-6).map(|m| (m.from.x, m.to.x)).unwrap()
    };
    let (f0, f1) = first_feed_x(f_moves);
    let (r0, r1) = first_feed_x(r_moves);
    assert!((f0 - r1).abs() < 0.01 && (f1 - r0).abs() < 0.01, "reverse should swap open cut endpoints: ({f0}->{f1}) vs ({r0}->{r1})");
}
