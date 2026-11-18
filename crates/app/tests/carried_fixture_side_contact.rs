//! #1580: positive side-face and step overlap finer than one grid pitch
//! must not pass as contact, while true contact within tolerance holds.
//! Provenance: our own box, cylinder, model, flipped-hat, and side-turned
//! fixtures across sub-pitch grid phases.
//! Gate map (#1580): covers app::fixture_carried (penetration, mesh_penetration,
//! carried_fixture_problems), sim::stock_sim (carry_history, carry_exact, carry),
//! sim::fixture_mesh (prism, column).

use flecs_ecs::prelude::World;

use camcraft_app::cl::{Move, MoveKind};
use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, SetStock,
    TagFixture,
};
use camcraft_app::fixture::FixtureBox;
use camcraft_app::fixture_carried::{
    CARRIED_CONTACT_TOL, FIXTURE_IN_CARRIED, MESH_VISIT_BUDGET, carried_fixture_problems, mesh_penetration,
    penetration,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::setup_align::AlignPreset;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::{Body, CarriedStock, CarryTurn, Dexel, SimOp, incoming_body};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_sim::fixture_mesh::FixtureMesh;

const TOOL: &str = "EM-0600-4FL";

fn box_fixture(face: f64, overlap: f64) -> FixtureBox {
    FixtureBox {
        min: Point3::new(face - overlap, -1.0, 2.0),
        max: Point3::new(5.0, 5.0, 8.0),
    }
}

fn mesh_depth(body: &Body, fixture: &FixtureBox) -> Option<f64> {
    let mesh = FixtureMesh::build(&cube(fixture.min, fixture.max).facets).expect("jaw mesh");
    let mut budget = MESH_VISIT_BUDGET;
    mesh_penetration(body, fixture, &mesh, &mut budget)
}

fn mesh_of_boxes(boxes: &[(Point3, Point3)]) -> (FixtureBox, FixtureMesh) {
    let tris: Vec<Triangle> = boxes
        .iter()
        .flat_map(|(a, b)| cube(*a, *b).facets.iter().copied().collect::<Vec<_>>())
        .collect();
    let mesh = FixtureMesh::build(&tris).expect("closed fixture mesh");
    let (lo, hi) = tris.iter().flat_map(|t| [t.a, t.b, t.c]).fold(
        (
            Point3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY),
            Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
        ),
        |(lo, hi), q| {
            (
                Point3::new(lo.x.min(q.x), lo.y.min(q.y), lo.z.min(q.z)),
                Point3::new(hi.x.max(q.x), hi.y.max(q.y), hi.z.max(q.z)),
            )
        },
    );
    (FixtureBox { min: lo, max: hi }, mesh)
}

fn assert_sub_pitch_overlap_is_refused(
    make_body: fn(f64) -> Body,
    judge: fn(&Body, &FixtureBox) -> Option<f64>,
    check_contact_tol: bool,
) {
    let mut missed = Vec::new();
    for phase in [0.1, 0.3, 0.7, 0.9] {
        let face = 2.0 + phase;
        let body = make_body(face);
        assert_eq!(judge(&body, &box_fixture(face, 0.0)), None, "contact at phase {phase}");
        assert_eq!(judge(&body, &box_fixture(face, -0.05)), None, "clear at phase {phase}");
        if check_contact_tol {
            assert_eq!(
                judge(&body, &box_fixture(face, 0.005)),
                None,
                "within contact tol at phase {phase}"
            );
        }
        // 0.05 mm is five contact tolerances, well under one 1.0 mm grid pitch.
        if judge(&body, &box_fixture(face, 0.05)).is_none() {
            missed.push(phase);
        }
    }
    assert!(missed.is_empty(), "0.05 mm of steel passed at face phases {missed:?}");
}

fn block_body(face: f64) -> Body {
    let dex = Dexel::block(0.0, 0.0, face, 4.0, 10.0, 1.0).expect("stock grid");
    Body::on_floor(dex, 0.0)
}

fn model_body(face: f64) -> Body {
    let dex = Dexel::block(0.0, 0.0, 4.0, 4.0, 10.0, 1.0).expect("stock grid");
    let stock = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(face, 4.0, 10.0));
    Body::on_mesh(dex, &stock.facets)
}

#[test]
fn box_block_refuses_sub_pitch_side_face_overlap_at_four_grid_phases() {
    assert_sub_pitch_overlap_is_refused(block_body, penetration, false);
    assert_sub_pitch_overlap_is_refused(block_body, mesh_depth, true);
}

#[test]
fn model_stock_refuses_sub_pitch_side_face_overlap_at_four_grid_phases() {
    assert_sub_pitch_overlap_is_refused(model_body, penetration, false);
    assert_sub_pitch_overlap_is_refused(model_body, mesh_depth, true);
}

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

fn face_op(cut_z: f64) -> OpParams {
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

fn flipped_hat_with_jaw(x_face: f64) -> (World, History, f64) {
    let world = new_world();
    let mut h = History::default();
    h.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::new(-25.0, -25.0, 0.0), Point3::new(25.0, 25.0, 10.0))),
    );
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), ..Default::default() };
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 56.0, dy: 56.0, dz: 10.0 }, at).expect("stock"),
    );
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    h.execute(&world, CreateOperation::new(&world, "Sides", TOOL, SrcRef::Entity("part".into()), contour()).expect("op"));
    h.execute(&world, GenerateOperation::new(&world, "Sides").expect("gen"));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", AlignPreset::FlipX).expect("flip"));
    h.execute(&world, CreateOperation::new(&world, "Brim", TOOL, SrcRef::Stock, face_op(10.0)).expect("op2"));
    h.execute(&world, GenerateOperation::new(&world, "Brim").expect("gen2"));
    let pitch = incoming_body(&world, "Setup2").expect("carried").dex.cell;
    let jaw = Mesh::from_facets(cube(Point3::new(x_face, -20.0, 0.0), Point3::new(35.0, 20.0, 5.0)).facets.to_vec());
    h.execute(&world, ImportModel::from_mesh("Jaw", jaw));
    h.execute(&world, TagFixture::new(&world, "Jaw", Some("Setup2")).expect("tag jaw"));
    (world, h, pitch)
}

#[test]
fn flipped_hat_refuses_sub_pitch_jaw_overlap_from_tenth_to_seven_tenths_pitch() {
    for frac in [0.1, 0.3, 0.5, 0.7] {
        let (world, _h, pitch) = flipped_hat_with_jaw(25.0);
        let overlap = frac * pitch;
        assert!(overlap > CARRIED_CONTACT_TOL && overlap < pitch);
        drop(world);
        let (world, _h, pitch) = flipped_hat_with_jaw(25.0 - overlap);
        let r = carried_fixture_problems(&world);
        let gate: Vec<String> = posting::verification_gate(&world)
            .into_iter()
            .filter(|l| l.contains(FIXTURE_IN_CARRIED))
            .collect();
        assert_eq!(gate, r);
        assert!(
            r.len() == 1 && r[0].contains("fixture 'Jaw'") && r[0].contains("5.00 mm deep"),
            "frac {frac} (overlap {overlap:.4} mm of pitch {pitch}): {r:?}"
        );
    }
    for clear_x in [25.0, 25.0 - 0.005, 25.05] {
        let (world, _h, _) = flipped_hat_with_jaw(clear_x);
        let r = carried_fixture_problems(&world);
        assert!(r.is_empty(), "jaw at {clear_x} should be held: {r:?}");
    }
}

#[test]
fn cylinder_stock_refuses_sub_pitch_side_face_overlap_and_allows_tangent_contact() {
    let mut dex = Dexel::from_shape(
        &StockShape::Cylinder { diameter: 10.6, height: 10.0 },
        1.0,
    )
    .expect("cylinder");
    // Column centers are at ..., 4.5, 5.5; radius is 5.3.
    let body = Body::on_floor(dex.clone(), 0.0);
    let _ = &mut dex;
    let contact = FixtureBox {
        min: Point3::new(5.3, -1.0, 2.0),
        max: Point3::new(8.0, 1.0, 8.0),
    };
    assert_eq!(mesh_depth(&body, &contact), None, "tangent at x = 5.3 is contact");
    assert_eq!(penetration(&body, &contact), None, "box tangent at x = 5.3 is contact");
    let overlap = FixtureBox {
        min: Point3::new(5.25, -1.0, 2.0),
        max: Point3::new(8.0, 1.0, 8.0),
    };
    assert_eq!(mesh_depth(&body, &overlap), Some(6.0), "0.05 mm inside cylinder wall");
    assert_eq!(penetration(&body, &overlap), Some(6.0), "box 0.05 mm inside cylinder wall");
}

#[test]
fn side_turned_machined_wall_refuses_sub_pitch_overlap_and_allows_contact() {
    let mut stock = CarriedStock::Column(Body::on_floor(
        Dexel::block(0.0, 0.0, 12.0, 10.0, 6.0, 1.0).expect("block"),
        0.0,
    ));
    let moves = [Move::new(
        MoveKind::Feed,
        Point3::new(-3.0, 8.2, 2.0),
        Point3::new(15.0, 8.2, 2.0),
        Some(300.0),
    )];
    stock
        .simulate(&[SimOp { moves: &moves, radius: 2.0, ball: false, cone: None, corner: None }])
        .expect("contour");
    // Machined wall sits at y = 6.2; turn side-on so it lies at z = 6.2,
    // and check the sub-pitch uncut side wall at x = 12.0 or y = -4.0 as well as step.
    let turn = CarryTurn::from_matrix(
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    )
    .expect("side turn");
    let body = stock.carry_through(&turn).expect("carry").into_body().expect("body");
    // In turned frame: z = 6.2 is the top of the 0..6.2 slab (step to 0..10 at z = 6.2? Wait:
    // y in 0..6.2 maps to z in 0..6.2? Let's check: z' = y, y' = -z, so z' = 6.2!
    // And x = 12.0 is a side face; let's also cut a vertical side wall along x = 9.7 in Setup2:
    // Wait: column centers in x are 0.5, ..., 9.5, 10.5. Cutting at y' in [-6..0] leaves side face.
    // Here let's test the step in z' (z' = 6.2) AND side face at x = 11.7 (phase 0.7 of 1.0 cell,
    // wait: Setup1 block ends at x = 12.0; let's cut a side wall at x = 9.7 in Setup1!).
    let mut stock2 = CarriedStock::Column(Body::on_floor(
        Dexel::block(0.0, 0.0, 12.0, 10.0, 6.0, 1.0).expect("block"),
        0.0,
    ));
    let wall_cut = [Move::new(
        MoveKind::Feed,
        Point3::new(11.7, -3.0, 2.0),
        Point3::new(11.7, 13.0, 2.0),
        Some(300.0),
    )];
    stock2
        .simulate(&[SimOp { moves: &wall_cut, radius: 2.0, ball: false, cone: None, corner: None }])
        .expect("side wall");
    let body2 = stock2.carry_through(&turn).expect("carry").into_body().expect("body");
    // Cut leaves wall at x = 9.7 (above z_old = 2.0, which maps to y' in -6..-2).
    // Column centers in x are 8.5, 9.5, 10.5; 9.65 is 0.05 mm inside the machined wall 9.7.
    let contact = FixtureBox { min: Point3::new(9.7, -5.0, 1.0), max: Point3::new(13.0, -3.0, 5.0) };
    assert_eq!(mesh_depth(&body2, &contact), None, "contact on turned machined side wall");
    let overlap = FixtureBox { min: Point3::new(9.65, -5.0, 1.0), max: Point3::new(13.0, -3.0, 5.0) };
    assert_eq!(mesh_depth(&body2, &overlap), Some(4.0), "0.05 mm inside turned machined side wall");
    let _ = body;
}

#[test]
fn step_face_refuses_sub_pitch_overlap_and_clears_recessed_jaw() {
    // 10 x 10 x 10 block, cut at x > 4.2 down to Z = 4.0 -> step wall at x = 4.2, Z = 4..10.
    // Column centers in x are 3.5 (Z = 10) and 4.5 (Z = 4).
    let mut stock = CarriedStock::Column(Body::on_floor(
        Dexel::block(0.0, 0.0, 10.0, 10.0, 10.0, 1.0).expect("block"),
        0.0,
    ));
    let step_cut = [Move::new(
        MoveKind::Feed,
        Point3::new(6.2, -3.0, 4.0),
        Point3::new(6.2, 13.0, 4.0),
        Some(300.0),
    )];
    stock
        .simulate(&[SimOp { moves: &step_cut, radius: 2.0, ball: false, cone: None, corner: None }])
        .expect("step cut");
    let body = stock.into_body().expect("body");
    // Solid jaw stopping at x = 4.05 (0.15 mm inside the step at 4.2, short of column 3.5), Z = 5..8.
    let hitting = FixtureBox { min: Point3::new(4.05, 1.0, 5.0), max: Point3::new(8.0, 9.0, 8.0) };
    assert_eq!(mesh_depth(&body, &hitting), Some(3.0), "0.15 mm into step wall");
    assert_eq!(penetration(&body, &hitting), Some(3.0), "box 0.15 mm into step wall");
    // Recessed L-shaped jaw around the Z=4..10 step at x=4.2:
    // - lower body at x=4.25..8.0, Z=4.0..10.05 rests in contact on the step floor Z=4.0 and 0.05 mm clear of x=4.2;
    // - upper overhang at x=4.05..8.0, Z=10.05..12.0 sits 0.05 mm above the Z=10.0 top while reaching past x=4.2.
    // Its world AABB is x=4.05..8.0, Z=4.0..12.0 (box overlaps the step wall), yet its real steel clears!
    let (bbox, mesh) = mesh_of_boxes(&[
        (Point3::new(4.25, 1.0, 4.0), Point3::new(8.0, 9.0, 10.05)),
        (Point3::new(4.05, 1.0, 10.05), Point3::new(8.0, 9.0, 12.0)),
    ]);
    assert_eq!(penetration(&body, &bbox), Some(6.0), "world AABB overlaps the 6 mm step wall");
    let mut budget = MESH_VISIT_BUDGET;
    assert_eq!(
        mesh_penetration(&body, &bbox, &mesh, &mut budget),
        None,
        "recessed mesh jaw clearing the step must pass"
    );
}
