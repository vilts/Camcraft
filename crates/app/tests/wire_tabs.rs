//! #539 (part of #290 D6): wire EDM tabs and the tab cut-off. A die
//! with two tabs leaves them in its main cut and severs them in the
//! cut-off - its NC golden posts through the gate on the synthetic
//! fixture; every missing or impossible tab parameter is refused by
//! name; and the hold law: an unheld slug is refused, tabs or a stated
//! capture let it pass.
//! The fixture's conditions are SYNTHETIC (invented numbers, #290 D10);
//! not exercised on a real EDM control.
//!
//! Gate map (#539): covers motion::wire_edm::tabs, sim::verify::wire
//! (the hold law), app::operation::{wire_op, wire_verify} (tabs,
//! capture, the cut-off condition), app::posting_wire (the two-section
//! stream) and project::wire_edm_dto (the tab keys).
//! Provenance: our own tests over our own synthetic fixture.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef, WireContourOp, WireSide, WireTabs};
use camcraft_app::posting::{self, BUILTINS};
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

const FIXTURE: &str = include_str!("edm/wire-iso-fixture.mch");
const GOLDEN: &str = "tests/golden/nc/wire-iso/wire_contour_die_tabs.nc";

/// The fixture with a second SYNTHETIC condition of the same wire and
/// gap - the cut-off's - and one of another gap.
fn machine() -> String {
    let mut v: serde_json::Value = serde_json::from_str(FIXTURE).expect("fixture json");
    let conds = v["edm"]["conditions"].as_array_mut().expect("conditions");
    conds.push(serde_json::json!({ "id": "C120", "name": "synthetic cut-off", "gap": 0.04, "wire_diameter": 0.25, "role": "rough", "notes": "SYNTHETIC" }));
    conds.push(serde_json::json!({ "id": "C130", "name": "synthetic other gap", "gap": 0.02, "wire_diameter": 0.25, "role": "skim", "notes": "SYNTHETIC" }));
    v.to_string()
}

/// A 40 x 20 die opening with 2 mm corners (15-degree facets, under
/// the cover law's facet limit, so the golden stays short), z 0..12.
fn opening() -> Mesh {
    let (r, hx, hy) = (2.0, 18.0, 8.0);
    let mut pts = Vec::new();
    for (cx, cy, a0) in [(hx, hy, 0.0_f64), (-hx, hy, 90.0), (-hx, -hy, 180.0), (hx, -hy, 270.0)] {
        for s in 0..=6 {
            let t = (a0 + 15.0 * f64::from(s)).to_radians();
            pts.push((cx + r * t.cos(), cy + r * t.sin()));
        }
    }
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let mut f = Vec::new();
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        f.push(Triangle::new(Point3::new(0.0, 0.0, 12.0), p(a, 12.0), p(b, 12.0)));
        f.push(Triangle::new(Point3::new(0.0, 0.0, 0.0), p(b, 0.0), p(a, 0.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 0.0), p(b, 12.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 12.0), p(a, 12.0)));
    }
    Mesh::from_facets(f)
}

fn two_tabs() -> WireTabs {
    WireTabs { count: Some(2), width: Some(1.0), at: vec![[0.0, 10.0], [0.0, -10.0]], cutoff_condition: "C120".into() }
}

fn die(tabs: Option<WireTabs>, capture: bool) -> WireContourOp {
    WireContourOp {
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side: WireSide::Die,
        start: Some([0.0, 0.0]),
        tabs,
        capture,
        taper_deg: 0.0,
        four_axis: None,
        burnout: None,
        clearance: camcraft_app::operation::DEFAULT_CLEARANCE,
        skims: Vec::new(),
    }
}

/// The fixture mounted, the opening in a 60 x 60 x 12 plate, and the
/// die cut `op` created as "Die1" (not generated).
fn doc(op: WireContourOp) -> World {
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(&machine()).expect("the fixture parses");
    h.execute(&world, LoadMachine::new(&world, "wire-iso.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, ImportModel::from_mesh("opening", opening()));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 12.0 }, Location::default()).expect("stock");
    h.execute(&world, stock);
    let c = CreateOperation::new(&world, "Die1", "", SrcRef::Entity("opening".into()), OpParams::WireContour(op)).expect("op");
    h.execute(&world, c);
    world
}

fn generated(op: WireContourOp) -> World {
    let world = doc(op);
    let mut h = History::default();
    h.execute(&world, GenerateOperation::new(&world, "Die1").expect("generates"));
    world
}

fn refusal(op: WireContourOp) -> String {
    let world = doc(op);
    GenerateOperation::new(&world, "Die1").err().expect("refused").to_string()
}

fn cps() -> &'static str {
    BUILTINS.iter().find(|(n, _)| *n == "Generic ISO Wire EDM").map(|(_, s)| *s).expect("registered")
}

/// Done means: a die with two tabs leaves the slug held until the
/// cut-off pass - the main cut frees nothing, the cut-off frees the
/// slug, the program stops between them - and posts its golden.
#[test]
fn a_die_with_two_tabs_holds_its_slug_until_the_cut_off() {
    let world = generated(die(Some(two_tabs()), false));
    let e = model::find_by_name(&world, "Die1").expect("op");
    let path = model::toolpath_of(&world, &e).expect("generated").path;
    let split = camcraft_app::wire_edm::cutoff_split(&path, Point2::new(0.0, 0.0), 2).expect("a tabbed path");
    // The main cut never crosses a tab's gap: no main move reaches X0 on the top or bottom side.
    for m in &path.moves[..split] {
        let crosses = (m.from.x < -0.5 && m.to.x > 0.5) || (m.to.x < -0.5 && m.from.x > 0.5);
        assert!(!(crosses && m.from.y.abs() > 9.0 && m.to.y.abs() > 9.0), "the main cut crosses a tab: {m:?}");
    }
    // The cut-off crosses each tab exactly once each way.
    // Across a gap: from one end (X -/+0.665, the width plus the kerf) to the other.
    let gap_end = |x: f64| (x.abs() - 0.665).abs() < 1e-6;
    let across = |m: &&camcraft_app::cl::Move| gap_end(m.from.x) && gap_end(m.to.x) && m.from.x * m.to.x < 0.0 && m.from.y.abs() > 9.0;
    assert_eq!(path.moves[..split].iter().filter(across).count(), 0, "the main cut leaves the tabs");
    let across = path.moves[split..].iter().filter(across).count();
    assert_eq!(across, 4, "two tabs, cut and retraced");
    assert!(path.notes.iter().any(|n| n.contains("2 tab(s) of 1.000 mm")), "{:?}", path.notes);
    let c = posting::wire_clause(&world);
    assert!(c.problems.is_empty(), "{:?}", c.problems);
    let line = c.line.expect("judged");
    assert!(line.contains("Die1 holds its slug by tabs through the main cut; the tab cut-off frees"), "{line}");
    let out = std::env::temp_dir().join(format!("camcraft-wire-tabs-{}.nc", std::process::id()));
    let posted = posting::post_script(&world, &out, cps()).expect("the tabbed program posts");
    let _ = std::fs::remove_file(&out);
    let text = posted.program;
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(GOLDEN, &text).expect("write golden");
    }
    assert_eq!(text, std::fs::read_to_string(GOLDEN).expect("golden present"), "the tabbed program drifted from its golden");
    // Said out loud: main cut, stop, then the cut-off with its own condition.
    let main = text.find("FX-COND C110").expect("main condition");
    let stop = text[main..].find("M00").map(|i| i + main).expect("the stop before the cut-off");
    let off = text.find("FX-COND C120").expect("cut-off condition");
    assert!(main < stop && stop < off, "{text}");
    assert!(text.to_uppercase().contains("TAB CUT-OFF"), "{text}");
}

/// Done means: the same cut with tabs off and no capture is refused by
/// name; a stated capture lets it pass.
#[test]
fn an_unheld_slug_is_refused_and_a_capture_passes_it() {
    let world = generated(die(None, false));
    let problems = posting::verification_gate(&world);
    assert!(
        problems.iter().any(|p| p.starts_with("Die1: wire slug: the slug freed") && p.contains("drops unsupported")),
        "{problems:?}"
    );
    let world = generated(die(None, true));
    let c = posting::wire_clause(&world);
    assert!(c.problems.is_empty(), "{:?}", c.problems);
    assert!(c.line.is_some_and(|l| l.contains("into the stated capture")));
}

/// Every missing or impossible tab parameter is refused by name.
#[test]
fn missing_or_impossible_tab_parameters_are_refused_by_name() {
    let t = two_tabs;
    for (tabs, want) in [
        (WireTabs { count: None, ..t() }, "the tab count is not stated"),
        (WireTabs { count: Some(0), ..t() }, "the tab count must be 1 to 64"),
        (WireTabs { width: None, ..t() }, "the tab width is not stated"),
        (WireTabs { width: Some(-1.0), ..t() }, "the tab width must be a positive number"),
        (WireTabs { at: Vec::new(), ..t() }, "no tab position is stated"),
        (WireTabs { count: Some(3), ..t() }, "the tab count is 3 but 2 tab position(s) are stated"),
        (WireTabs { cutoff_condition: String::new(), ..t() }, "no tab cut-off condition is named"),
        (WireTabs { cutoff_condition: "C130".into(), ..t() }, "the tab cut-off's condition C130 cuts at a kerf offset"),
        (WireTabs { cutoff_condition: "C999".into(), ..t() }, "the tab cut-off's condition"),
        (WireTabs { width: Some(36.0), ..t() }, "does not fit on one straight side"),
        (WireTabs { at: vec![[0.0, 10.0], [0.5, 10.0]], ..t() }, "tabs 1 and 2 overlap"),
        (WireTabs { count: Some(1), at: vec![[19.4, 9.4]], ..t() }, "the side there is only"),
        (WireTabs { count: Some(1), at: vec![[0.0, 3.0]], ..t() }, "stands 6.835 mm from the wire path"),
    ] {
        let err = refusal(die(Some(tabs), false));
        assert!(err.contains(want), "want {want:?}, got: {err}");
    }
}

/// #539 x #536: the tapered builder reads the top-plane path as one
/// closed ring and a tabbed path is not one, so tabs with a taper are
/// refused by name - at creation, at a params edit and at generation (a
/// file that states both) - never refused only by accident of the
/// geometry. Tabs with taper 0 still generate.
#[test]
fn tabs_with_a_taper_are_refused_by_name_at_every_door() {
    let why = "tabs can't be combined with a taper yet (a 2.000 degree taper is asked) - remove the tabs or set the taper to 0";
    let tapered = WireContourOp { taper_deg: 2.0, ..die(Some(two_tabs()), false) };
    // Creation.
    let world = new_world();
    let m = parse_mch(&machine()).expect("the fixture parses");
    let mut h = History::default();
    h.execute(&world, LoadMachine::new(&world, "wire-iso.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, ImportModel::from_mesh("opening", opening()));
    let err = CreateOperation::new(&world, "Die1", "", SrcRef::Entity("opening".into()), OpParams::WireContour(tapered.clone()))
        .err()
        .expect("refused at creation")
        .to_string();
    assert!(err.contains(why), "{err}");
    // A params edit onto an untapered tabbed die.
    let world = generated(die(Some(two_tabs()), false));
    let err = EditOperationParams::new(&world, "Die1", OpParams::WireContour(tapered)).err().expect("refused at the edit").to_string();
    assert!(err.contains(why), "{err}");
    // Generation: a file that states both reaches no command door.
    let file = std::env::temp_dir().join(format!("camcraft-539-taper-{}.ccam", std::process::id()));
    project::save(&world, &file).expect("save");
    let text = std::fs::read_to_string(&file).expect("read back");
    let mut v: serde_json::Value = serde_json::from_str(&text).expect("project json");
    fn lean(v: &mut serde_json::Value) -> bool {
        match v {
            serde_json::Value::Object(m) => match m.get_mut("wire_contour") {
                Some(w) => {
                    w["taper_deg"] = serde_json::json!(2.0);
                    true
                }
                None => m.values_mut().any(lean),
            },
            serde_json::Value::Array(a) => a.iter_mut().any(lean),
            _ => false,
        }
    }
    assert!(lean(&mut v), "the file states the wire contour");
    std::fs::write(&file, v.to_string()).expect("write");
    let back = new_world();
    let loaded = project::load(&back, &file);
    let _ = std::fs::remove_file(&file);
    loaded.expect("load");
    let err = GenerateOperation::new(&back, "Die1").err().expect("refused at generation").to_string();
    assert!(err.contains("Die1") && err.contains(why), "{err}");
}
