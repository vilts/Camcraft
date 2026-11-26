//! #547 (part of #290): Sinker EDM electrode generation from part faces.
//! A pocket's picked faces (its floor and walls) yield an electrode tool:
//! the cavity offset inward by the condition's undersize, standing a
//! stub above the mouth. It burns that pocket to size in the program
//! verdict (the column engine's box atom); the command is one undo
//! step; and every face set that is no single cavity is refused by
//! name. The MCP tool is pinned in `sinker_electrode_mcp`; the body
//! electrode of a non-rectangular cavity (#1152) in
//! `sinker_electrode_body`.
//!
//! Gate map (#547): covers app::operation::sinker_faces,
//! app::commands::electrode (ElectrodeFromFaces), motion::sinker_cavity,
//! motion::sinker_electrode, app::stock_sim (simulate_document over the
//! box atom).
//! Provenance: our own SYNTHETIC fixtures - the sinker template of #530
//! with invented conditions (no machine's manual) and blocks built in
//! code. Not exercised on a real EDM control.

mod pocket_support;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    CommandError, CreateOperation, ElectrodeFromFaces, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::feature::{self, FaceKind};
use camcraft_app::geometry::Mesh;
use camcraft_app::machine::parse_mch;
use camcraft_app::model;
use camcraft_app::operation::sinker_faces::condition_undersize;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, OpParams, SinkerBurnOp, SinkerDatum, SinkerPass, SrcRef};
use camcraft_app::sinker_electrode::{ElectrodeBox, ElectrodeSpec};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::world::new_world;
use pocket_support::{block_with_pocket, height_field, rounded_rect};

const SINKER: &str = include_str!("edm/sinker-edm.mch");
/// The cavity: 20 x 12, off-centre at (6, -4), floor Z 14 under the
/// 20 mm work top.
const LO: (f64, f64) = (-4.0, -10.0);
const HI: (f64, f64) = (16.0, 2.0);
const FLOOR: f64 = 14.0;
const TOP: f64 = 20.0;

/// The sinker template with SYNTHETIC conditions (no manual's values).
fn sinker_machine() -> String {
    let mut v: serde_json::Value = serde_json::from_str(SINKER).expect("template json");
    v["edm"]["conditions"] = serde_json::json!([
        { "id": "R1", "name": "synthetic rough", "gap": 0.1, "undersize": 0.3, "role": "rough" },
        { "id": "F1", "name": "synthetic finish", "gap": 0.02, "undersize": 0.02, "role": "finish" },
        { "id": "NU", "name": "synthetic, no undersize", "gap": 0.1 }
    ]);
    v.to_string()
}

/// A 60 x 60 x 20 block with the cavity sunk in it.
fn block() -> Mesh {
    height_field(&[-30.0, LO.0, HI.0, 30.0], &[-30.0, LO.1, HI.1, 30.0], |i, j| if (i, j) == (1, 1) { FLOOR } else { TOP })
}

/// `mesh` as the part, its block of stock, the sinker mounted.
fn doc(mesh: Mesh) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", mesh));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("stock"));
    let m = parse_mch(&sinker_machine()).expect("sinker parses");
    h.execute(&world, LoadMachine::new(&world, "sinker.mch".into(), Some(m)).expect("mount"));
    (world, h)
}

/// The planar faces of `model` lying wholly inside the box `lo..hi`
/// (XY) at or above `floor`, as picks: a cavity's floor and walls.
fn faces_in(world: &World, model_name: &str, lo: (f64, f64), hi: (f64, f64), floor: f64) -> Vec<GeoSel> {
    let e = model::find_by_name(world, model_name).expect("model");
    let f = model::features_of(world, &e).expect("features");
    let eps = 1e-6;
    f.faces
        .iter()
        .filter(|fa| matches!(fa.kind, FaceKind::Planar))
        .filter(|fa| {
            let mut pts = fa.loops.iter().flat_map(|l| l.verts.iter()).filter_map(|&v| f.verts.get(v as usize));
            pts.all(|p| p[0] >= lo.0 - eps && p[0] <= hi.0 + eps && p[1] >= lo.1 - eps && p[1] <= hi.1 + eps && p[2] >= floor - eps)
        })
        .map(|fa| GeoSel { entity: model_name.to_owned(), feature: FeatureSel::Face(fa.id) })
        .collect()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn spec(undersize: f64) -> ElectrodeSpec {
    ElectrodeSpec { undersize, orbit: None, stub: 10.0, pickup: 15.0 }
}

fn refusal(world: &World, picks: &[GeoSel], s: &ElectrodeSpec) -> String {
    match ElectrodeFromFaces::new(world, "E", picks, s) {
        Ok(c) => panic!("expected a refusal, got {:?}", c.electrode()),
        Err(e) => e.to_string(),
    }
}

fn burn(e: &ElectrodeBox, pass: SinkerPass, condition: &str) -> OpParams {
    OpParams::SinkerBurn(SinkerBurnOp {
        clearance_z: Height::fixed(e.top_z + 5.0),
        top_z: Height::fixed(e.top_z),
        floor_z: Height::fixed(e.floor_z),
        datum: SinkerDatum::WorkTop,
        depth: e.depth,
        x: e.center.0,
        y: e.center.1,
        feed: 40.0,
        condition: condition.into(),
        pass,
        cavity_x: e.cavity.0,
        cavity_y: e.cavity.1,
        cavity_outline: Vec::new(),
        orbit: None,
        layer: None,
        wear_lost: None,
        wear_over: None,
        picks: Default::default(),
    })
}

/// Generate the electrode `name` for `condition` from the cavity's
/// faces and burn the cavity with it as `pass`.
fn electrode_and_burn(world: &World, h: &mut History, name: &str, condition: &str, pass: SinkerPass) -> ElectrodeBox {
    let picks = faces_in(world, "part", LO, HI, FLOOR);
    assert_eq!(picks.len(), 5, "the floor and four walls");
    let u = condition_undersize(world, None, condition).expect("the condition's undersize");
    let cmd = ElectrodeFromFaces::new(world, name, &picks, &spec(u)).expect("an electrode from the pocket's faces");
    let e = cmd.electrode().clone();
    h.execute(world, cmd);
    let op = format!("Burn {name}");
    h.execute(world, CreateOperation::new(world, &op, name, SrcRef::Stock, burn(&e, pass, condition)).expect("a sinker burn"));
    h.execute(world, GenerateOperation::new(world, &op).expect("generates"));
    e
}

#[test]
fn a_pockets_faces_yield_an_electrode_that_burns_that_pocket_in_the_sim() {
    let (world, mut h) = doc(block());
    // The rough electrode, sized by R1's 0.3 undersize: it gouges
    // nothing and leaves stock for the finish.
    let rough = electrode_and_burn(&world, &mut h, "E-R", "R1", SinkerPass::Rough);
    assert!((rough.face.0 - 19.4).abs() < 1e-9 && (rough.face.1 - 11.4).abs() < 1e-9, "{:?}", rough.face);
    assert_eq!((rough.center, rough.cavity, rough.floor_z, rough.top_z, rough.depth), ((6.0, -4.0), (20.0, 12.0), FLOOR, TOP, 6.0));
    let sim = simulate_document(&world).expect("judged");
    assert_eq!(sim.dev.gouge_cells, 0, "the generated rough gouges nothing: {:?}", sim.dev.max_gouge_at);
    assert!(!sim.dev.is_clean() && sim.dev.max_rest > 0.15, "the rough leaves stock: max rest {}", sim.dev.max_rest);

    // The finish electrode, sized by F1's 0.02: the pocket is burned.
    let finish = electrode_and_burn(&world, &mut h, "E-F", "F1", SinkerPass::Finish);
    assert!((finish.face.0 - 19.96).abs() < 1e-9 && (finish.face.1 - 11.96).abs() < 1e-9, "{:?}", finish.face);
    let tool = model::tool_of(&world, &model::find_by_name(&world, "E-F").expect("the tool")).expect("a tool");
    assert_eq!(tool.kind, camcraft_app::tools::ToolKind::Electrode);
    assert_eq!((tool.diameter, tool.width, tool.flute_len, tool.shank_len), (finish.face.0, finish.face.1, 16.0, 15.0));
    assert!(tool.is_sane() && tool.torch_refusal().is_none());
    let sim = simulate_document(&world).expect("judged");
    assert!(
        sim.dev.is_clean(),
        "the generated electrodes burn the pocket to size: max rest {} at {:?}, max gouge {} at {:?}",
        sim.dev.max_rest,
        sim.dev.max_rest_at,
        sim.dev.max_gouge,
        sim.dev.max_gouge_at
    );
}

#[test]
fn the_electrode_is_one_undo_step_and_its_name_is_checked() {
    let (world, mut h) = doc(block());
    let picks = faces_in(&world, "part", LO, HI, FLOOR);
    h.execute(&world, ElectrodeFromFaces::new(&world, "E-GEN", &picks, &spec(0.1)).expect("electrode"));
    assert!(model::find_by_name(&world, "E-GEN").is_some_and(|e| model::is_tool(&world, &e)));
    assert!(matches!(ElectrodeFromFaces::new(&world, "E-GEN", &picks, &spec(0.1)), Err(CommandError::NameConflict(_))));
    assert!(matches!(ElectrodeFromFaces::new(&world, "", &picks, &spec(0.1)), Err(CommandError::Invalid(_))));
    assert!(h.undo(&world));
    assert!(model::find_by_name(&world, "E-GEN").is_none(), "undo removes the electrode");
    assert!(h.redo(&world));
    let t = model::tool_of(&world, &model::find_by_name(&world, "E-GEN").expect("redo restores it")).expect("a tool");
    assert!(near(t.diameter, 19.8) && near(t.width, 11.8), "{t:?}");
    // The pick order and a face picked twice change nothing.
    let mut shuffled: Vec<GeoSel> = picks.iter().rev().cloned().collect();
    shuffled.push(picks[0].clone());
    let again = ElectrodeFromFaces::new(&world, "E-2", &shuffled, &spec(0.1)).expect("electrode");
    let e = again.electrode();
    assert!(near(e.face.0, 19.8) && near(e.face.1, 11.8) && e.center == (6.0, -4.0), "{e:?}");
}

#[test]
fn the_condition_door_reads_the_undersize_or_refuses_by_name() {
    let (world, mut h) = doc(block());
    assert_eq!(condition_undersize(&world, None, "F1"), Ok(0.02));
    assert_eq!(condition_undersize(&world, None, " R1 "), Ok(0.3));
    let err = |id: &str| condition_undersize(&world, None, id).expect_err("refused");
    assert!(err("ZZ").contains("states no EDM condition \"ZZ\""), "{}", err("ZZ"));
    assert!(err("NU").contains("states no undersize"), "{}", err("NU"));
    assert!(err("  ").contains("no EDM condition is named"), "{}", err("  "));
    // Off a sinker: the machine's kind is named.
    h.execute(&world, LoadMachine::new(&world, String::new(), None).expect("unmount"));
    assert!(err("F1").contains("sinker EDM's condition"), "{}", err("F1"));
}

#[test]
fn a_face_set_that_is_no_single_cavity_is_refused_by_name() {
    let (world, mut h) = doc(block());
    let picks = faces_in(&world, "part", LO, HI, FLOOR);
    let s = spec(0.1);
    assert!(refusal(&world, &[], &s).contains("no faces are picked"));
    // An open loop of walls, and the floor alone.
    for skip in 0..picks.len() {
        let some: Vec<GeoSel> = picks.iter().enumerate().filter(|(i, _)| *i != skip).map(|(_, g)| g.clone()).collect();
        let why = refusal(&world, &some, &s);
        assert!(why.contains("do not close around the floor") || why.contains("no floor is picked"), "{why}");
    }
    // A stale id, a chain, a vertex, a face that is the part's own side.
    let stale = GeoSel { entity: "part".into(), feature: FeatureSel::Face(0xDEAD_BEEF) };
    assert!(refusal(&world, &[stale], &s).contains("no longer in the model"));
    for f in [FeatureSel::Chain(1), FeatureSel::Vertex(1)] {
        let mut with = picks.clone();
        with.push(GeoSel { entity: "part".into(), feature: f });
        assert!(refusal(&world, &with, &s).contains("is not a face"));
    }
    let all = faces_in(&world, "part", (-30.0, -30.0), (30.0, 30.0), 0.0);
    let why = refusal(&world, &all, &s);
    assert!(why.contains("faces down") || why.contains("floors are picked") || why.contains("below the picked floor"), "{why}");
    // The numbers: an undersize that eats the cavity, a garbled stub.
    assert!(refusal(&world, &picks, &spec(6.0)).contains("leaves no electrode"));
    assert!(refusal(&world, &picks, &spec(f64::NAN)).contains("undersize"));
    assert!(refusal(&world, &picks, &ElectrodeSpec { stub: 0.0, ..s }).contains("holder stub"));
    // Picks on two models, and on something that is no part.
    h.execute(&world, ImportModel::from_mesh("other", block()));
    let mut two = picks.clone();
    two.extend(faces_in(&world, "other", LO, HI, FLOOR));
    assert!(refusal(&world, &two, &s).contains("different models"));
    let ghost = GeoSel { entity: "nobody".into(), feature: FeatureSel::Face(1) };
    assert!(refusal(&world, &[ghost], &s).contains("no model named 'nobody'"));
    let stock_name = camcraft_app::setup::stock_entity_name(&world, None);
    let on_stock = GeoSel { entity: stock_name, feature: FeatureSel::Face(1) };
    assert!(refusal(&world, &[on_stock], &s).contains("is not a part"));
}

#[test]
fn faces_from_different_cavities_are_refused_by_name() {
    // Two pockets side by side: A (floor 14) and B (floor 10).
    let mesh = height_field(&[-30.0, -20.0, -10.0, 0.0, 10.0, 20.0, 30.0], &[-30.0, -10.0, 10.0, 30.0], |i, j| match (i, j) {
        (1, 1) => 14.0,
        (4, 1) => 10.0,
        _ => TOP,
    });
    let (world, _h) = doc(mesh);
    let a = faces_in(&world, "part", (-20.0, -10.0), (-10.0, 10.0), 14.0);
    let b = faces_in(&world, "part", (10.0, -10.0), (20.0, 10.0), 10.0);
    assert_eq!((a.len(), b.len()), (5, 5));
    let s = spec(0.1);
    assert!(ElectrodeFromFaces::new(&world, "E-A", &a, &s).is_ok() && ElectrodeFromFaces::new(&world, "E-B", &b, &s).is_ok());
    let both: Vec<GeoSel> = a.iter().chain(b.iter()).cloned().collect();
    let why = refusal(&world, &both, &s);
    assert!(why.contains("2 floors are picked") && why.contains("different cavities"), "{why}");
    // A's faces with one of B's walls swapped in for one of A's own.
    for wall_of_b in &b {
        let mut mixed = a.clone();
        mixed.push(wall_of_b.clone());
        let why = refusal(&world, &mixed, &s);
        assert!(
            why.contains("different cavit") || why.contains("floors are picked") || why.contains("below the picked floor"),
            "{why}"
        );
    }
}

#[test]
fn a_boss_in_the_cavity_is_in_the_electrodes_way() {
    // A 20 x 20 cavity (floor 14) with a 4 x 4 boss standing 3 mm on it.
    let cuts = [-30.0, -10.0, -2.0, 2.0, 10.0, 30.0];
    let mesh = height_field(&cuts, &cuts, |i, j| match (i, j) {
        (2, 2) => 17.0,
        (1..=3, 1..=3) => 14.0,
        _ => TOP,
    });
    let (world, _h) = doc(mesh);
    // The floor and the four cavity walls - not the boss's own faces.
    let picks: Vec<GeoSel> = faces_in(&world, "part", (-10.0, -10.0), (10.0, 10.0), 14.0)
        .into_iter()
        .filter(|g| faces_in(&world, "part", (-2.0, -2.0), (2.0, 2.0), 14.0).iter().all(|b| b != g))
        .collect();
    assert_eq!(picks.len(), 5, "the ring floor and four walls");
    let why = refusal(&world, &picks, &spec(0.1));
    assert!(why.contains("stands inside the cavity above its floor") && why.contains("in the electrode's way"), "{why}");
}

#[test]
fn a_rounded_pockets_corner_picked_as_its_welded_face_reads_as_its_strips() {
    // A 20 x 12 pocket with 2 mm corner radii, 6 deep. Its planar faces
    // yield a body electrode (#1152, `sinker_electrode_body`).
    let mesh = block_with_pocket(30.0, 30.0, &rounded_rect(10.0, 6.0, 2.0, 4), FLOOR, TOP);
    let (world, _h) = doc(mesh);
    let picks = faces_in(&world, "part", (-10.0, -6.0), (10.0, 6.0), FLOOR);
    assert!(picks.len() > 5, "the floor, four flat walls and the corner strips: {}", picks.len());
    let planar = ElectrodeFromFaces::new(&world, "E", &picks, &spec(0.1)).expect("the strips' electrode");
    assert!(planar.electrode().body);
    // A corner picked as its welded (curved) face as well (#1808): its
    // vertical strips are walls already picked, so nothing changes.
    let e = model::find_by_name(&world, "part").expect("part");
    let f = model::features_of(&world, &e).expect("features");
    let patch = feature::patches(&f).into_iter().next().expect("a rounded corner welds into a patch");
    let mut curved = picks.clone();
    curved.push(GeoSel { entity: "part".into(), feature: FeatureSel::Patch(patch.id) });
    let welded = ElectrodeFromFaces::new(&world, "E", &curved, &spec(0.1)).expect("the welded pick's electrode");
    assert_eq!(welded.electrode(), planar.electrode());
}

#[test]
fn the_verdict_convicts_the_generated_electrode_burned_off_its_point() {
    // The contrast to the clean burn: the same generated electrode fed
    // down 1 mm off the cavity's centre burns into a wall, and the
    // program verdict says so - the sim judges the electrode, it does
    // not wave it through.
    let (world, mut h) = doc(block());
    let picks = faces_in(&world, "part", LO, HI, FLOOR);
    let cmd = ElectrodeFromFaces::new(&world, "E", &picks, &spec(0.02)).expect("electrode");
    let e = cmd.electrode().clone();
    h.execute(&world, cmd);
    let mut p = burn(&e, SinkerPass::Single, "F1");
    if let OpParams::SinkerBurn(op) = &mut p {
        op.x += 1.0;
    }
    h.execute(&world, CreateOperation::new(&world, "Off", "E", SrcRef::Stock, p).expect("a sinker burn"));
    h.execute(&world, GenerateOperation::new(&world, "Off").expect("generates"));
    let sim = simulate_document(&world).expect("judged");
    assert!(sim.dev.gouge_cells > 0 && sim.dev.max_gouge > 0.5, "an off-centre burn gouges the wall: max gouge {}", sim.dev.max_gouge);
}
