//! #1152 (part of #290): the sinker BODY electrode. A cavity that is no
//! axis-aligned rectangle - a rounded-corner pocket, an L - yields an
//! electrode whose tool record carries its section, and the program
//! verdict burns that section with the outline atom: the pocket it was
//! made for is judged clean, a smaller pocket is convicted as a gouge,
//! and the rectangle of the same two sizes (what the box atom would
//! burn) is convicted in the pocket's corners. The record survives the
//! project file, and a re-cut outline marks its burn stale.
//!
//! Gate map (#1152): covers app::stock_sim (simulate_document over the
//! outline atom, ElectrodeAtom), app::stock_sim_carry,
//! app::operation::sinker_faces, app::operation::sinker_op (the size
//! law on an outline), app::operation::stamp_tool (the outline's
//! bytes), app::project::file (ToolDto::outline),
//! app::commands::electrode, motion::sinker_electrode_body.
//! Provenance: our own SYNTHETIC fixtures - the sinker template of #530
//! with invented conditions (no machine's manual) and blocks built in
//! code. Not exercised on a real EDM control.

mod pocket_support;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, ChangeTool, CreateOperation, ElectrodeFromFaces, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::feature::{self, FaceKind};
use camcraft_app::geometry::Mesh;
use camcraft_app::machine::parse_mch;
use camcraft_app::operation::sinker_faces::condition_undersize;
use camcraft_app::operation::{status_of, FeatureSel, GeoSel, Height, OpParams, OpStatus, SinkerBurnOp, SinkerDatum, SinkerPass, SrcRef};
use camcraft_app::sinker_electrode::{ElectrodeBox, ElectrodeSpec};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::tools::{Tool, ToolKind};
use camcraft_app::world::new_world;
use camcraft_app::{model, project};
use pocket_support::{block_with_pocket, height_field, rounded_rect};

const SINKER: &str = include_str!("edm/sinker-edm.mch");
const FLOOR: f64 = 14.0;
const TOP: f64 = 20.0;

/// The sinker template with SYNTHETIC conditions (no manual's values).
fn sinker_machine() -> String {
    let mut v: serde_json::Value = serde_json::from_str(SINKER).expect("template json");
    v["edm"]["conditions"] = serde_json::json!([
        { "id": "R1", "name": "synthetic rough", "gap": 0.1, "undersize": 0.3, "role": "rough" },
        { "id": "F1", "name": "synthetic finish", "gap": 0.02, "undersize": 0.02, "role": "finish" }
    ]);
    v.to_string()
}

/// `mesh` as the part, its 60 x 60 x 20 block of stock, the sinker mounted.
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

/// A 20 x 12 pocket with 2 mm corner radii (4 chords a corner), 6 deep.
fn rounded() -> Mesh {
    block_with_pocket(30.0, 30.0, &rounded_rect(10.0, 6.0, 2.0, 4), FLOOR, TOP)
}

/// The planar faces of the part lying wholly inside the box `lo..hi`
/// (XY) at or above the floor, as picks: a cavity's floor and walls.
fn faces_in(world: &World, lo: (f64, f64), hi: (f64, f64)) -> Vec<GeoSel> {
    let e = model::find_by_name(world, "part").expect("model");
    let f = model::features_of(world, &e).expect("features");
    let eps = 1e-6;
    f.faces
        .iter()
        .filter(|fa| matches!(fa.kind, FaceKind::Planar))
        .filter(|fa| {
            let mut pts = fa.loops.iter().flat_map(|l| l.verts.iter()).filter_map(|&v| f.verts.get(v as usize));
            pts.all(|p| p[0] >= lo.0 - eps && p[0] <= hi.0 + eps && p[1] >= lo.1 - eps && p[1] <= hi.1 + eps && p[2] >= FLOOR - eps)
        })
        .map(|fa| GeoSel { entity: "part".to_owned(), feature: FeatureSel::Face(fa.id) })
        .collect()
}

fn spec(undersize: f64) -> ElectrodeSpec {
    ElectrodeSpec { undersize, orbit: None, stub: 10.0, pickup: 15.0 }
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
        orbit: None,
        layer: None,
        wear_lost: None,
        wear_over: None,
    })
}

/// Generate the electrode `name` for `condition` from `picks` and burn
/// the cavity with it as `pass`.
fn electrode_and_burn(world: &World, h: &mut History, picks: &[GeoSel], name: &str, condition: &str, pass: SinkerPass) -> ElectrodeBox {
    let u = condition_undersize(world, None, condition).expect("the condition's undersize");
    let cmd = ElectrodeFromFaces::new(world, name, picks, &spec(u)).expect("an electrode from the pocket's faces");
    let e = cmd.electrode().clone();
    h.execute(world, cmd);
    let op = format!("Burn {name}");
    h.execute(world, CreateOperation::new(world, &op, name, SrcRef::Stock, burn(&e, pass, condition)).expect("a sinker burn"));
    h.execute(world, GenerateOperation::new(world, &op).expect("generates"));
    e
}

fn tool(world: &World, name: &str) -> Tool {
    model::tool_of(world, &model::find_by_name(world, name).expect("the tool")).expect("a tool")
}

#[test]
fn a_rounded_pockets_faces_yield_an_electrode_whose_burn_is_judged_clean() {
    let (world, mut h) = doc(rounded());
    let picks = faces_in(&world, (-10.0, -6.0), (10.0, 6.0));
    assert_eq!(picks.len(), 21, "the floor, four flat walls and sixteen corner strips");
    // The rough: it gouges nothing and leaves stock for the finish.
    let rough = electrode_and_burn(&world, &mut h, &picks, "E-R", "R1", SinkerPass::Rough);
    assert!(rough.body && (rough.face.0 - 19.4).abs() < 1e-5 && (rough.face.1 - 11.4).abs() < 1e-5, "{rough:?}");
    assert_eq!((rough.center, rough.cavity, rough.floor_z, rough.top_z, rough.depth), ((0.0, 0.0), (20.0, 12.0), FLOOR, TOP, 6.0));
    let sim = simulate_document(&world).expect("judged");
    assert_eq!(sim.dev.gouge_cells, 0, "the generated rough gouges nothing: {:?}", sim.dev.max_gouge_at);
    assert!(!sim.dev.is_clean() && sim.dev.max_rest > 0.15, "the rough leaves stock: max rest {}", sim.dev.max_rest);

    // The finish: the pocket is burned to size, corners and all.
    let finish = electrode_and_burn(&world, &mut h, &picks, "E-F", "F1", SinkerPass::Finish);
    let t = tool(&world, "E-F");
    assert_eq!(t.kind, ToolKind::Electrode);
    assert_eq!(t.outline.len(), finish.outline.len());
    assert!(t.outline.len() >= 20, "the section keeps its corner chords: {}", t.outline.len());
    assert!((t.diameter - 19.96).abs() < 1e-5 && (t.width - 11.96).abs() < 1e-5 && (t.flute_len, t.shank_len) == (16.0, 15.0), "{t:?}");
    assert!(t.is_sane() && t.torch_refusal().is_none());
    let sim = simulate_document(&world).expect("judged");
    assert!(
        sim.dev.is_clean(),
        "the generated electrodes burn the rounded pocket to size: max rest {} at {:?}, max gouge {} at {:?}",
        sim.dev.max_rest,
        sim.dev.max_rest_at,
        sim.dev.max_gouge,
        sim.dev.max_gouge_at
    );

    // The contrast: the rectangle of the same two sizes - the record
    // without its outline, what the box atom burns - passes the same
    // size law and is convicted in the pocket's rounded corners. The
    // verdict burns the section, not its bounds.
    let boxed = Tool { outline: Vec::new(), ..t.clone() };
    h.execute(&world, ChangeTool::new(&world, "E-F", boxed).expect("a rectangular electrode"));
    h.execute(&world, GenerateOperation::new(&world, "Burn E-F").expect("generates"));
    let sim = simulate_document(&world).expect("judged");
    assert!(sim.dev.gouge_cells > 0 && sim.dev.max_gouge > 1.0, "a square corner in a round one: max gouge {}", sim.dev.max_gouge);
    let at = sim.dev.max_gouge_at.expect("located");
    assert!(at.x.abs() > 8.0 && at.y.abs() > 4.0, "the gouge is in a corner: {at:?}");
}

#[test]
fn a_rounded_pockets_corners_picked_as_welded_faces_yield_the_strips_electrode() {
    // #1808: the floor and the wall picked as its welded (curved) face -
    // what a click on a rounded corner gives: the tangent flat walls and
    // corner strips weld into one face - make the electrode of the
    // planar-strip pick.
    let (world, mut h) = doc(rounded());
    let strips = faces_in(&world, (-10.0, -6.0), (10.0, 6.0));
    let f = model::features_of(&world, &model::find_by_name(&world, "part").expect("part")).expect("features");
    let welds = feature::patches(&f);
    assert_eq!(welds.len(), 1, "the tangent wall band welds into one face");
    let welded: Vec<u64> = welds.iter().flat_map(|p| p.faces.iter().copied()).collect();
    assert_eq!(welded.len(), 20, "four flat walls and sixteen corner strips");
    let mut picks: Vec<GeoSel> = strips.iter().filter(|g| !matches!(g.feature, FeatureSel::Face(id) if welded.contains(&id))).cloned().collect();
    assert_eq!(picks.len(), 1, "the floor");
    picks.extend(welds.iter().map(|p| GeoSel { entity: "part".into(), feature: FeatureSel::Patch(p.id) }));
    let of_strips = ElectrodeFromFaces::new(&world, "E-S", &strips, &spec(0.02)).expect("the strips' electrode");
    let e = electrode_and_burn(&world, &mut h, &picks, "E-W", "F1", SinkerPass::Finish);
    assert_eq!(&e, of_strips.electrode(), "the welded pick is the strips' electrode");
    assert!(e.body && e.cavity == (20.0, 12.0), "{e:?}");
    let sim = simulate_document(&world).expect("judged");
    assert!(sim.dev.is_clean(), "max rest {} at {:?}, max gouge {} at {:?}", sim.dev.max_rest, sim.dev.max_rest_at, sim.dev.max_gouge, sim.dev.max_gouge_at);
    // A welded face picked beside its own strips: the same walls twice.
    let mut both = picks.clone();
    both.extend(strips.iter().cloned());
    let again = ElectrodeFromFaces::new(&world, "E-B", &both, &spec(0.02)).expect("electrode");
    assert_eq!(again.electrode(), of_strips.electrode());
    // A stale welded face is refused by name.
    let stale = GeoSel { entity: "part".into(), feature: FeatureSel::Patch(0xDEAD_BEEF) };
    let Err(why) = ElectrodeFromFaces::new(&world, "E-X", &[stale], &spec(0.02)) else { panic!("a stale patch was admitted") };
    assert!(why.to_string().contains("no longer in the model"), "{why}");
}

#[test]
fn the_same_electrode_in_a_smaller_cavity_is_convicted_as_a_gouge() {
    // The electrode of the 20 x 12 pocket ...
    let (big, mut hb) = doc(rounded());
    let picks = faces_in(&big, (-10.0, -6.0), (10.0, 6.0));
    let e = electrode_and_burn(&big, &mut hb, &picks, "E", "F1", SinkerPass::Finish);
    let t = tool(&big, "E");
    // ... fed down an 18 x 10 pocket of the same corner radius.
    let (world, mut h) = doc(block_with_pocket(30.0, 30.0, &rounded_rect(9.0, 5.0, 2.0, 4), FLOOR, TOP));
    h.execute(&world, AddTool::new(&world, "E", t).expect("the electrode"));
    h.execute(&world, CreateOperation::new(&world, "Burn", "E", SrcRef::Stock, burn(&e, SinkerPass::Single, "F1")).expect("a sinker burn"));
    h.execute(&world, GenerateOperation::new(&world, "Burn").expect("generates"));
    let sim = simulate_document(&world).expect("judged");
    assert!(sim.dev.gouge_cells > 50 && sim.dev.max_gouge > 5.0, "the walls are burned away: {} cells, max gouge {}", sim.dev.gouge_cells, sim.dev.max_gouge);
    // Stated as a finish of the smaller cavity, the size law refuses it
    // by name before any burn is made.
    let mut small = burn(&e, SinkerPass::Finish, "F1");
    if let OpParams::SinkerBurn(op) = &mut small {
        (op.cavity_x, op.cavity_y) = (18.0, 10.0);
    }
    h.execute(&world, CreateOperation::new(&world, "Finish", "E", SrcRef::Stock, small).expect("a sinker burn"));
    let why = match GenerateOperation::new(&world, "Finish") {
        Ok(cmd) => {
            h.execute(&world, cmd);
            let op = model::find_by_name(&world, "Finish").expect("op");
            format!("{:?}", status_of(&world, &op))
        }
        Err(err) => err.to_string(),
    };
    assert!(why.contains("wider than the 18.000 cavity") || why.contains("past size"), "{why}");
}

#[test]
fn an_l_cavitys_electrode_keeps_its_notch_and_gouges_nothing() {
    // An L: 20 x 20 with its upper right 10 x 10 left standing.
    let cuts = [-30.0, -10.0, 0.0, 10.0, 30.0];
    let mesh = height_field(&cuts, &cuts, |i, j| if matches!((i, j), (1, 1) | (2, 1) | (1, 2)) { FLOOR } else { TOP });
    let (world, mut h) = doc(mesh);
    let picks = faces_in(&world, (-10.0, -10.0), (10.0, 10.0));
    assert_eq!(picks.len(), 7, "the L floor and six walls");
    let e = electrode_and_burn(&world, &mut h, &picks, "E-L", "R1", SinkerPass::Rough);
    assert!(e.body && e.outline.len() == 6 && e.cavity == (20.0, 20.0), "{e:?}");
    assert!((e.face.0 - 19.4).abs() < 1e-5 && (e.face.1 - 19.4).abs() < 1e-5, "{:?}", e.face);
    let sim = simulate_document(&world).expect("judged");
    assert_eq!(sim.dev.gouge_cells, 0, "the L's rough gouges nothing - not even the corner its bounds cover: {:?}", sim.dev.max_gouge_at);
    assert!(sim.dev.max_rest > 0.15, "the rough leaves stock: {}", sim.dev.max_rest);
    // Its finish burns the L to size. (A mitred inside corner stands
    // off 8 um further than the inset; no column centre lies in that.)
    electrode_and_burn(&world, &mut h, &picks, "E-LF", "F1", SinkerPass::Finish);
    let sim = simulate_document(&world).expect("judged");
    assert!(sim.dev.is_clean(), "max rest {} at {:?}, max gouge {} at {:?}", sim.dev.max_rest, sim.dev.max_rest_at, sim.dev.max_gouge, sim.dev.max_gouge_at);
    // The contrast: the box of the L's two sizes burns the corner the
    // part keeps.
    let boxed = Tool { outline: Vec::new(), ..tool(&world, "E-LF") };
    h.execute(&world, ChangeTool::new(&world, "E-LF", boxed).expect("a rectangular electrode"));
    h.execute(&world, GenerateOperation::new(&world, "Burn E-LF").expect("generates"));
    let sim = simulate_document(&world).expect("judged");
    let at = sim.dev.max_gouge_at.expect("a gouge");
    assert!(sim.dev.max_gouge > 5.0 && at.x > 0.0 && at.y > 0.0, "the box burns the notch: {} at {at:?}", sim.dev.max_gouge);
}

#[test]
fn a_cavity_turned_from_the_axes_is_judged_on_its_outlines_extents() {
    // A diamond: its corners are sharp and not square to the axes, so
    // the inward offset moves them further than the undersize and the
    // electrode's two sizes are under "cavity less twice the undersize".
    // The size law judges an outline's extents one way only (never
    // over), so the rough is the right electrode and is burned; the
    // rectangle of those sizes is held both ways and refused by name.
    let ring = [(10.0, 0.0), (0.0, 6.0), (-10.0, 0.0), (0.0, -6.0)];
    let (world, mut h) = doc(block_with_pocket(30.0, 30.0, &ring, FLOOR, TOP));
    let picks = faces_in(&world, (-10.0, -6.0), (10.0, 6.0));
    assert_eq!(picks.len(), 5, "the floor and four walls");
    let e = electrode_and_burn(&world, &mut h, &picks, "E-D", "R1", SinkerPass::Rough);
    assert!(e.body && e.cavity == (20.0, 12.0) && e.face.0 < 19.4 - 0.5 && e.face.1 < 11.4 - 0.05, "{e:?}");
    let status = |w: &World| status_of(w, &model::find_by_name(w, "Burn E-D").expect("op"));
    assert_eq!(status(&world), OpStatus::Ok, "the diamond's rough is generated");
    let sim = simulate_document(&world).expect("judged");
    assert_eq!(sim.dev.gouge_cells, 0, "{:?}", sim.dev.max_gouge_at);
    // The same two sizes as a rectangular face: the wrong electrode.
    let boxed = Tool { outline: Vec::new(), ..tool(&world, "E-D") };
    h.execute(&world, ChangeTool::new(&world, "E-D", boxed).expect("a rectangular electrode"));
    let why = match GenerateOperation::new(&world, "Burn E-D") {
        Ok(cmd) => {
            h.execute(&world, cmd);
            format!("{:?}", status(&world))
        }
        Err(err) => err.to_string(),
    };
    assert!(why.contains("undersizes its electrode"), "{why}");
}

#[test]
fn a_body_electrode_survives_the_project_file_and_a_recut_outline_restales_its_burn() {
    let (world, mut h) = doc(rounded());
    let picks = faces_in(&world, (-10.0, -6.0), (10.0, 6.0));
    electrode_and_burn(&world, &mut h, &picks, "E-F", "F1", SinkerPass::Finish);
    let t = tool(&world, "E-F");
    let status = |w: &World| status_of(w, &model::find_by_name(w, "Burn E-F").expect("op"));
    assert_eq!(status(&world), OpStatus::Ok);

    // The project file keeps the section, point for point.
    let path = std::env::temp_dir().join(format!("camcraft-1152-{}.ccam", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("the file");
    let back = new_world();
    project::load(&back, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    assert!(text.contains("\"outline\""), "the section is written");
    assert_eq!(tool(&back, "E-F"), t, "the record survives save and load");
    assert!(!tool(&back, "E-F").outline.is_empty());
    // A rectangular electrode writes no outline key: its file is as before.
    let json = project::tool_to_json(&camcraft_tools::electrode::electrode_tool(10.0, 6.0, 30.0, 15.0));
    assert!(json.get("outline").is_none(), "{json}");
    assert_eq!(project::tool_from_json(&project::tool_to_json(&t)).expect("round trip"), t);

    // A re-cut section of the same two sizes is a different burn: one
    // point moved inward, the bounds untouched.
    let mut recut = t.clone();
    let k = (0..recut.outline.len())
        .find(|&i| {
            let p = recut.outline[i];
            (p[0].abs() - 0.5 * t.diameter).abs() > 0.3 && (p[1].abs() - 0.5 * t.width).abs() > 0.3
        })
        .expect("a corner point off the bounds");
    recut.outline[k] = [recut.outline[k][0] * 0.98, recut.outline[k][1] * 0.98];
    assert_eq!((recut.diameter, recut.width), (t.diameter, t.width));
    h.execute(&world, ChangeTool::new(&world, "E-F", recut).expect("a re-cut electrode"));
    assert_eq!(status(&world), OpStatus::Stale, "a re-cut outline is a different burn");
    assert!(h.undo(&world));
    assert_eq!(status(&world), OpStatus::Ok, "undo restores the section the burn was made with");

    // The record's door holds at the commands: sizes that leave the
    // outline, and an outline that crosses itself, are refused by name.
    let wide = Tool { diameter: t.diameter + 1.0, ..t.clone() };
    let Err(why) = ChangeTool::new(&world, "E-F", wide.clone()) else { panic!("a size off its outline was admitted") };
    assert!(why.to_string().contains("the outline's X and Y extents"), "{why}");
    let Err(why) = AddTool::new(&world, "E-wide", wide) else { panic!("a size off its outline was admitted") };
    assert!(why.to_string().contains("the outline's X and Y extents"), "{why}");
}
