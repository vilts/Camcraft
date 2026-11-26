//! #2739: a project that a build with defect #2599 saved after an angle
//! edit holds a legacy tilt (`basis_version: 0`) with the new angles.
//! Characterisation: what such a file generates and posts, that the
//! preview (the world toolpath) and the post agree, and that the file
//! is the same file as a legal, never-edited basis-0 tilt - so the load
//! door has no safe test to tell the two apart (decision
//! `docs/decisions/2739-tilt-basis0-edited-file.md`).
//! Provenance: our own tests over our own commands, project file and
//! frame law (#2133 / PR #2157, #2599 / PR #2730).
//!
//! Gate map (M4.4): covers app::project (open, save_text), app::commands
//! (CreateOperation, GenerateOperation), app::posting, cl::frame,
//! post::post (tormach.cps).

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, LoadMachine, SelectPost, SetStock,
};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::Point3;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef, Tilt};
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::{model, posting, project};
use flecs_ecs::prelude::World;
use serde_json::Value;

const TOOL: &str = "EM 6";

fn slot(a: (f64, f64), b: (f64, f64), tilt: Option<Tilt>) -> OpParams {
    let above = |mm: f64| Height::of(HeightReference::TopOfStock, mm);
    OpParams::Slot {
        blend_turns: false,
        blend_radius: None,
        a: Point2::new(a.0, a.1),
        b: Point2::new(b.0, b.1),
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
        links: camcraft_app::links::Links::PLANAR,
        tilt,
        extend_start: 0.0,
        extend_end: 0.0,
        picks: Default::default(),
    }
}

fn tilt(basis_version: u8, a_deg: f64, b_deg: f64) -> Option<Tilt> {
    Some(Tilt { basis_version, a_deg, b_deg, origin: [0.0; 3] })
}

/// The slot that the user drew, in plane coordinates.
const DRAWN: ((f64, f64), (f64, f64)) = ((10.0, 0.0), (30.0, 0.0));

/// A 50 mm bar along machine X on the Tormach 770MX + microARC 4 (A only),
/// the machine of #2599.
fn bar_world() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::builtin(&world, "Tormach 770MX + microARC 4").expect("machine"));
    h.execute(
        &world,
        SelectPost::new(&world, PostRef::Named("Tormach PathPilot".into())).expect("post"),
    );
    let stock = SetStock::new(
        &world,
        StockShape::Box { dx: 60.0, dy: 50.0, dz: 50.0 },
        Location { pos: Point3::new(20.0, 0.0, -25.0), ..Location::default() },
    )
    .expect("stock");
    h.execute(&world, stock);
    let (_, mut t) = catalog().into_iter().next().expect("catalog tool");
    t.diameter = 6.0;
    t.shank_d = 6.0;
    t.flute_len = 20.0;
    t.rpm = 6000.0;
    t.feed = 400.0;
    h.execute(&world, AddTool::new(&world, TOOL, t).expect("tool"));
    (world, h)
}

fn create(world: &World, h: &mut History, name: &str, params: OpParams) {
    h.execute(world, CreateOperation::new(world, name, TOOL, SrcRef::Stock, params).expect(name));
}

fn generate_all(world: &World) {
    let mut h = History::default();
    for (name, _) in camcraft_app::setup::operation_order(world) {
        h.execute(world, GenerateOperation::new(world, &name).expect("generate"));
    }
}

/// Rewrite the `Legacy` tilt of a saved project: `basis` None removes the
/// key (the shape a pre-#2133 build wrote), Some states it.
fn with_legacy_tilt(text: &str, basis: Option<u8>, a_deg: f64, b_deg: f64) -> String {
    let mut file: Value = serde_json::from_str(text).expect("json");
    let entity = file["entities"]
        .as_array_mut()
        .expect("entities")
        .iter_mut()
        .find(|e| e["name"] == "Legacy")
        .expect("legacy operation");
    let tilt = entity["operation"]["params"]["slot"]["tilt"].as_object_mut().expect("tilt");
    match basis {
        None => {
            tilt.remove("basis_version");
        }
        Some(v) => {
            tilt.insert("basis_version".into(), Value::from(v));
        }
    }
    tilt.insert("a_deg".into(), Value::from(a_deg));
    tilt.insert("b_deg".into(), Value::from(b_deg));
    file.to_string()
}

fn opened(text: &str) -> World {
    let world = camcraft_app::world::new_world();
    project::open(&world, text).expect("open saved project");
    world
}

/// The feed end points of an operation's toolpath, in world mm: what the
/// preview draws.
fn feed_points(world: &World, name: &str) -> Vec<Point3> {
    let entity = model::find_by_name(world, name).expect(name);
    let path = model::toolpath_of(world, &entity).expect("generated toolpath").path;
    path.moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| m.to).collect()
}

fn same_points(a: &[Point3], b: &[Point3]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(p, q)| (p.x - q.x).abs() < 1e-6 && (p.y - q.y).abs() < 1e-6 && (p.z - q.z).abs() < 1e-6)
}

/// The modal machine (X, Y) at each G1 line of each operation section.
fn posted_xy(world: &World) -> Vec<(String, Vec<(String, String)>)> {
    let out = std::env::temp_dir().join(format!("camcraft_2739_{}.nc", std::process::id()));
    let posted = posting::post_setup(world, &out).expect("posts on microARC 4");
    let _ = std::fs::remove_file(&out);
    posted
        .program
        .split("(OPERATION: ")
        .skip(1)
        .map(|section| {
            let name = section.split(" - ").next().unwrap_or_default().to_owned();
            let (mut x, mut y) = (String::new(), String::new());
            let mut feeds = Vec::new();
            for line in section.lines() {
                for word in line.split_whitespace() {
                    if let Some(v) = word.strip_prefix('X') {
                        x = v.to_owned();
                    } else if let Some(v) = word.strip_prefix('Y') {
                        y = v.to_owned();
                    }
                }
                if line.starts_with("G1 ") {
                    feeds.push((x.clone(), y.clone()));
                }
            }
            (name, feeds)
        })
        .collect()
}

fn section<'a>(sections: &'a [(String, Vec<(String, String)>)], name: &str) -> &'a [(String, String)] {
    sections.iter().find(|(n, _)| n == name).map(|(_, f)| f.as_slice()).unwrap_or_default()
}

fn xy(x: &str, y: &str) -> (String, String) {
    (x.to_owned(), y.to_owned())
}

/// Step 1 of #2739: the file state is basis 0 with A0 B180. It generates
/// and posts the slot turned 90 degrees in the plane from the basis-1
/// result (`Current`). The preview and the post agree: a basis-1
/// operation (`Twin`) drawn at the turned plane coordinates has the same
/// world toolpath and posts the same G1 lines.
#[test]
fn a_basis0_a0_b180_file_posts_the_turned_plane_that_its_preview_shows() {
    let (world, mut h) = bar_world();
    create(&world, &mut h, "Legacy", slot(DRAWN.0, DRAWN.1, tilt(1, 90.0, 0.0)));
    create(&world, &mut h, "Current", slot(DRAWN.0, DRAWN.1, tilt(1, 0.0, 180.0)));
    create(&world, &mut h, "Twin", slot((0.0, -10.0), (0.0, -30.0), tilt(1, 0.0, 180.0)));
    // The file a #2599 build saved after the edit A90 -> A0 B180.
    let saved = project::save_text(&world).expect("save");
    let world = opened(&with_legacy_tilt(&saved, Some(0), 0.0, 180.0));
    let entity = model::find_by_name(&world, "Legacy").expect("legacy operation");
    let t = model::operation_of(&world, &entity).expect("definition").params.tilt().expect("tilt");
    assert_eq!((t.basis_version, t.a_deg, t.b_deg), (0, 0.0, 180.0), "the load keeps the saved basis");
    generate_all(&world);

    let legacy = feed_points(&world, "Legacy");
    let current = feed_points(&world, "Current");
    assert!(!legacy.is_empty(), "the legacy slot generates");
    assert!(
        legacy.iter().all(|p| p.x.abs() < 1e-6 && (10.0 - 1e-6..=30.0 + 1e-6).contains(&p.y)),
        "basis 0 puts the slot along world Y at X0: {legacy:?}"
    );
    assert!(
        current.iter().all(|p| p.y.abs() < 1e-6 && (10.0 - 1e-6..=30.0 + 1e-6).contains(&p.x)),
        "basis 1 puts the slot along world X at Y0: {current:?}"
    );
    assert!(
        same_points(&legacy, &feed_points(&world, "Twin")),
        "the legacy toolpath is the basis-1 toolpath of the turned slot"
    );

    let sections = posted_xy(&world);
    assert_eq!(
        section(&sections, "Current"),
        &[xy("10", "0"), xy("30", "0")][..],
        "basis 1 posts the drawn plane XY"
    );
    assert_eq!(
        section(&sections, "Legacy"),
        &[xy("0", "-10"), xy("0", "-30")][..],
        "basis 0 posts the plane XY turned 90 degrees"
    );
    assert_eq!(
        section(&sections, "Legacy"),
        section(&sections, "Twin"),
        "the post states the toolpath that the preview draws"
    );
}

/// Step 2 of #2739: no safe test exists at the load door. A legal
/// pre-#2133 project with a basis-0 A0 B180 tilt that nobody edited,
/// opened and saved by a current build, is the same file, byte for byte,
/// as a project that a #2599 build saved after the edit A90 -> A0 B180.
#[test]
fn an_edited_basis0_file_is_the_same_file_as_a_legal_unedited_one() {
    let (world, mut h) = bar_world();
    create(&world, &mut h, "Legacy", slot(DRAWN.0, DRAWN.1, tilt(1, 90.0, 0.0)));
    let saved = project::save_text(&world).expect("save");
    // Legal: a pre-#2133 file (no key) whose tilt was always A0 B180.
    let legal = opened(&with_legacy_tilt(&saved, None, 0.0, 180.0));
    // Defect: a pre-#2133 file at A90, edited to A0 B180 by a #2599 build,
    // which kept basis 0 and wrote the key.
    let edited = opened(&with_legacy_tilt(&saved, Some(0), 0.0, 180.0));
    generate_all(&legal);
    generate_all(&edited);
    let legal_text = project::save_text(&legal).expect("save legal");
    let edited_text = project::save_text(&edited).expect("save edited");
    let file: Value = serde_json::from_str(&legal_text).expect("json");
    let states_zero = file["entities"].as_array().is_some_and(|es| {
        es.iter().any(|e| e["name"] == "Legacy" && e["operation"]["params"]["slot"]["tilt"]["basis_version"] == 0)
    });
    assert!(states_zero, "the save states basis 0");
    assert_eq!(legal_text, edited_text, "no field tells the edited file from the legal one");
    assert_eq!(posted_xy(&legal), posted_xy(&edited), "and the two post the same program");
}
