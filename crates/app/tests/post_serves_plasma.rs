//! #1266: a user's own post script on a plasma table. The mask cannot
//! tell a plasma script from a milling one, so the script states what it
//! serves (`serves = ["plasma"];`): a milling script (stated or not) is
//! refused by name on a torch table, an undeclared copy of the plasma
//! script is refused with the way to declare it, and the stated one posts
//! the shipped post's program byte for byte.
//! Provenance: our own fixtures over our own commands (plasma_post.rs's
//! plate); no vendor material.
//!
//! Gate map (#1266): covers app::posting (the posting_serves door),
//! post::dialect (declared_serves), post::js (words_global).
//! Lane shape (M4.4): seconds.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::operation::{Height, OpParams, PlasmaComp, PlasmaOp, SrcRef};
use camcraft_app::tools::torch_tool;
use camcraft_app::{posting, world::new_world};

fn plate_world() -> flecs_ecs::prelude::World {
    let ring = |h: f64, z: f64| [Point3::new(-h, -h, z), Point3::new(h, -h, z), Point3::new(h, h, z), Point3::new(-h, h, z)];
    let quad = |a, b, c, d| [Triangle::new(a, b, c), Triangle::new(a, c, d)];
    let mut f = Vec::new();
    for (z, up) in [(5.0, true), (0.0, false)] {
        let (o, i) = (ring(20.0, z), ring(5.0, z));
        for k in 0..4 {
            let (o0, o1, i0, i1) = (o[k], o[(k + 1) % 4], i[k], i[(k + 1) % 4]);
            f.extend(if up { quad(o0, o1, i1, i0) } else { quad(o0, i0, i1, o1) });
        }
    }
    for (r, out) in [(20.0, true), (5.0, false)] {
        let (lo, hi) = (ring(r, 0.0), ring(r, 5.0));
        for k in 0..4 {
            let (a, b, c, d) = (lo[k], lo[(k + 1) % 4], hi[(k + 1) % 4], hi[k]);
            f.extend(if out { quad(a, b, c, d) } else { quad(a, d, c, b) });
        }
    }
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::builtin(&world, "Tormach 1300PL").expect("mounts"));
    h.execute(&world, AddTool::new(&world, "PT-KERF-150", torch_tool(1.5)).expect("torch"));
    h.execute(&world, ImportModel::from_mesh("plate", Mesh::from_facets(f)));
    let op = PlasmaOp {
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        kerf: None,
        comp: PlasmaComp::Computer,
        reversed: false,
        lead_in: 4.0,
        lead_out: 3.0,
        small_hole_dia: 32.0,
        small_hole_feed: 0.65,
        overburn: 1.0,
        feed: None,
        pierce_height: None,
        cut_height: None,
        pierce_delay: None,
    };
    let src = SrcRef::Entity("plate".into());
    h.execute(&world, CreateOperation::new(&world, "Plasma1", "PT-KERF-150", src, OpParams::PlasmaProfile(op)).expect("op"));
    h.execute(&world, GenerateOperation::new(&world, "Plasma1").expect("generates"));
    world
}

fn shipped(name: &str) -> &'static str {
    posting::BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, t)| *t).expect("ships")
}

#[test]
fn a_users_plasma_script_posts_only_when_it_states_plasma() {
    let world = plate_world();
    let file = std::env::temp_dir().join(format!("camcraft_1266_plasma_{}.nc", std::process::id()));
    let plasma = shipped("Tormach PathPilot Plasma");

    // A user's mill script on the torch table: refused by name, stated or not.
    for own in [format!("{}\n// own copy\n", shipped("Haas")), format!("serves = [\"mill\"];\n{}", shipped("Haas"))] {
        let why = posting::post_script(&world, &file, &own).expect_err("a mill script on a plasma table");
        for w in ["Plasma1", "the given post script", "does not serve", "plasma cutting"] {
            assert!(why.contains(w), "{w:?} missing: {why}");
        }
    }
    // The plasma script copied without stating its kind: not proven, the
    // refusal says how to declare it.
    let undeclared = format!("{plasma}\n// the user's own copy\n");
    let why = posting::post_script(&world, &file, &undeclared).expect_err("undeclared on a plasma table");
    assert!(why.contains("serves = [\"plasma\"]"), "{why}");
    assert!(!file.exists(), "nothing written");

    // Stating it posts, and the program is the shipped post's own.
    let stated = format!("serves = [\"plasma\"];\n{plasma}");
    let own = posting::post_script(&world, &file, &stated).expect("a stated plasma script posts");
    let shipped_run = posting::post_script(&world, &file, plasma).expect("the shipped one posts");
    let _ = std::fs::remove_file(&file);
    assert_eq!(own.program, shipped_run.program);
}
