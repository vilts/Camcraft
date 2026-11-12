//! Turning cutting data (#891): a turning op on a stated material gets
//! a surface speed and a feed per rev from the turning table, read at
//! the stock's swing diameter; applying it is one undoable command; the
//! lathe posts write G96 S<Vc> under their clamp and a per-rev F equal
//! to the table's (pinned on a Fanuc-lathe golden). A G96 with no clamp
//! stated anywhere is refused by name, a tool's own max-rpm clamp rides
//! the program (G50 / D), threading keeps its constant rpm, and a tool
//! that states no turning data posts exactly as before (G97). Across
//! sections a lower tool clamp is lifted back to the machine's before a
//! G97, a higher one posts the machine's, and an inch Tormach program
//! states Vc in ft/min with D in rpm.
//!
//! Regenerate after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test lathe_post_css`
//!
//! Gate map (M4.4): covers app::cutting, app::cutting_turn,
//! app::commands (ApplyCuttingData), app::posting (the CSS PARAMs),
//! post::post (both lathe dialects), app::project (the additive key),
//! tools::turning_data.
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, ApplyCuttingData, ChangeTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine,
    SetStock, SetWorkMaterial,
};
use camcraft_app::cutting;
use camcraft_app::cutting_turn;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::tools::{Tool, ToolMaterial, TurningData, WorkMaterial, catalog};
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const FANUC: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");
const TORMACH: &str = include_str!("../../../assets/posts/tormach-lathe.cps");
/// The stage-A bar: wall r6 to z 2, neck r5 to the top, z -16..16.
const BAR: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];

fn insert() -> Tool {
    let t = catalog().into_iter().find(|(n, _)| *n == "TN-1600-0R8").map(|(_, t)| t);
    Tool { tool_material: Some(ToolMaterial::Carbide), ..t.expect("catalog insert") }
}

fn lathe(spindle_max: Option<f64>) -> Machine {
    Machine {
        model: "TestBench lathe".into(),
        vendor: "Camcraft Testbench".into(),
        kind: Some("lathe".into()),
        spindle_min: Some(25.0),
        spindle_max,
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    }
}

fn revolve(meridian: &[(f64, f64)], segs: usize) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / segs as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..segs {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(facets)
}

/// The bar in its own-size cylinder stock on the lathe, a carbide
/// insert on the rack (as `tool`), mild steel stated, a facing op F1.
fn faced(tool: Tool, spindle_max: Option<f64>) -> (World, History) {
    faced_on(tool, lathe(spindle_max))
}

/// [`faced`] on this machine (an inch one, say).
fn faced_on(tool: Tool, machine: Machine) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
    let stock = SetStock::new(&world, camcraft_app::stock::StockShape::Cylinder { diameter: 12.0, height: 32.0 }, at);
    h.execute(&world, stock.expect("bar stock"));
    h.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(machine)).expect("lathe"));
    h.execute(&world, AddTool::new(&world, "TN", tool).expect("insert"));
    h.execute(&world, ImportModel::from_mesh("bar", revolve(&BAR, 16)));
    h.execute(&world, SetWorkMaterial::new(&world, Some(WorkMaterial::MildSteel)).expect("material"));
    let params = OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let op = CreateOperation::new(&world, "F1", "TN", SrcRef::Entity("bar".into()), params);
    h.execute(&world, op.expect("create"));
    (world, h)
}

fn generate(world: &World, h: &mut History) {
    h.execute(world, GenerateOperation::new(world, "F1").expect("generate"));
}

/// A second facing section F2 after F1, on a tool TP that states no
/// turning data (so it posts G97); both ops generated.
fn then_constant_rpm(world: &World, h: &mut History) {
    h.execute(world, AddTool::new(world, "TP", insert()).expect("plain insert"));
    let params = OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let op = CreateOperation::new(world, "F2", "TP", SrcRef::Entity("bar".into()), params);
    h.execute(world, op.expect("create F2"));
    generate(world, h);
    h.execute(world, GenerateOperation::new(world, "F2").expect("generate F2"));
}

/// The byte offsets of each needle in turn, each searched after the
/// last; `None` when one is missing or out of order.
fn in_order(p: &str, needles: &[&str]) -> Option<Vec<usize>> {
    let mut at = 0;
    let mut out = Vec::new();
    for n in needles {
        let i = at + p.get(at..)?.find(n)?;
        out.push(i);
        at = i + n.len();
    }
    Some(out)
}

fn css(max_rpm: Option<f64>) -> Tool {
    Tool { turning: Some(TurningData { vc: 170.0, feed_per_rev: 0.2, max_rpm }), ..insert() }
}

fn tool(world: &World) -> Tool {
    model::find_by_name(world, "TN").and_then(|e| model::tool_of(world, &e)).expect("tool")
}

fn feeds(world: &World) -> (f64, f64) {
    let op = model::find_by_name(world, "F1").and_then(|e| model::operation_of(world, &e)).expect("op");
    let mut p = op.params.clone();
    cutting_turn::turn_feeds_mut(&mut p).map(|(f, pl)| (*f, *pl)).expect("turning feeds")
}

/// The document's stream through a lathe script, gate-free. The
/// returned warnings are the SCRIPT's own refusals: the stream's WARN
/// records (the fixture's verifier note, which `lathe_post.rs`'s
/// facing golden carries too and the gate judges elsewhere) are taken
/// out.
fn post(cps: &str, world: &World) -> engine::PostOutput {
    let records = posting::stream_records(world).expect("every op generated");
    let mut out = engine::run(cps, &posting::machine_value(world), &cnc::serialize(&records))
        .expect("the lathe script runs");
    let warns: Vec<String> = records
        .iter()
        .filter_map(|r| match r {
            cnc::Record::Warn { text } => Some(text.clone()),
            _ => None,
        })
        .collect();
    out.warnings.retain(|w| !warns.contains(w));
    out
}

#[test]
fn a_turning_op_on_mild_steel_gets_the_table_vc_and_feed_per_rev() {
    let (world, _h) = faced(insert(), Some(12000.0));
    let s = cutting::suggest(&world, "F1").expect("a turning suggestion");
    let plan = s.turning.expect("turning, not milling");
    // Carbide on mild steel: the window's middles, 170 m/min, 0.2 mm/rev.
    assert!((s.vc - 170.0).abs() < 1e-9 && (s.fz - 0.2).abs() < 1e-9, "{s:?}");
    // At the 12 mm bar: 170000 / (pi*12) = 4509.4 -> 4509 rpm.
    assert!((plan.diameter - 12.0).abs() < 1e-6, "the stock's swing: {plan:?}");
    assert_eq!(s.rpm, 4509.0);
    assert!((s.feed / s.rpm - 0.2).abs() < 1e-12, "feed = f x n: {}", s.feed);
    assert!((plan.plunge_feed / s.rpm - 0.1).abs() < 1e-12, "plunge at half the per-rev feed");
    assert!(s.notes.is_empty(), "{:?}", s.notes);
}

#[test]
fn applying_the_turning_suggestion_is_one_undo_and_posts_g96_and_the_table_feed() {
    let (world, mut h) = faced(insert(), Some(12000.0));
    let before = (tool(&world), feeds(&world));
    h.execute(&world, ApplyCuttingData::new(&world, "F1").expect("a suggestion"));
    let t = tool(&world);
    assert_eq!(t.rpm, 4509.0);
    assert_eq!(t.turning, Some(TurningData { vc: 170.0, feed_per_rev: 0.2, max_rpm: None }));
    assert!(ApplyCuttingData::new(&world, "F1").is_err(), "already applied is refused");
    generate(&world, &mut h);
    let out = post(FANUC, &world);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let p = &out.program;
    assert!(p.contains("G50 S12000\n"), "the machine's clamp: {p}");
    assert!(p.contains("G96 S170 M03\n"), "constant surface speed from the tool: {p}");
    assert!(!p.contains("G97 S"), "no constant rpm in a CSS-only program: {p}");
    // #1173: the CSS section cancels its G96 at its end - the only G97.
    assert_eq!(p.matches("G97").count(), 1, "{p}");
    assert!(p.contains("M05\nG97\nG28 U0 W0\nM30\n"), "{p}");
    assert!(p.contains("G99"), "feed per revolution: {p}");
    assert!(p.contains(" F0.2\n"), "the cut feeds at the table's 0.2 mm/rev: {p}");
    assert!(p.contains(" F0.1\n"), "the plunge at half of it: {p}");
    common::write_golden("nc/fanuc-lathe/facing_css.nc", p);

    // Undo the generation, then the suggestion: one step puts the tool
    // and the op back together.
    assert!(h.undo(&world) && h.undo(&world));
    assert_eq!((tool(&world), feeds(&world)), before);
}

#[test]
fn a_tool_max_rpm_holds_the_suggestion_and_clamps_both_dialects() {
    let capped = Tool { turning: Some(TurningData { vc: 100.0, feed_per_rev: 0.1, max_rpm: Some(3000.0) }), ..insert() };
    let (world, mut h) = faced(capped, Some(12000.0));
    let s = cutting::suggest(&world, "F1").expect("a suggestion");
    assert_eq!(s.rpm, 3000.0, "held to the tool's own maximum");
    assert!(s.notes.iter().any(|n| n.contains("tool's maximum 3000")), "{:?}", s.notes);
    h.execute(&world, ApplyCuttingData::new(&world, "F1").expect("apply"));
    assert_eq!(tool(&world).turning.and_then(|t| t.max_rpm), Some(3000.0), "the clamp is kept");
    generate(&world, &mut h);
    let f = post(FANUC, &world);
    assert!(f.warnings.is_empty(), "{:?}", f.warnings);
    assert!(f.program.contains("G50 S3000\nG96 S170 M03\n"), "the tool's clamp before its G96: {}", f.program);
    let t = post(TORMACH, &world);
    assert!(t.warnings.is_empty(), "{:?}", t.warnings);
    assert!(t.program.contains("G96 D3000 S170 M3\n"), "the lower clamp as D: {}", t.program);
}

#[test]
fn a_g96_with_no_clamp_anywhere_is_refused_by_name() {
    let css = Tool { turning: Some(TurningData { vc: 170.0, feed_per_rev: 0.2, max_rpm: None }), ..insert() };
    let (world, mut h) = faced(css, None);
    generate(&world, &mut h);
    for cps in [FANUC, TORMACH] {
        let out = post(cps, &world);
        assert!(out.warnings.iter().any(|w| w.contains("no rpm clamp")), "{:?}", out.warnings);
        assert!(!out.program.contains("G96"), "no unclamped G96 is written: {}", out.program);
    }
}

#[test]
fn a_tool_without_turning_data_posts_g97_and_threading_never_runs_css() {
    let (world, mut h) = faced(insert(), Some(12000.0));
    generate(&world, &mut h);
    let p = post(FANUC, &world).program;
    assert!(p.contains("G97 S3000 M03\n") && !p.contains("G96"), "unchanged before #891: {p}");
    let thread = OpParams::Threading {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(2.0),
        pitch: 1.0,
        depth: 0.6,
        infeed: 0.1,
        feed: 100.0,
        plunge_feed: 50.0,
        hand: Default::default(),
    };
    assert!(!cutting_turn::css_op(&thread), "a synchronized pass needs a constant rpm");
    let css = Tool { turning: Some(TurningData { vc: 170.0, feed_per_rev: 0.2, max_rpm: None }), ..insert() };
    assert!(cutting_turn::css_of(&thread, &css).is_none());
}

#[test]
fn turning_data_is_additive_in_the_project_file_and_sanity_checked() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, AddTool::new(&world, "TN", insert()).expect("insert"));
    let stated = Tool {
        rpm: 2000.0,
        turning: Some(TurningData { vc: 170.0, feed_per_rev: 0.2, max_rpm: Some(2500.0) }),
        ..insert()
    };
    h.execute(&world, AddTool::new(&world, "TC", stated.clone()).expect("stated"));
    let path = std::env::temp_dir().join(format!("camcraft-891-{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    // (Both tools' KIND is the word "turning"; the data key opens an object.)
    assert_eq!(text.matches("\"turning\": {").count(), 1, "only the stated tool writes the key: {text}");
    let back = new_world();
    project::open(&back, &text).expect("load");
    let t = |n: &str| model::find_by_name(&back, n).and_then(|e| model::tool_of(&back, &e)).expect("tool");
    assert_eq!(t("TN").turning, None, "an older tool states none");
    assert_eq!(t("TC").turning, stated.turning);
    project::save(&back, &path).expect("resave");
    let again = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(again, text, "byte-identical round trip");
    // An rpm over the tool's own clamp, or a zero Vc, is refused at the command.
    let over = Tool { rpm: 3000.0, ..stated.clone() };
    assert!(ChangeTool::new(&world, "TC", over).is_err(), "rpm over the tool's max is refused");
    let zero = Tool { turning: Some(TurningData { vc: 0.0, feed_per_rev: 0.2, max_rpm: None }), ..insert() };
    assert!(AddTool::new(&world, "TZ", zero).is_err(), "a zero surface speed is refused");
}

#[test]
fn a_lower_tool_clamp_is_lifted_back_to_the_machines_before_a_later_g97() {
    let (world, mut h) = faced(css(Some(3000.0)), Some(12000.0));
    then_constant_rpm(&world, &mut h);
    let f = post(FANUC, &world);
    assert!(f.warnings.is_empty(), "{:?}", f.warnings);
    let order = ["G50 S12000\n", "G50 S3000\nG96 S170 M03\n", "G50 S12000\nG97 S"];
    assert!(in_order(&f.program, &order).is_some(), "tool clamp, G96, machine clamp, G97: {}", f.program);
    assert_eq!(f.program.matches("G50").count(), 3, "{}", f.program);
    let t = post(TORMACH, &world);
    assert!(t.warnings.is_empty(), "{:?}", t.warnings);
    assert!(in_order(&t.program, &["G96 D3000 S170 M3\n", "G97 S"]).is_some(), "{}", t.program);
    assert_eq!(t.program.matches("G96").count(), 1, "the G97 section runs no CSS: {}", t.program);
}

#[test]
fn a_tool_clamp_over_the_machines_posts_the_machines() {
    let (world, mut h) = faced(css(Some(20000.0)), Some(12000.0));
    then_constant_rpm(&world, &mut h);
    let f = post(FANUC, &world);
    assert!(f.warnings.is_empty(), "{:?}", f.warnings);
    assert!(f.program.contains("G50 S12000\n") && !f.program.contains("S20000"), "{}", f.program);
    assert_eq!(f.program.matches("G50").count(), 1, "the machine's clamp already holds: {}", f.program);
    assert!(in_order(&f.program, &["G50 S12000\n", "G96 S170 M03\n", "G97 S"]).is_some(), "{}", f.program);
    let t = post(TORMACH, &world);
    assert!(t.warnings.is_empty(), "{:?}", t.warnings);
    assert!(in_order(&t.program, &["G96 D12000 S170 M3\n", "G97 S"]).is_some(), "{}", t.program);
}

/// With no machine maximum there is nothing to lift back to, so the
/// tool's G50 stays in force into a later G97 section - deliberately:
/// on Fanuc G50 S governs only G96 (see fanuc-lathe.cps, #891).
#[test]
fn with_no_machine_max_the_tools_g50_carries_into_a_later_g97() {
    let (world, mut h) = faced(css(Some(3000.0)), None);
    then_constant_rpm(&world, &mut h);
    let f = post(FANUC, &world);
    assert!(f.warnings.is_empty(), "{:?}", f.warnings);
    assert!(in_order(&f.program, &["G50 S3000\nG96 S170 M03\n", "G97 S"]).is_some(), "{}", f.program);
    assert_eq!(f.program.matches("G50").count(), 1, "no lift with no machine clamp to lift to: {}", f.program);
    let t = post(TORMACH, &world);
    assert!(t.warnings.is_empty(), "{:?}", t.warnings);
    assert!(in_order(&t.program, &["G96 D3000 S170 M3\n", "G97 S"]).is_some(), "{}", t.program);
}

#[test]
fn an_inch_tormach_posts_vc_in_ft_per_min_and_d_in_rpm() {
    let inch = Machine { units: Some("inch".into()), ..lathe(Some(12000.0)) };
    let (world, mut h) = faced_on(css(Some(3000.0)), inch);
    generate(&world, &mut h);
    let t = post(TORMACH, &world);
    assert!(t.warnings.is_empty(), "{:?}", t.warnings);
    assert!(t.program.contains("G20"), "an inch program: {}", t.program);
    // 170 m/min / 0.3048 = 557.7 ft/min -> S558; D stays the rpm clamp.
    assert!(t.program.contains("G96 D3000 S558 M3\n"), "Vc in ft/min, D in rpm: {}", t.program);
    assert!(!t.program.contains("S170"), "no metric Vc under G20: {}", t.program);
}
