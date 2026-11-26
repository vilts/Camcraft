//! #2657: a Face on a model source lifting over a hole near the open
//! outer rim must have no Gouge finding and post cleanly, while a move
//! within a tool radius of a wall below the model top still convicts
//! and blocks the post.
//!
//! Provenance: our own fixtures over the Face generator and section law;
//! no vendor material consulted.
//!
//! Gate map (#2657): covers sim::verify::section (law (b), Mode::Boundary
//! open rims), app::operation::generate (Face), app::posting.

mod common;

use common::solid_part::{circle, plate, rect};

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::machining2d::FaceHoles;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

fn plate_with_rim_hole() -> Mesh {
    let outer = rect(-20.0, -30.0, 20.0, 30.0);
    let hole = circle((-12.0, 0.0), 3.0, 64);
    let mut facets = Vec::new();
    plate(&outer, Some((&hole, 0.0)), 0.0, 10.0, &mut facets);
    Mesh::from_facets(facets)
}

fn stepped_part_with_wall_below_top() -> Mesh {
    let mut facets: Vec<Triangle> = cube(Point3::new(-20.0, -30.0, 0.0), Point3::new(20.0, 30.0, 10.0)).facets.to_vec();
    facets.extend(cube(Point3::new(5.0, -10.0, 10.0), Point3::new(15.0, 10.0, 15.0)).facets.iter().cloned());
    Mesh::from_facets(facets)
}

fn setup_world(mesh: Mesh) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("plate", mesh));
    h.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box { dx: 40.0, dy: 60.0, dz: 12.0 },
            Location::default(),
        )
        .expect("box stock"),
    );
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-1000-4FL")
        .map(|(_, t)| t)
        .expect("EM-1000-4FL");
    h.execute(&world, AddTool::new(&world, "EM-1000-4FL", tool).expect("tool"));
    (world, h)
}

fn run_face(world: &World, h: &mut History, cut_z: f64) -> camcraft_app::schema::Toolpath {
    let params = OpParams::Face {
        holes: FaceHoles::Lift,
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 4.0,
        clearance_z: Height::fixed(30.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    h.execute(
        world,
        CreateOperation::new(world, "F1", "EM-1000-4FL", SrcRef::Entity("plate".into()), params).expect("create face"),
    );
    h.execute(world, GenerateOperation::new(world, "F1").expect("generate face"));
    let e = model::find_by_name(world, "F1").expect("op");
    model::toolpath_of(world, &e).expect("cached path")
}

#[test]
fn face_lifting_over_a_hole_near_the_open_outer_rim_has_no_gouge_and_posts() {
    let (world, mut h) = setup_world(plate_with_rim_hole());
    let tp = run_face(&world, &mut h, 10.0);
    let plunges = tp.path.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count();
    assert_eq!(plunges, 5, "Face lifts over the dia-6 through-hole: {plunges} plunges");
    assert!(
        !tp.warnings.iter().any(|w| matches!(w, Warning::Gouge { .. })),
        "open outer rim must not produce false gouges: {:?}",
        tp.warnings
    );
    let nc = std::env::temp_dir().join(format!("camcraft-face-rim-hole-clean-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&nc);
    posting::post_setup(&world, &nc).unwrap_or_else(|e| panic!("clean Face must post: {e}"));
    assert!(nc.exists());
    let _ = std::fs::remove_file(&nc);
}

#[test]
fn face_cutting_within_tool_radius_of_a_wall_below_model_top_still_gouges_and_blocks_post() {
    let (world, mut h) = setup_world(stepped_part_with_wall_below_top());
    // Cut at z 8: below the z 10 plate top, which itself sits below the z 15 boss top.
    let tp = run_face(&world, &mut h, 8.0);
    assert!(
        tp.warnings.iter().any(|w| matches!(w, Warning::Gouge { .. })),
        "cut near wall below model top must still report Gouge: {:?}",
        tp.warnings
    );
    let nc = std::env::temp_dir().join(format!("camcraft-face-rim-hole-wall-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&nc);
    let err = posting::post_setup(&world, &nc).expect_err("gouge must block post");
    assert!(err.contains("post blocked"), "{err}");
    assert!(!nc.exists());
}
