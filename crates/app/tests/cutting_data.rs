//! Cutting data and the spindle checks (M13.2): the power estimate on a
//! known removal rate against a hand calculation, the suggestion math
//! against a hand calculation, the machine verdicts (rpm / feed /
//! power) blocking the post by name and staying silent when unstated,
//! and the material + suggestion commands through undo and a save.
//!
//! Gate map (M4.4): covers app::commands, app::cutting, app::machine, post::machine, app::posting, app::project, tools::tools.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, ApplyCuttingData, CreateOperation, GenerateOperation, History, LoadMachine, SetStock,
    SetWorkMaterial,
};
use camcraft_app::cutting;
use camcraft_app::machine::{AxisLimits, Machine, check_document};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, ToolMaterial, WorkMaterial, catalog};
use camcraft_app::world::new_world;

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

fn slot_params() -> OpParams {
    OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a: camcraft_app::geo2d::Point2::new(0.0, 10.0),
        b: camcraft_app::geo2d::Point2::new(40.0, 10.0),
        width: 12.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 3.0,
        stepover: 4.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
    }
}

/// A generated slot: EM-0600-4FL (D 6, z 4, 9500 rpm), feed 800,
/// stepdown 3 - a full-width cut, ae = D.
fn slotted_doc() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, AddTool::new(&world, "EM-0600-4FL", catalog_tool("EM-0600-4FL")).expect("tool"));
    let stock = SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("stock"));
    let op = CreateOperation::new(&world, "Slot1", "EM-0600-4FL", SrcRef::Stock, slot_params());
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Slot1").expect("generate"));
    (world, h)
}

fn mount(world: &World, h: &mut History, m: Machine) {
    h.execute(world, LoadMachine::new(world, "t.mch".into(), Some(m)).expect("sane machine"));
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-cutting-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

/// The hand calculation for the slot on aluminium 6061 (kc1.1 700,
/// mc 0.25): fz = 800/(9500*4); hm = 2 fz/pi (full slot);
/// kc = 700 hm^-0.25; MRR = 6*3*800; Pc = MRR*kc/(60e6*0.8).
fn hand_power_kw() -> f64 {
    let fz = 800.0 / (9500.0 * 4.0);
    let hm = 2.0 * fz / std::f64::consts::PI;
    let kc = 700.0 * hm.powf(-0.25);
    6.0 * 3.0 * 800.0 * kc / (60.0e6 * 0.8)
}

#[test]
fn the_power_estimate_matches_the_hand_calculation() {
    let p = cutting::power_of(&slot_params(), &catalog_tool("EM-0600-4FL"), WorkMaterial::Aluminium6061)
        .expect("a stated slot claims an estimate");
    assert!((p.mrr - 14_400.0).abs() < 1e-9, "{p:?}");
    assert!((p.kw - hand_power_kw()).abs() < 1e-12, "{p:?} vs {}", hand_power_kw());
    // ~0.617 kW: the number the hand calc gives in full.
    assert!((p.kw - 0.6172).abs() < 1e-3, "{p:?}");
    // Unstated flutes claim nothing.
    let mute = Tool { flutes: None, ..catalog_tool("EM-0600-4FL") };
    assert!(cutting::power_of(&slot_params(), &mute, WorkMaterial::Aluminium6061).is_none());
}

/// A generated staged face that posts clean: EM-0400-2FL (D 4, z 2,
/// 12000 rpm), stepover 3, one 0.5 mm level from z 20 to 19.5, feed
/// 1200 - ae 3, ap 0.5, the posting suite's stock-only program shape.
fn faced_doc() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL")).expect("tool"));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("stock"));
    let face = OpParams::Face { holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: Some(Height::fixed(20.0)),
        stepdown: Some(0.5),
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    h.execute(&world, CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    (world, h)
}

/// Hand calc for the face on 6061: fz = 1200/(12000*2) = 0.05; ae/D =
/// 0.75, phi = acos(-0.5) = 2pi/3; hm = 0.05*1.5/phi = 0.03581;
/// kc = 700*hm^-0.25 = 1609; Pc = 3*0.5*1200*kc/48e6 = 0.0603 kW.
#[test]
fn power_blocks_the_post_by_name_and_is_silent_unstated() {
    let (world, mut h) = faced_doc();
    let path = tmp_nc("power");
    posting::post_setup(&world, &path).expect("the bare program posts");
    let _ = std::fs::remove_file(&path);
    let weak = Machine { model: "Weak".into(), spindle_power: Some(0.05), ..Default::default() };
    // Machine power stated, material not: no claim.
    assert!(check_document(&world, &weak).expect("checks").is_ok());
    h.execute(&world, SetWorkMaterial::new(&world, Some(WorkMaterial::Aluminium6061)).expect("material"));
    let c = check_document(&world, &weak).expect("checks");
    assert_eq!(c.problems.len(), 1, "{:?}", c.problems);
    assert!(
        c.problems[0].starts_with("Face1: spindle power estimated 0.06 kW (1.8 cm3/min of Aluminium 6061 at kc 1609 N/mm2)")
            && c.problems[0].contains("above the machine's 0.05 kW"),
        "{:?}",
        c.problems
    );
    mount(&world, &mut h, weak);
    let err = posting::post_setup(&world, &path).expect_err("the motor blocks");
    assert!(err.contains("Face1: spindle power estimated"), "{err}");
    assert!(!path.exists());
    // A machine that states no power: silent, the post goes.
    let mute = Machine { model: "Mute".into(), ..Default::default() };
    assert!(check_document(&world, &mute).expect("checks").is_ok());
    let strong = Machine { model: "Strong".into(), spindle_power: Some(0.1), ..Default::default() };
    mount(&world, &mut h, strong);
    posting::post_setup(&world, &path).expect("within the motor posts");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn rpm_and_feed_limits_block_the_post_by_name() {
    let (world, mut h) = faced_doc();
    let path = tmp_nc("rpmfeed");
    // 12000 rpm against an 8000 ceiling.
    mount(&world, &mut h, Machine { model: "Slow".into(), spindle_max: Some(8000.0), ..Default::default() });
    let err = posting::post_setup(&world, &path).expect_err("spindle blocks");
    assert!(err.contains("Face1: tool EM-0400-2FL runs 12000 rpm, above the machine's 8000"), "{err}");
    // Feed 1200 against a 500 X ceiling.
    let x = AxisLimits { name: "X".into(), feed: Some(500.0), ..Default::default() };
    mount(&world, &mut h, Machine { model: "Crawl".into(), axes: vec![x], ..Default::default() });
    let err = posting::post_setup(&world, &path).expect_err("feed blocks");
    assert!(err.contains("feeds 1200 mm/min, X tops out at 500"), "{err}");
    assert!(!path.exists());
    // Limits stated nowhere: silent.
    mount(&world, &mut h, Machine { model: "Mute".into(), ..Default::default() });
    posting::post_setup(&world, &path).expect("a mute machine judges nothing");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_suggestion_matches_the_hand_calculation() {
    let t = catalog_tool("EM-0600-4FL");
    assert_eq!((t.flutes, t.tool_material), (Some(4), Some(ToolMaterial::Carbide)));
    // Carbide on 6061: Vc mid (250+600)/2 = 425 m/min; fz mid at 10 mm
    // (0.05+0.10)/2 = 0.075, x0.5 for a 6 mm cutter = 0.0375.
    // n = 425000/(pi*6) = 22546.9 -> 22547; vf = 0.0375*4*22547 = 3382.
    let s = cutting::compute("EM-0600-4FL", &t, 4, ToolMaterial::Carbide, WorkMaterial::Aluminium6061, None, None);
    assert!((s.vc - 425.0).abs() < 1e-12 && (s.fz - 0.0375).abs() < 1e-12, "{s:?}");
    let n = (425_000.0 / (std::f64::consts::PI * 6.0)).round();
    assert_eq!(s.rpm, n);
    assert_eq!(s.rpm, 22_547.0);
    assert_eq!(s.feed, (0.0375 * 4.0 * n).round());
    // A 12000 rpm spindle holds n; the feed follows: 0.0375*4*12000.
    let m = Machine { spindle_max: Some(12_000.0), ..Default::default() };
    let s = cutting::compute("EM-0600-4FL", &t, 4, ToolMaterial::Carbide, WorkMaterial::Aluminium6061, None, Some(&m));
    assert_eq!((s.rpm, s.feed), (12_000.0, 1_800.0));
    assert_eq!(s.notes.len(), 1, "the clamp is named: {:?}", s.notes);
    // Optimal load 10%: hex/fz = 2 sqrt(0.1 - 0.01) = 0.6, so the
    // programmed fz is 0.0375/0.6 = 0.0625 -> 0.0625*4*12000 = 3000.
    let s = cutting::compute("EM-0600-4FL", &t, 4, ToolMaterial::Carbide, WorkMaterial::Aluminium6061, Some(0.1), Some(&m));
    assert!(s.thinning.is_some_and(|f| (f - 0.6).abs() < 1e-12), "{s:?}");
    assert_eq!(s.feed, 3_000.0);
}

#[test]
fn material_and_suggestion_are_commands_undo_and_save() {
    let (world, mut h) = slotted_doc();
    // No material: the suggestion refuses by name.
    let err = ApplyCuttingData::new(&world, "Slot1").err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err.contains("no workpiece material"), "{err}");
    h.execute(&world, SetWorkMaterial::new(&world, Some(WorkMaterial::MildSteel)).expect("material"));
    assert!(SetWorkMaterial::new(&world, Some(WorkMaterial::MildSteel)).is_err(), "no-op refused");
    let feed_of = |w: &World| {
        model::find_by_name(w, "Slot1").and_then(|e| model::operation_of(w, &e)).and_then(|o| cutting::feed_of(&o.params))
    };
    let rpm_of = |w: &World| {
        model::find_by_name(w, "EM-0600-4FL").and_then(|e| model::tool_of(w, &e)).map(|t| t.rpm)
    };
    let cmd = ApplyCuttingData::new(&world, "Slot1").expect("a suggestion");
    let (rpm, feed) = (cmd.suggestion().rpm, cmd.suggestion().feed);
    // Carbide on mild steel: Vc 185 -> n = 185000/(pi*6) = 9814.6 -> 9815;
    // fz 0.06*0.5 = 0.03 -> 0.03*4*9815 = 1178.
    assert_eq!((rpm, feed), (9_815.0, 1_178.0));
    h.execute(&world, cmd);
    assert_eq!((rpm_of(&world), feed_of(&world)), (Some(9_815.0), Some(1_178.0)));
    assert!(ApplyCuttingData::new(&world, "Slot1").is_err(), "already applied is refused");
    // Save while applied: material, flutes and edge material survive.
    let path = std::env::temp_dir().join(format!("camcraft-cutting-{}.json", std::process::id()));
    project::save(&world, &path).expect("save");
    // One undo puts BOTH numbers back.
    assert!(h.undo(&world));
    assert_eq!((rpm_of(&world), feed_of(&world)), (Some(9_500.0), Some(800.0)));
    assert!(h.undo(&world));
    assert_eq!(cutting::material_of(&world), None);
    let back = new_world();
    project::load(&back, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    assert_eq!(cutting::material_of(&back), Some(WorkMaterial::MildSteel));
    let t = model::find_by_name(&back, "EM-0600-4FL").and_then(|e| model::tool_of(&back, &e)).expect("tool");
    assert_eq!((t.flutes, t.tool_material, t.rpm), (Some(4), Some(ToolMaterial::Carbide), 9_815.0));
    assert_eq!(feed_of(&back), Some(1_178.0));
}

#[test]
fn a_document_without_material_writes_no_key_and_an_unknown_word_refuses() {
    let (world, _h) = slotted_doc();
    let path = std::env::temp_dir().join(format!("camcraft-cutting-nokey-{}.json", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(!text.contains("\"material\""), "unstated writes nothing");
    let bad = text.replacen("\"entities\"", "\"material\": \"unobtainium\", \"entities\"", 1);
    let err = project::open(&new_world(), &bad).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err.contains("unknown workpiece material"), "{err}");
}
