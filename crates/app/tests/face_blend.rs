//! M11.1 A through the app: a facing op with `blend_turns` generates
//! clean (no verify warning), posts its reversals as G2/G3 words, keeps
//! a distinct cache stamp, and round-trips through the project file -
//! where the key is written only when ON, so every legacy save stays
//! byte-identical.
//!
//! Gate map (M11.1): covers app::commands, motion::machining2d, app::operation, app::posting, app::project.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

fn face_params(blend: bool) -> OpParams {
    OpParams::Face { holes: Default::default(),
        tilt: None,
        blend_turns: blend,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// 60x60x20 box stock, a 4 mm end mill (stepover 3 overlaps the rows,
/// so every reversal is legal), one generated face op.
fn faced(blend: bool) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(
        &world,
        StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        Default::default(),
    );
    history.execute(&world, stock.expect("box stock"));
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .map(|(_, t)| t)
        .expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("add tool"));
    let op = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params(blend));
    history.execute(&world, op.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    (world, history)
}

fn tmp(tag: &str, ext: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft-face-blend-{tag}-{}.{ext}", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

#[test]
fn blended_face_generates_clean_and_posts_arc_words() {
    for blend in [false, true] {
        let (world, _h) = faced(blend);
        let e = model::find_by_name(&world, "Face1").expect("op");
        assert_eq!(operation::status_of(&world, &e), OpStatus::Ok);
        let cache = model::toolpath_of(&world, &e).expect("cached path");
        assert!(cache.warnings.is_empty(), "blend {blend}: {:?}", cache.warnings);
        assert_eq!(cache.path.has_arcs(), blend);
        assert!(cache.path.notes.is_empty(), "{:?}", cache.path.notes);
        let nc = tmp(if blend { "on" } else { "off" }, "nc");
        posting::post_setup(&world, &nc).expect("posts");
        let text = std::fs::read_to_string(&nc).expect("written");
        let _ = std::fs::remove_file(&nc);
        let words = |w: &str| text.lines().filter(|l| l.split_whitespace().any(|t| t == w)).count();
        if blend {
            // Rows alternate direction: both senses appear, one block per
            // reversal, and none is ever a straight step.
            let (g2, g3) = (words("G2") + words("G02"), words("G3") + words("G03"));
            assert!(g2 > 0 && g3 > 0, "{text}");
            assert_eq!(g2 + g3, cache.path.arc_count(), "one arc word per reversal");
        } else {
            assert_eq!(words("G2") + words("G3") + words("G02") + words("G03"), 0);
        }
    }
}

#[test]
fn the_stamp_tells_blended_from_sharp() {
    let (world, _h) = faced(false);
    let stamp = |blend: bool| {
        operation::current_stamp(&world, "EM-0400-2FL", &SrcRef::Stock, &face_params(blend), "Face1", None)
    };
    assert!(stamp(false).is_some());
    assert_ne!(stamp(false), stamp(true), "a sharp cache never answers for a blended op");
}

#[test]
fn the_key_is_written_only_when_on_and_round_trips() {
    for blend in [false, true] {
        let (world, _h) = faced(blend);
        let path = tmp(if blend { "on" } else { "off" }, "camproj");
        project::save(&world, &path).expect("save");
        let first = std::fs::read_to_string(&path).expect("read");
        assert_eq!(first.contains("blend_turns"), blend, "legacy saves carry no new key");
        let world2 = new_world();
        project::load(&world2, &path).expect("load");
        project::save(&world2, &path).expect("resave");
        let second = std::fs::read_to_string(&path).expect("reread");
        let _ = std::fs::remove_file(&path);
        assert_eq!(first, second, "round-trip is byte-identical");
        let e = model::find_by_name(&world2, "Face1").expect("loaded op");
        assert_eq!(operation::status_of(&world2, &e), OpStatus::Ok, "stamp survives");
        assert_eq!(
            model::operation_of(&world2, &e).map(|o| o.params),
            Some(face_params(blend))
        );
    }
}
