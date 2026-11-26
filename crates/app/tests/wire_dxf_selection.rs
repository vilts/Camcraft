//! A single selected DXF wire profile through the ordinary command and post doors.
//! Gate map (M4.4): covers app::operation (wire_sketch, wire_sketch_arcs, wire_verify, job, stamp),
//! app::sketch_model, app::commands::SetOperationGeometry, app::posting.
//! Provenance: original synthetic geometry and the existing fictional wire machine fixture.

use camcraft_app::commands::{
    AddTool, CreateOperation, DxfOptions, EditOperationParams, GenerateOperation, History,
    ImportDxf, LoadMachine, MoveSketch, SetOperationGeometry, SetStock,
};
use camcraft_app::geometry::Point3;
use camcraft_app::operation::{
    self, FeatureSel, GeoSel, Height, HeightReference, OpParams, OpStatus, SrcRef, WireContourOp,
    WireSide,
};
use camcraft_app::schema::Location;
use camcraft_app::{
    machine, model, posting, project, sketch, stock::StockShape, wire_tools, world::new_world,
};
use flecs_ecs::prelude::World;

#[path = "wire_dxf_selection/arcs.rs"]
mod arcs;
// #3991: the tangent arc lead on a selected chain.
#[path = "wire_dxf_selection/arc_lead.rs"]
mod arc_lead;
// #3332: the exit pick on a selected chain.
#[path = "wire_dxf_selection/exit_pick.rs"]
mod exit_pick;

const FIXTURE: &str = include_str!("edm/wire-iso-fixture.mch");
const DXF: &str = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
    0\nLWPOLYLINE\n8\noutline\n90\n4\n70\n1\n10\n-8\n20\n-3\n10\n8\n20\n-3\n10\n8\n20\n3\n10\n-8\n20\n3\n\
    0\nCIRCLE\n8\nother\n10\n27\n20\n0\n40\n1\n0\nENDSEC\n0\nEOF\n";

fn pick(world: &World, layer: &str) -> GeoSel {
    let (_, drawing) = sketch::find(world, "drawing").expect("drawing");
    let chain = drawing.data.chains().by_layer(layer)[0].id();
    GeoSel {
        entity: "drawing".into(),
        feature: FeatureSel::SketchChain(chain),
    }
}

fn wire_op() -> WireContourOp {
    WireContourOp {
        lead_arc_radius: None,
        condition: "C110".into(),
        top_z: Height::of(HeightReference::TopOfStock, 0.0),
        bottom_z: Height::of(HeightReference::BottomOfStock, 0.0),
        side: WireSide::Punch,
        picks: camcraft_app::path_picks::PathPicks { start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(-4.0, 5.0))], exit: Vec::new() },
        tabs: None,
        capture: true,
        both_pieces_restrained: false,
        stock_centered_in_xy_travel: false,
        taper_deg: 0.0,
        four_axis: None,
        burnout: None,
        clearance: operation::DEFAULT_CLEARANCE,
        skims: Vec::new(),
        sharp_corners: false,
    }
}

fn document(dxf: &str) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let machine = machine::parse_mch(FIXTURE).expect("machine");
    h.execute(
        &world,
        LoadMachine::new(&world, "wire.mch".into(), Some(machine)).expect("mount"),
    );
    h.execute(
        &world,
        AddTool::new(&world, "Wire", wire_tools::wire_tool(0.25)).expect("wire"),
    );
    let at = Location {
        pos: Point3::new(2.0, 3.0, 1.0),
        ..Default::default()
    };
    h.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 100.0,
                dy: 100.0,
                dz: 12.0,
            },
            at,
        )
        .expect("stock"),
    );
    h.execute(
        &world,
        ImportDxf::from_bytes(
            &world,
            None,
            "drawing.dxf",
            dxf.as_bytes(),
            &DxfOptions::default(),
        )
        .expect("DXF"),
    );
    h.execute(
        &world,
        MoveSketch::new(&world, "drawing", 4.0, 2.0, 90.0).expect("place"),
    );
    h.execute(
        &world,
        CreateOperation::new(
            &world,
            "Cut",
            "Wire",
            SrcRef::Entity("drawing".into()),
            OpParams::WireContour(wire_op()),
        )
        .expect("operation"),
    );
    h.execute(
        &world,
        SetOperationGeometry::new(&world, "Cut", vec![pick(&world, "outline")]).expect("pick"),
    );
    (world, h)
}

fn refusal(world: &World) -> String {
    GenerateOperation::new(world, "Cut")
        .err()
        .expect("generation refused")
        .to_string()
}

#[test]
fn placed_chain_posts_one_offset_and_retains_its_identity_on_reopen() {
    let (world, mut h) = document(DXF);
    h.execute(
        &world,
        GenerateOperation::new(&world, "Cut").expect("generate selected chain"),
    );
    let entity = model::find_by_name(&world, "Cut").expect("operation");
    let verdict = operation::wire_verify::wire_verdict(&world, &entity)
        .expect("wire")
        .expect("verifiable");
    assert!(
        verdict.findings.is_empty() && verdict.captured && verdict.slugs.len() == 1,
        "{verdict:?}"
    );
    assert_eq!(verdict.slugs[0].z, 1.0);
    let cache = model::toolpath_of(&world, &entity).expect("cache");
    assert!(cache.path.moves.iter().all(|m| m.to.z == 1.0));
    let doc = serde_json::to_string(&project::collect(&world)).expect("save");
    let back = new_world();
    project::open(&back, &doc).expect("reopen");
    let e = model::find_by_name(&back, "Cut").expect("operation");
    assert_eq!(
        model::operation_of(&back, &e).expect("cut").src,
        SrcRef::Selection(vec![pick(&back, "outline")])
    );
    assert_eq!(operation::status_of(&back, &e), OpStatus::Ok);
    let post = posting::builtin_post_text("Generic ISO Wire EDM").expect("ISO wire post");
    let file =
        std::env::temp_dir().join(format!("camcraft-selected-wire-{}.nc", std::process::id()));
    let nc = posting::post_script(&back, &file, post)
        .expect("verified post")
        .program;
    let _ = std::fs::remove_file(file);
    for line in [
        "FX-THREAD",
        "FX-COND C110",
        "FX-COMP-OFF",
        "G1 X2.835 Y-3.165",
        "G1 X9.165 Y13.165",
    ] {
        assert!(nc.lines().any(|l| l == line), "{line}\n{nc}");
    }
    // Change an unselected curve by less than the chain identity quantum.
    let mut changed: serde_json::Value = serde_json::from_str(&doc).expect("project");
    let drawing = changed["entities"]
        .as_array_mut()
        .expect("entities")
        .iter_mut()
        .find(|e| e["name"] == "drawing")
        .expect("sketch");
    let circle = drawing["sketch"]["primitives"]
        .as_array_mut()
        .expect("primitives")
        .iter_mut()
        .find(|p| p.get("circle").is_some())
        .expect("circle");
    circle["circle"]["r"] = serde_json::json!(1.0000001);
    let edited = new_world();
    project::open(&edited, &changed.to_string()).expect("reopen exact edit");
    assert_eq!(
        pick(&edited, "other"),
        pick(&back, "other"),
        "the stable pick still resolves"
    );
    assert_eq!(
        operation::status_of(
            &edited,
            &model::find_by_name(&edited, "Cut").expect("operation")
        ),
        OpStatus::Stale
    );
}

#[test]
fn curved_pick_survives_and_is_judged_on_its_arcs_and_undo_restores_the_pick() {
    let (world, mut h) = document(DXF);
    h.execute(
        &world,
        SetOperationGeometry::new(&world, "Cut", vec![pick(&world, "other")])
            .expect("select circle"),
    );
    // #2022: a punch reads a selected circle as a round profile. This lead
    // from (-4, 5) to the R1 circle crosses the unselected outline.
    let why = refusal(&world);
    assert!(why.contains("cannot clear unselected DXF primitive"), "{why}");
    // #3937: a die on the circle is cut on its arcs; this start hole lies
    // outside the circle, so the die is refused by name.
    let die = WireContourOp {
        side: WireSide::Die,
        ..wire_op()
    };
    h.execute(
        &world,
        EditOperationParams::new(&world, "Cut", OpParams::WireContour(die)).expect("die"),
    );
    let why = refusal(&world);
    assert!(why.contains("lies outside the profile"), "{why}");
    assert!(h.undo(&world));
    assert!(h.undo(&world));
    h.execute(
        &world,
        GenerateOperation::new(&world, "Cut").expect("restored straight chain"),
    );
    assert!(
        SetOperationGeometry::new(
            &world,
            "Cut",
            vec![pick(&world, "outline"), pick(&world, "other")]
        )
        .is_err()
    );
}

#[test]
fn selected_chain_requires_clear_stock_a_stated_start_and_slug_capture() {
    let (world, mut h) = document(DXF);
    for (params, message) in [
        (
            WireContourOp {
                capture: false,
                both_pieces_restrained: false,
                stock_centered_in_xy_travel: false,
                ..wire_op()
            },
            "slug capture",
        ),
        (
            WireContourOp {
                picks: Default::default(),
                ..wire_op()
            },
            "no start hole",
        ),
        (
            WireContourOp {
                picks: camcraft_app::path_picks::PathPicks { start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(6.0, 5.0))], exit: Vec::new() },
                ..wire_op()
            },
            "inside the profile",
        ),
        (
            WireContourOp {
                picks: camcraft_app::path_picks::PathPicks { start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(-50.0, 5.0))], exit: Vec::new() },
                ..wire_op()
            },
            "inside the box stock",
        ),
        (
            WireContourOp {
                top_z: Height::fixed(10.0),
                ..wire_op()
            },
            "top and bottom of the stock",
        ),
    ] {
        h.execute(
            &world,
            EditOperationParams::new(&world, "Cut", OpParams::WireContour(params)).expect("edit"),
        );
        let why = refusal(&world);
        assert!(why.contains(message), "{why}");
    }
    let (crossed, _) = document(&DXF.replace("other\n10\n27\n20\n0", "other\n10\n0\n20\n7"));
    let why = refusal(&crossed);
    assert!(why.contains("unselected DXF primitive"), "{why}");
}
