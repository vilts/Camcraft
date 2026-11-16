//! The rigid-tapping capability (#786): `spindle.rigid_tapping` in the
//! `.mch` schema is a boolean or a refusal by name (the reader and the
//! Machines pane's validator agree), the pane's sheet says what it does,
//! every Tormach mill and router file states it, and a tap cycle on a
//! machine that says it cannot rigid-tap is refused by name at the post
//! gate - whatever post is chosen - and never posted as G84.
//!
//! Fixture law (clean-room): crafted files are our own values in the
//! `.mch` layout of `assets/machines/AGENTS.md`; the puck is our own mesh.
//! Provenance: our own tests, specced from the issue; capability facts
//! from Tormach's public machine manuals (behaviour only).
//!
//! Gate map (M4.4): covers app::machine (tap gate), app::machine::validate, app::machine::sheet, post::machine::parse, app::posting.

use camcraft_app::machine::library;
use camcraft_app::machine::validate::validate_text;
use camcraft_app::machine::{parse_mch, Kind};
use camcraft_app::posting;

const SPINDLE: &str = "/kinematics/default/parts/0/parts/1/spindle";

/// A good 3-axis mill; `patch` edits its JSON before it is written out.
fn mill(patch: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut v = serde_json::json!({
      "general": { "model": "TEST MILL", "vendor": "Testbench", "kind": "mill" },
      "kinematics": { "default": {
        "units": { "length": "mm", "angle": "degrees" },
        "parts": [
          { "id": "base", "name": "table", "type": "table", "parts": [
            { "id": "z", "name": "Z", "type": "linear", "travel": { "min": -200.0, "max": 200.0 }, "parts": [
              { "id": "y", "name": "Y", "type": "linear", "travel": { "min": -100.0, "max": 100.0 }, "parts": [
                { "id": "x", "name": "X", "type": "linear", "travel": { "min": -150.0, "max": 150.0 } }
              ] }
            ] },
            { "id": "head", "name": "head", "type": "head",
              "spindle": { "min_speed": 100.0, "max_speed": 8000.0 } }
          ] }
        ] } }
    });
    patch(&mut v);
    v.to_string()
}

fn with_tapping(x: serde_json::Value) -> String {
    mill(|v| {
        if let Some(sp) = v.pointer_mut(SPINDLE).and_then(|s| s.as_object_mut()) {
            sp.insert("rigid_tapping".to_owned(), x);
        }
    })
}

/// The schema: true, false and absent (or null) read; anything else is
/// refused by name at both doors - never read as "not limited".
#[test]
fn rigid_tapping_must_be_a_boolean_or_the_file_is_refused_by_name() {
    for (x, want) in [
        (serde_json::json!(true), Some(true)),
        (serde_json::json!(false), Some(false)),
        (serde_json::Value::Null, None),
    ] {
        let t = with_tapping(x.clone());
        assert!(validate_text(&t).is_empty(), "{x}: {:?}", validate_text(&t));
        assert_eq!(parse_mch(&t).map(|m| m.rigid_tapping), Ok(want), "{x}");
    }
    assert_eq!(parse_mch(&mill(|_| {})).map(|m| m.rigid_tapping), Ok(None), "absent is unstated");
    for x in [serde_json::json!("no"), serde_json::json!(0), serde_json::json!({ "g84": false })] {
        let t = with_tapping(x.clone());
        let r = validate_text(&t);
        assert_eq!(r.len(), 1, "{x}: one named refusal: {r:?}");
        assert_eq!(r[0].key, "spindle.rigid_tapping", "{r:?}");
        assert!(r[0].rule.contains("true or false"), "{r:?}");
        let err = parse_mch(&t).expect_err("the reader refuses it too");
        assert!(err.contains("spindle.rigid_tapping") && err.contains("true or false"), "{err}");
    }
}

/// The Machines pane's sheet says what the capability does.
#[test]
fn the_machines_sheet_states_rigid_tapping_in_words() {
    let list = library::builtins();
    let row = |name: &str| {
        let e = list.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("{name} listed"));
        let sp = e.groups.iter().find(|g| g.title == "Spindle").expect("a Spindle group");
        sp.rows
            .iter()
            .find(|(l, _)| l == "Rigid tapping")
            .map(|(_, v)| v.clone())
            .expect("a Rigid tapping row")
    };
    assert_eq!(row("Tormach 770MX"), "yes");
    assert_eq!(row("Tormach 770M"), "no - tap cycles are refused");
    assert_eq!(row("VMC 3-axis"), "not stated - not limited");
}

/// Every Tormach mill and router file states the capability (from the
/// manuals: the MX mills rigid-tap from the spindle encoder; the R8
/// M/M+ mills, the PCNC series and the routers do not). The lathes and
/// the plasma table are outside it (docs/decisions/786-rigid-tapping.md).
#[test]
fn every_tormach_mill_and_router_file_states_rigid_tapping() {
    let mut checked = 0;
    for (name, text) in posting::MACHINES.iter().filter(|(n, _)| n.starts_with("Tormach")) {
        let m = parse_mch(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        if matches!(camcraft_app::machine::kind(Some(&m)), Kind::Lathe | Kind::Plasma) {
            continue;
        }
        let want = name.contains("MX");
        assert_eq!(m.rigid_tapping, Some(want), "{name}");
        checked += 1;
    }
    assert!(checked >= 16, "the Tormach mills and routers: {checked}");
}

/// A Ø40 x 10 mm puck with a blind Ø6.2 hole 8 mm deep on its axis.
fn tapped_puck() -> camcraft_app::geometry::Mesh {
    use camcraft_app::geometry::{Mesh, Point3, Triangle};
    let meridian = [(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (3.1, 10.0), (3.1, 2.0), (0.0, 2.0)];
    let v = |j: usize, r: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
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

/// A tap job on `machine` through `post`, posted by the GUI's own path:
/// the gate's answer, the machine check's problems, and whether a file
/// was written.
fn tap_job(machine: &str, post: &str) -> (Result<posting::Posted, String>, Vec<String>, bool) {
    use camcraft_app::commands::{
        AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost, SetStock,
    };
    use camcraft_app::drilling::{CycleChoice, Hole};
    use camcraft_app::geo2d::Point2;
    use camcraft_app::operation::{Height, OpParams, SrcRef};
    use camcraft_app::schema::{Location, PostRef};
    use camcraft_app::stock::StockShape;
    use camcraft_app::tools::catalog;

    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::builtin(&world, machine).expect("built-in machine"));
    h.execute(&world, SelectPost::new(&world, PostRef::Named(post.into())).expect("post"));
    let stock = StockShape::Cylinder { diameter: 40.0, height: 10.0 };
    h.execute(&world, SetStock::new(&world, stock, Location::default()).expect("stock"));
    h.execute(&world, ImportModel::from_mesh("puck", tapped_puck()));
    let tap = catalog().into_iter().find(|(n, _)| *n == "TAP-M6").map(|(_, t)| t).expect("tap");
    h.execute(&world, AddTool::new(&world, "TAP-M6", tap).expect("add tap"));
    let drill = OpParams::Drill {
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(12.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 100.0,
        plunge_feed: 100.0,
        holes: vec![Hole {
            xy: Point2::new(0.0, 0.0),
            radius: 3.05,
            entry_z: 10.0,
            floor_z: 2.0,
            through: false,
            stepped: false,
        }],
        cross: Vec::new(),
    };
    let src = SrcRef::Entity("puck".into());
    h.execute(&world, CreateOperation::new(&world, "Tap1", "TAP-M6", src, drill).expect("tap op"));
    h.execute(&world, GenerateOperation::new(&world, "Tap1").expect("generate"));
    let m = camcraft_app::machine::of(&world).1.expect("a machine is mounted");
    let problems = camcraft_app::machine::check_document(&world, &m).map(|c| c.problems).unwrap_or_else(|e| vec![e]);
    let path = std::env::temp_dir().join(format!(
        "camcraft_rigid_tap_{}_{}_{}.nc",
        machine.replace(' ', "_"),
        post,
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let posted = posting::post_setup(&world, &path);
    let written = path.exists();
    let _ = std::fs::remove_file(&path);
    (posted, problems, written)
}

/// The pin (#786): a tap on a machine that cannot rigid-tap is refused
/// by name at the post gate through EVERY mill post - not only the
/// Tormach one that knows the capability - and no file is written. The
/// machine check names the op and the machine.
#[test]
fn a_tap_on_a_machine_that_cannot_rigid_tap_is_refused_by_name_whatever_the_post() {
    for post in ["Tormach PathPilot", "Fanuc", "Haas"] {
        let (posted, problems, written) = tap_job("Tormach 770M", post);
        let err = match posted {
            Ok(p) => panic!("{post}: the 770M must refuse a rigid tap:\n{}", p.program),
            Err(e) => e,
        };
        assert!(err.contains("rigid tapping") && err.contains("Tormach 770M") && err.contains("Tap1"), "{post}: {err}");
        assert!(!written, "{post}: a refused post writes nothing");
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].starts_with("Tap1: 1 tap cycle(s) need rigid tapping"), "{problems:?}");
    }
}

/// Unaffected: a machine that states it can (the 1100MX) and one that
/// states nothing (the generic VMC) still post their G84, the check silent.
#[test]
fn a_tap_posts_where_rigid_tapping_is_stated_or_unstated() {
    for (machine, post) in [("Tormach 1100MX", "Tormach PathPilot"), ("VMC 3-axis", "Fanuc")] {
        let (posted, problems, written) = tap_job(machine, post);
        let p = posted.unwrap_or_else(|e| panic!("{machine}: {e}"));
        assert!(written, "{machine}");
        assert!(problems.is_empty(), "{machine}: {problems:?}");
        assert!(p.program.lines().any(|l| l.starts_with("G84")), "{machine}:\n{}", p.program);
    }
}
