//! #1530: a flipped setup's fixtures are judged against the carried
//! stock by their real shape, not their bounding box. A 50 x 50 x 10
//! part in 56 x 56 stock; Setup1 contours its sides down to Z 4, leaving
//! a 3 mm brim (the hat); Setup2 flips it over X, so the machined
//! 50 x 50 section stands Z 0..6 and the brim Z 6..10 above it. A
//! built-in vise gripping the section's sides, and soft jaws whose nest
//! pocket clears it, are held; a nest too small, or a floor standing a
//! hair past the contact tolerance into the part, is refused by name
//! with its depth.
//! Provenance: our own fixtures over plain re-clamp practice (a part
//! flipped into a vise or a nest); no vendor material consulted.
//!
//! The #1530 review: a post or a lip narrower than the grid pitch,
//! standing between the column lines, is refused; an open mesh and a
//! coarse-fidelity fixture keep the box verdict. The second review: a
//! setup turned about Z (not flipped) is judged in its turned frame.
//!
//! Gate map (#1530): covers app::fixture_carried (carried_fixture_problems,
//! mesh_penetration, CARRIED_CONTACT_TOL), sim::fixture_mesh::column
//! (steel_on_line), app::commands (InsertVise, AlignPartToSetup).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, InsertVise,
    SetStock, TagFixture, VisePlacement,
};
use camcraft_app::fixture_carried::{CARRIED_CONTACT_TOL, FIXTURE_IN_CARRIED, carried_fixture_problems};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::setup_align::AlignPreset;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::incoming_body;
use camcraft_app::tools::catalog;
use camcraft_app::vise;
use camcraft_app::world::new_world;
use camcraft_app::{model, model_fidelity};

const TOOL: &str = "EM-0600-4FL";

fn contour() -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(20.0),
        retract_z: Height::fixed(15.0),
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(4.0),
        stepdown: 2.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

fn face(cut_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 4.0,
        clearance_z: Height::fixed(30.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// The hat, flipped: Setup2 is entered with the 50 x 50 section at
/// Z 0..6 and the 56 x 56 brim at Z 6..10, and faces the brim side.
fn flipped_hat() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-25.0, -25.0, 0.0), Point3::new(25.0, 25.0, 10.0))));
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 56.0, dy: 56.0, dz: 10.0 }, at).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    let op = CreateOperation::new(&world, "Sides", TOOL, SrcRef::Entity("part".into()), contour()).expect("create");
    h.execute(&world, op);
    h.execute(&world, GenerateOperation::new(&world, "Sides").expect("generate"));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", AlignPreset::FlipX).expect("flip"));
    h.execute(&world, CreateOperation::new(&world, "Brim", TOOL, SrcRef::Stock, face(10.0)).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Brim").expect("generate"));
    (world, h)
}

fn mesh_of(boxes: &[(Point3, Point3)]) -> Mesh {
    let tris: Vec<Triangle> = boxes.iter().flat_map(|(a, b)| cube(*a, *b).facets.iter().copied().collect::<Vec<_>>()).collect();
    Mesh { facets: tris.into() }
}

/// One-piece soft jaws (80 x 80, Z -10..5) with a square nest pocket
/// `half` from the centre each way and its floor at `floor`, plus any
/// `extra` blocks.
fn soft_jaws_mesh(half: f64, floor: f64, extra: &[(Point3, Point3)]) -> Mesh {
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let mut boxes = vec![
        (p(-40.0, -40.0, -10.0), p(40.0, 40.0, floor)),
        (p(-40.0, -40.0, floor), p(-half, 40.0, 5.0)),
        (p(half, -40.0, floor), p(40.0, 40.0, 5.0)),
        (p(-half, -40.0, floor), p(half, -half, 5.0)),
        (p(-half, half, floor), p(half, 40.0, 5.0)),
    ];
    boxes.extend_from_slice(extra);
    mesh_of(&boxes)
}

fn add_jaws(world: &World, h: &mut History, m: Mesh) {
    h.execute(world, ImportModel::from_mesh("Soft jaws", m));
    h.execute(world, TagFixture::new(world, "Soft jaws", Some("Setup2")).expect("tag jaws"));
}

fn soft_jaws(world: &World, h: &mut History, half: f64, floor: f64) {
    add_jaws(world, h, soft_jaws_mesh(half, floor, &[]));
}

/// The middle between four column centres of Setup2's carried grid,
/// near `(x, y)`: as far from every column line as the grid allows.
fn between_columns(world: &World, x: f64, y: f64) -> (f64, f64) {
    let body = incoming_body(world, "Setup2").expect("carried");
    let d = &body.dex;
    let ix = ((x - d.x0) / d.cell).floor().clamp(0.0, (d.nx - 2) as f64) as usize;
    let iy = ((y - d.y0) / d.cell).floor().clamp(0.0, (d.ny - 2) as f64) as usize;
    let c = d.center(ix, iy);
    (c.x + d.cell * 0.5, c.y + d.cell * 0.5)
}

fn refusals(world: &World) -> Vec<String> {
    let p = carried_fixture_problems(world);
    let gate: Vec<String> = posting::verification_gate(world).into_iter().filter(|l| l.contains(FIXTURE_IN_CARRIED)).collect();
    assert_eq!(gate, p, "the post gate reads the same clause");
    p
}

#[test]
fn the_flipped_hat_enters_setup2_section_down_brim_up() {
    let (world, _h) = flipped_hat();
    let body = incoming_body(&world, "Setup2").expect("carried");
    let (lo, hi) = body.bounds().expect("material");
    assert!(lo.z.abs() < 1e-9 && (hi.z - 10.0).abs() < 1e-9, "{lo:?} {hi:?}");
    assert!(body.dex.cell < 0.5, "the pins below need a pitch under 0.5 mm: {}", body.dex.cell);
}

#[test]
fn a_builtin_vise_gripping_the_flipped_parts_sides_is_held() {
    let (world, mut h) = flipped_hat();
    // Jaws 5 mm high, faces on the section's sides (Y +-25), the floor
    // under it at Z 0, the brim 1 mm above the jaw tops.
    let mut p = vise::size("5 in").expect("5 in").params(50.0);
    p.jaw_height = 5.0;
    let at = VisePlacement::At(Location { pos: Point3::new(0.0, 25.0, 0.0), ..Default::default() });
    h.execute(&world, InsertVise::new(&world, &p, Some("Setup2"), at).expect("vise"));
    let r = refusals(&world);
    assert!(r.is_empty(), "the vise only touches the part: {r:?}");
}

#[test]
fn a_vise_jaw_moved_into_the_part_is_refused_by_name_with_its_depth() {
    let (world, mut h) = flipped_hat();
    let mut p = vise::size("5 in").expect("5 in").params(48.0);
    p.jaw_height = 5.0;
    let at = VisePlacement::At(Location { pos: Point3::new(0.0, 24.0, 0.0), ..Default::default() });
    let cmd = InsertVise::new(&world, &p, Some("Setup2"), at).expect("vise");
    let name = cmd.name().to_owned();
    h.execute(&world, cmd);
    let r = refusals(&world);
    assert!(r.len() == 1 && r[0].contains(&format!("fixture '{name}'")) && r[0].contains("5.00 mm deep"), "{r:?}");
}

#[test]
fn soft_jaws_whose_nest_clears_the_part_are_held() {
    let (world, mut h) = flipped_hat();
    soft_jaws(&world, &mut h, 25.05, 0.0);
    let r = refusals(&world);
    assert!(r.is_empty(), "the part sits in the nest: {r:?}");
}

#[test]
fn soft_jaws_with_a_nest_too_small_are_refused_by_name_with_the_depth() {
    // The pocket 0.5 mm too small on each side: its walls (Z 0..5) stand
    // in the section (Z 0..6).
    let (world, mut h) = flipped_hat();
    soft_jaws(&world, &mut h, 24.5, 0.0);
    let r = refusals(&world);
    assert!(
        r.len() == 1
            && r[0].contains("setup 'Setup2': fixture 'Soft jaws'")
            && r[0].contains("setup 'Setup1'")
            && r[0].contains("5.00 mm deep"),
        "{r:?}"
    );
}

#[test]
fn a_nest_floor_a_hair_past_the_contact_tolerance_is_refused() {
    // 0.02 mm beyond the 0.01 mm tolerance into the part's face (a
    // literal, so a loosened tolerance turns this red).
    let into = 0.03;
    let (world, mut h) = flipped_hat();
    soft_jaws(&world, &mut h, 25.05, into);
    let r = refusals(&world);
    assert!(r.len() == 1 && r[0].contains("fixture 'Soft jaws'") && r[0].contains("0.03 mm deep"), "{r:?}");
}

#[test]
fn a_nest_floor_touching_within_the_contact_tolerance_is_held() {
    let (world, mut h) = flipped_hat();
    soft_jaws(&world, &mut h, 25.05, 0.005);
    let r = refusals(&world);
    assert!(r.is_empty(), "contact within {CARRIED_CONTACT_TOL} mm: {r:?}");
}


#[test]
fn a_post_narrower_than_the_pitch_between_the_columns_is_refused() {
    // 0.19 x 0.19 mm, 3 mm into the part, on no column's line.
    let (world, mut h) = flipped_hat();
    let (x, y) = between_columns(&world, 10.0, 10.0);
    let r = 0.095;
    let post = (Point3::new(x - r, y - r, 0.0), Point3::new(x + r, y + r, 3.0));
    add_jaws(&world, &mut h, soft_jaws_mesh(25.05, 0.0, &[post]));
    let r = refusals(&world);
    assert!(r.len() == 1 && r[0].contains("fixture 'Soft jaws'") && r[0].contains("3.00 mm deep"), "{r:?}");
}

#[test]
fn a_lip_narrower_than_the_pitch_between_the_columns_is_refused() {
    // 0.19 mm wide along Y, 4 mm into the part, between two column rows.
    let (world, mut h) = flipped_hat();
    let (x, _) = between_columns(&world, 10.0, 0.0);
    let lip = (Point3::new(x - 0.095, -24.0, 0.0), Point3::new(x + 0.095, 24.0, 4.0));
    add_jaws(&world, &mut h, soft_jaws_mesh(25.05, 0.0, &[lip]));
    let r = refusals(&world);
    assert!(r.len() == 1 && r[0].contains("fixture 'Soft jaws'") && r[0].contains("4.00 mm deep"), "{r:?}");
}

#[test]
fn an_open_mesh_keeps_the_box_verdict() {
    // The clear nest, one facet short: no inside to judge, so the box
    // (Z -10..5 over the part) refuses as before #1530.
    let (world, mut h) = flipped_hat();
    let mut m = soft_jaws_mesh(25.05, 0.0, &[]);
    let mut facets: Vec<Triangle> = m.facets.iter().copied().collect();
    facets.pop();
    m.facets = facets.into();
    add_jaws(&world, &mut h, m);
    let r = refusals(&world);
    assert!(r.len() == 1 && r[0].contains("fixture 'Soft jaws'") && r[0].contains("5.00 mm deep"), "{r:?}");
}

#[test]
fn a_coarse_fidelity_fixture_keeps_the_box_verdict() {
    // #864: a mesh tessellated coarser than the importer promises is not
    // its shape; the box refuses the clear nest.
    let (world, mut h) = flipped_hat();
    soft_jaws(&world, &mut h, 25.05, 0.0);
    assert!(refusals(&world).is_empty(), "at full fidelity the nest is held");
    let e = model::find_by_name(&world, "Soft jaws").expect("jaws");
    model_fidelity::set(&world, &e, Some(1.0));
    let r = refusals(&world);
    assert!(r.len() == 1 && r[0].contains("fixture 'Soft jaws'") && r[0].contains("5.00 mm deep"), "{r:?}");
}

/// A 60 x 30 bar in 66 x 36 stock, its sides contoured down to Z 4 (as
/// the hat), then Setup2 turned 90 degrees about Z (not flipped): the
/// carried body is the 30 x 60 section Z 4..10 on a 36 x 66 brim Z 0..4.
fn turned_bar() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-30.0, -15.0, 0.0), Point3::new(30.0, 15.0, 10.0))));
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 66.0, dy: 36.0, dz: 10.0 }, at).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    let op = CreateOperation::new(&world, "Sides", TOOL, SrcRef::Entity("part".into()), contour()).expect("create");
    h.execute(&world, op);
    h.execute(&world, GenerateOperation::new(&world, "Sides").expect("generate"));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", AlignPreset::TurnZ90).expect("turn"));
    h.execute(&world, CreateOperation::new(&world, "Top", TOOL, SrcRef::Stock, face(10.0)).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Top").expect("generate"));
    (world, h)
}

/// A pocket `hx` x `hy` from the centre, Z 0..10, in a block standing
/// on a plate (Z -10..0): the brim sits on the plate inside the pocket.
fn pocket(hx: f64, hy: f64) -> Mesh {
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    mesh_of(&[
        (p(-50.0, -50.0, -10.0), p(50.0, 50.0, 0.0)),
        (p(-50.0, -50.0, 0.0), p(-hx, 50.0, 10.0)),
        (p(hx, -50.0, 0.0), p(50.0, 50.0, 10.0)),
        (p(-hx, -50.0, 0.0), p(hx, -hy, 10.0)),
        (p(-hx, hy, 0.0), p(hx, 50.0, 10.0)),
    ])
}

#[test]
fn a_turned_setup_judges_its_fixture_in_the_turned_frame() {
    // #1530 review: a rotated (not flipped) setup. The pocket fits the
    // turned brim (X +-18, Y +-33); read in the unturned frame (X +-33)
    // its walls would stand in it. One mm short in Y, they do.
    let (world, mut h) = turned_bar();
    let body = incoming_body(&world, "Setup2").expect("carried");
    let (lo, hi) = body.bounds().expect("material");
    assert!(hi.y - lo.y > hi.x - lo.x + 20.0, "the bar enters turned: {lo:?} {hi:?}");
    add_jaws(&world, &mut h, pocket(18.05, 33.05));
    let r = refusals(&world);
    assert!(r.is_empty(), "the brim sits in the pocket: {r:?}");
    let (world, mut h) = turned_bar();
    add_jaws(&world, &mut h, pocket(18.05, 32.0));
    let r = refusals(&world);
    assert!(r.len() == 1 && r[0].contains("fixture 'Soft jaws'") && r[0].contains("4.00 mm deep"), "{r:?}");
}
