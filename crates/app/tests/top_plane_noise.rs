//! #241: a whole-model 2D Contour or Trace sections the part at its
//! Top of Part Z. An imported mesh's top corners need not sit at one
//! exact height - the issue's part scattered by 3e-13 mm - and a plane
//! at the very highest corner meets only that corner: the section came
//! back empty and the op was refused "no material" over a part that is
//! plainly there. The section now reads a micrometre lower, as it
//! already did for a grazing plane; a plane that really misses the part
//! still refuses, and names the Z span that would answer.
//!
//! Gate map (#241): covers app::operation::material (section),
//! app::operation::material_source (the section kinds' region).

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel};
use camcraft_app::geometry::{Mesh, Point3, cube};
use camcraft_app::links::Links;
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-0600-4FL";

fn contour(top_z: Height) -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: Height::fixed(15.0),
        top_z,
        bottom_z: Height::fixed(0.0),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Tangential { radius: 2.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Arc,
        helix_radius: 1.0,
        blend_turns: false,
        tilt: None,
        links: Links::PLANAR,
    }
}

/// A 20x20x5 box whose (20, 20) top corner stands 3e-13 mm proud - in
/// every facet that shares it, so the mesh stays closed.
fn noisy_top_box() -> Mesh {
    let m = cube(Point3::ZERO, Point3::new(20.0, 20.0, 5.0));
    let lift = |p: Point3| {
        if (p.x - 20.0).abs() < 1e-9 && (p.y - 20.0).abs() < 1e-9 && (p.z - 5.0).abs() < 1e-9 {
            Point3::new(p.x, p.y, 5.0 + 3e-13)
        } else {
            p
        }
    };
    let facets = m
        .facets
        .iter()
        .map(|t| camcraft_app::geometry::Triangle::new(lift(t.a), lift(t.b), lift(t.c)))
        .collect();
    Mesh::from_facets(facets)
}

fn world_with_box() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("box", noisy_top_box()));
    (world, history)
}

#[test]
fn a_whole_model_contour_at_a_noisy_top_sections_the_part() {
    let (world, mut history) = world_with_box();
    let top = Height::of(HeightReference::TopOfModel, 0.0);
    history.execute(&world, CreateOperation::new(&world, "C1", TOOL, SrcRef::Entity("box".into()), contour(top)).expect("create"));
    let made = GenerateOperation::new(&world, "C1");
    let cmd = made.unwrap_or_else(|e| panic!("the box's top section is the box, not 'no material': {e}"));
    history.execute(&world, cmd);
    let e = model::find_by_name(&world, "C1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cache");
    // The cutter rides outside the 20x20 outline, a radius (3) off it.
    let feeds: Vec<_> = tp.path.moves.iter().filter(|m| m.kind == camcraft_app::cl::MoveKind::Feed).collect();
    assert!(!feeds.is_empty());
    let off = |x: f64, y: f64| (x - x.clamp(0.0, 20.0)).hypot(y - y.clamp(0.0, 20.0));
    assert!(feeds.iter().all(|m| off(m.to.x, m.to.y) >= 3.0 - 0.02), "no cut inside a radius of the box");
    assert!(tp.warnings.is_empty(), "{:?}", tp.warnings);
}

#[test]
fn a_top_plane_above_the_part_still_refuses_and_names_the_span() {
    let (world, mut history) = world_with_box();
    history.execute(&world, CreateOperation::new(&world, "C2", TOOL, SrcRef::Entity("box".into()), contour(Height::fixed(9.0))).expect("create"));
    let err = GenerateOperation::new(&world, "C2").err().expect("a plane above the part sections nothing");
    let msg = err.to_string();
    assert!(msg.contains("no material at z = 9.000"), "{msg}");
    assert!(msg.contains("the source spans Z0.000 to Z5.000") && msg.contains("Top of Part Z"), "{msg}");
}
