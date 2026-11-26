//! #3100: the `rotariesBeforeTraverse` declaration is backed by the real
//! posted order. A post that declares it takes its 3+2 operations out of
//! the posed first-traverse law (the law then reads the operation's own
//! pose), so each shipped post that declares it must post, in each
//! section that turns a rotary, the safe-Z move and the rotary move
//! BEFORE the first X/Y traverse. A script change that breaks the order
//! fails here; a new shipped post that declares it fails the registry
//! check until it has its own order test.
//!
//! Provenance: our own tests over our own post scripts and commands; the
//! LinuxCNC G-code reference for G53 (machine coordinates). No vendor
//! material.
//!
//! Gate map (#3100): covers app::posting, post::dialect
//! (turns_rotaries_first), post::post (tormach.cps).

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, LoadMachine, SelectPost, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::Point3;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef, Tilt};
use camcraft_app::post::dialect::turns_rotaries_first;
use camcraft_app::posting;
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

const TOOL: &str = "EM 6";

fn slot(a_deg: f64) -> OpParams {
    let above = |mm: f64| Height::of(HeightReference::TopOfStock, mm);
    OpParams::Slot {
        a: Point2::new(10.0, 0.0),
        b: Point2::new(30.0, 0.0),
        width: 6.0,
        stock_to_leave: 0.0,
        clearance_z: above(10.0),
        retract_z: None,
        top_z: above(0.0),
        bottom_z: above(-0.5),
        stepdown: 0.5,
        stepover: 3.0,
        feed: 400.0,
        plunge_feed: 300.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: Some(Tilt { basis_version: 1, a_deg, b_deg: 0.0, origin: [0.0; 3] }),
        extend_start: 0.0,
        extend_end: 0.0,
        picks: Default::default(),
    }
}

#[test]
fn only_the_tormach_post_declares_that_it_turns_the_rotaries_first() {
    // Each shipped post that declares it needs an order test below.
    let declaring: Vec<&str> = posting::BUILTINS
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| posting::builtin_post_text(name).is_ok_and(|t| turns_rotaries_first(t).unwrap_or(false)))
        .collect();
    assert_eq!(declaring, ["Tormach PathPilot"]);
}

#[test]
fn the_tormach_post_turns_a_at_safe_z_before_each_first_xy_traverse() {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::builtin(&world, "Tormach 770MX + microARC 4").expect("machine"));
    h.execute(&world, SelectPost::new(&world, PostRef::Named("Tormach PathPilot".into())).expect("post"));
    let at = Location { pos: Point3::new(20.0, 0.0, -25.0), ..Location::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 50.0, dz: 50.0 }, at).expect("stock"));
    let (_, mut t) = catalog().into_iter().next().expect("catalog tool");
    t.diameter = 6.0;
    t.shank_d = 6.0;
    t.flute_len = 20.0;
    t.rpm = 6000.0;
    t.feed = 400.0;
    h.execute(&world, AddTool::new(&world, TOOL, t).expect("tool"));
    for (name, a) in [("A90", 90.0), ("A-90", -90.0), ("A30", 30.0)] {
        h.execute(&world, CreateOperation::new(&world, name, TOOL, SrcRef::Stock, slot(a)).expect(name));
        h.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
    }
    let out = std::env::temp_dir().join(format!("camcraft_3100_{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &out).expect("posts on the microARC 4");
    let _ = std::fs::remove_file(&out);
    let sections: Vec<&str> = posted.program.split("(OPERATION: ").skip(1).collect();
    assert_eq!(sections.len(), 3, "{}", posted.program);
    for section in sections {
        // The motion blocks only: a comment can name an axis ("G54 Y0 Z0").
        let lines: Vec<&str> = section.lines().filter(|l| !l.starts_with('(')).collect();
        let turn = lines.iter().position(|l| l.starts_with("G0 A")).unwrap_or_else(|| panic!("each section turns A:\n{section}"));
        let xy = lines.iter().position(|l| l.split_whitespace().any(|w| w.starts_with('X') || w.starts_with('Y'))).unwrap_or_else(|| panic!("no X/Y move:\n{section}"));
        assert!(turn < xy, "A turns before the first X/Y traverse:\n{section}");
        assert!(turn > 0 && lines[turn - 1].starts_with("G53 G0 Z"), "A turns right after the safe-Z move:\n{section}");
    }
}
