//! The arc entry (M11.1) at the app seam: the committed CL golden of a
//! round bore entered on circular ramps, and the project wire's new
//! `"arc"` entry word round-tripping with no format bump.
//!
//! Gate map (M11.1): covers motion::machining2d (contour arc arm),
//! app::project (EntryDto::Arc), cl::cl.

mod common;

use common::{gen_to_cltext, write_golden};

use camcraft_app::cl::MoveKind;
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::machining2d::{contour, ContourParams, Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::OpParams;
use camcraft_app::project::{self, ProjectError};
use camcraft_app::world::new_world;

/// A round bore of radius 12 about (20, 10), as import tessellates it.
fn bore() -> Region {
    let pts = (0..128)
        .map(|k| {
            let t = std::f64::consts::TAU * f64::from(k) / 128.0;
            Point2::new(20.0 + 12.0 * t.cos(), 10.0 + 12.0 * t.sin())
        })
        .collect();
    Region::new(vec![Loop::new(pts).unwrap()]).unwrap()
}

fn bore_params(entry: Entry3D) -> ContourParams {
    ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Inside,
        lead: Lead::Arc { radius: 3.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

#[test]
fn contour_bore_arc_entry_matches_golden() {
    let t = contour(&bore(), &bore_params(Entry3D::Arc)).unwrap();
    assert!(t.is_chained() && t.all_finite());
    assert!(t.notes.is_empty(), "the bore holds every lead: {:?}", t.notes);
    assert!(
        !t.moves.iter().any(|m| m.kind == MoveKind::Plunge),
        "an arc entry posts no axial dive"
    );
    write_golden("contour_bore_arc_entry.cl", &gen_to_cltext(&t));
}

#[test]
fn the_arc_entry_changes_only_the_descents() {
    // Opt-in: the same bore under Plunge and under Arc walks the same
    // cut at every level - only the moves that reach each plane differ
    // (the Plunge's dive stands in the floor's room and feeds one link
    // per level over to the lead-in; the arc lands on the lead-in).
    let cut = |e: Entry3D| -> Vec<_> {
        let t = contour(&bore(), &bore_params(e)).unwrap();
        t.moves
            .into_iter()
            .filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z)
            .map(|m| (m.from, m.to, m.arc))
            .collect()
    };
    let (plunge, arc) = (cut(Entry3D::Plunge), cut(Entry3D::Arc));
    let shared: Vec<_> = plunge.iter().filter(|m| arc.contains(m)).cloned().collect();
    assert_eq!(shared, arc, "the arc's level walks are the plunge's, in order");
    // M11.3's one-entry law: the plunge now dives ON the lead-in's
    // start (it stands off the wall), so no dive link is left either.
    assert_eq!(plunge.len() - arc.len(), 0, "the same cut, entered once per level");
}

#[test]
fn the_arc_entry_word_round_trips_with_no_version_bump() {
    let plain = r#"{
        "version": 30,
        "entities": [
            {"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket": {
                 "stepover": 3.0, "stock_to_leave": 0.0, "clearance_z": 20.0,
                 "top_z": 8.0, "bottom_z": 2.0, "stepdown": 2.0,
                 "feed": 900.0, "plunge_feed": 250.0, "climb": true,
                 "strategy": "zigzag", "entry": "arc"}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, plain).expect("the arc word reads");
    let e = model::find_by_name(&world, "Op1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Pocket { entry, .. } = &o.params else {
        panic!("pocket params expected, got {:?}", o.params);
    };
    assert_eq!(*entry, Entry3D::Arc);

    let path = std::env::temp_dir().join(format!("camcraft_proj_arcentry_{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world2 = new_world();
    project::load(&world2, &path).expect("reload");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the arc shape round-trips byte-identically");
    assert!(first.contains("\"entry\": \"arc\""), "{first}");
    // The word is a step-up, not a format change: the file states the
    // head version the build always wrote.
    assert!(first.contains(&format!("\"version\": {}", project::PROJECT_VERSION)), "{first}");

    // A misspelt word is still corrupt text, refused.
    let bad = plain.replace("\"arc\"", "\"arcs\"");
    assert!(matches!(project::open(&new_world(), &bad), Err(ProjectError::Corrupt(_))));
}
