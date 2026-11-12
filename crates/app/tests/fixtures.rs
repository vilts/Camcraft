//! M3.2 stage B acceptance: a fixture is an ordinary model wearing a
//! marker, the machine table is derived from the `.mch` and never an
//! entity, the collision laws fire on planted steel and stay silent
//! without it, and every fixture command round-trips through the
//! history - including the marker surviving a plain tree delete.
//!
//! Gate map (M4.4): covers cl::cl, app::commands, app::fixture, sim::fixture, geometry::geo2d, app::geometry, app::machine, post::machine, app::model, app::operation, app::posting, app::schema, app::setup, geometry::geometry::stock, tools::tools, sim::verify, app::world.

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::commands::{
    AddTool, CommandError, CreateOperation, CreateSetup, DeleteEntity, DeleteFixture, GenerateOperation,
    History, MoveFixture, SetStock, TagFixture, UntagFixture,
};
use camcraft_app::fixture::{self, FixtureBox, MACHINE_TABLE};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::machine::{self, AxisLimits, Machine};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::{FixtureDef, Location};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::verify::{
    bands_vs_fixtures, FixturePart, Mode, ToolGeom, VerifyContext, Warning, verify,
};
use camcraft_app::world::new_world;

/// Stock + a 4 mm end mill + a generated face operation: the same
/// fixture the operations suite runs on, rebuilt here so the live-gate
/// law can be told end to end (staleness, posting refusal, release).
fn face_world() -> (flecs_ecs::prelude::World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 60.0,
                dy: 60.0,
                dz: 20.0,
            },
            Location::default(),
        )
        .expect("box stock"),
    );
    history.execute(
        &world,
        AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL")).expect("catalog tool"),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Face1",
            "EM-0400-2FL",
            SrcRef::Stock,
            OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
                stepover: 3.0,
                clearance_z: Height::fixed(25.0),
                cut_z: Height::fixed(19.5),
                top_z: None,
                stepdown: None,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        )
        .expect("create face op"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("generate face op"),
    );
    (world, history)
}

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn p2(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        p2(x0, y0),
        p2(x1, y0),
        p2(x1, y1),
        p2(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

fn deep_tool() -> ToolGeom {
    ToolGeom {
        radius: 4.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 9.0,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    }
}

fn ctx<'a>(
    region: &'a Region,
    tool: &'a ToolGeom,
    fixtures: &'a [FixtureBox],
    spindle: Option<(f64, f64)>,
) -> VerifyContext<'a> {
    VerifyContext {
        region,
        stock: None,
        model: None,
        tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 0.0,
        clearance_z: 5.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures,
        spindle,
        ..Default::default()
    }
}

/// Facets whose world AABB is exactly `min..max` (three corners carry
/// the extremes; the AABB of a triangle is the AABB of its corners).
fn box_facets(min: Point3, max: Point3) -> Vec<Triangle> {
    vec![Triangle::new(
        min,
        p(max.x, max.y, max.z),
        p(min.x, max.y, max.z),
    )]
}

/// One jaw's worth of steel, standing clear of the part: the planted
/// collision. Its z band [40, 70] is exactly the shank band of
/// `deep_tool` cutting at -4 (26..66).
fn jaw_box() -> FixtureBox {
    FixtureBox {
        min: p(10.0, 10.0, 40.0),
        max: p(18.0, 20.0, 70.0),
    }
}

/// A plain model entity with geometry, the way `ImportModel`'s apply
/// leaves it - the starting point every tag in these tests rides on.
fn add_model(world: &flecs_ecs::prelude::World, name: &str, tris: Vec<Triangle>) {
    let entity = model::spawn(world, name);
    model::set_facets(world, &entity, tris.into());
    model::set_rev(world, &entity, 1);
}

/// The stage-B fixture recipe the rest of the tests share: a vise-jaw
/// solid, tagged through the history so the command path is exercised.
fn plant_jaw(world: &flecs_ecs::prelude::World, history: &mut History, name: &str) {
    add_model(world, name, box_facets(jaw_box().min, jaw_box().max));
    history.execute(
        world,
        TagFixture::new(world, name, None).expect("the model exists to be tagged"),
    );
}

/// A VMC with a stated 200 mm cube of travel and a 60 mm spindle body
/// standing 40 mm proud - every number derived fixtures need, stated.
fn vmc() -> Machine {
    let axis = |n: &str| AxisLimits {
        name: n.to_string(),
        travel: Some((-100.0, 100.0)),
        ..Default::default()
    };
    Machine {
        model: "TestBench VMC".to_string(),
        axes: vec![axis("X"), axis("Y"), axis("Z")],
        spindle_d: Some(60.0),
        spindle_len: Some(40.0),
        ..Default::default()
    }
}

#[test]
fn point_and_cylinder_predicates_err_toward_hit() {
    let b = jaw_box();
    // On a face is inside: steel contact is contact.
    assert!(fixture::point_inside(&b, p(10.0, 15.0, 55.0), 0.0));
    assert!(fixture::point_inside(&b, p(9.0, 15.0, 55.0), 1.5));
    assert!(!fixture::point_inside(&b, p(9.0, 15.0, 55.0), 0.5));
    // A band whose r5 disc just touches the y=10 face from outside is
    // XY touching - a hit; but a band whose z span merely TOUCHES the
    // box bottom is flush contact, not a crash, however deep in XY.
    assert!(fixture::cylinder_hits_box(10.0, 5.0, 45.0, 55.0, 5.0, &b));
    assert!(!fixture::cylinder_hits_box(14.0, 15.0, 30.0, 40.0, 2.0, &b));
    assert!(fixture::cylinder_hits_box(14.0, 15.0, 30.0, 40.001, 2.0, &b));
    // Clear in XY is clear at any depth.
    assert!(!fixture::cylinder_hits_box(100.0, 100.0, 0.0, 1_000.0, 9.0, &b));
    // Non-finite clearance fails CLOSED - a hit, never a pass.
    assert!(fixture::cylinder_hits_box(
        f64::NAN,
        5.0,
        45.0,
        55.0,
        1.0,
        &b
    ));
}

#[test]
fn rapids_dip_through_steel_but_not_across_its_top() {
    // Steel is not kerf: the rapid law's vertical exemption does not
    // carry over - a vertical retract through the jaw is reported.
    assert!(fixture::rapid_hits_box(
        &p(14.0, 15.0, 30.0),
        &p(14.0, 15.0, 50.0),
        4.0,
        &jaw_box()
    ));
    // Lifting entirely above the box top is the strict-dip silence.
    assert!(!fixture::rapid_hits_box(
        &p(14.0, 15.0, 70.0),
        &p(14.0, 15.0, 90.0),
        4.0,
        &jaw_box()
    ));
    // A traverse level with the steel, tool disc just reaching the
    // face: the r4 disc spans x 2..10 at its end - touching is cutting
    // air into steel's footprint, so it is reported.
    assert!(fixture::rapid_hits_box(
        &p(0.0, 15.0, 55.0),
        &p(6.0, 15.0, 55.0),
        4.0,
        &jaw_box()
    ));
    assert!(!fixture::rapid_hits_box(
        &p(0.0, 15.0, 55.0),
        &p(6.0, 15.0, 55.0),
        1.0,
        &jaw_box()
    ));
}

/// The swept band: the cylinder rides the move's whole XY path, so a
/// long stroke over a small vise reports - an endpoint-sampled law
/// could be read straight past an 8 mm jaw on a 60 mm face line.
#[test]
fn a_band_is_swept_along_the_move_not_sampled_at_endpoints() {
    let b = FixtureBox {
        min: p(10.0, 10.0, 18.0),
        max: p(18.0, 20.0, 22.0),
    };
    // A 60 mm line crossing the jaw's footprint, no endpoint inside it.
    assert!(fixture::band_hits_box(-30.0, 15.0, 30.0, 15.0, 19.5, 29.5, 2.0, &b));
    // The same line 3.5 mm clear of the y=20 flank, r 2 disc: silent.
    assert!(!fixture::band_hits_box(-30.0, 23.5, 30.0, 23.5, 19.5, 29.5, 2.0, &b));
    // Touching counts: flank exactly at the swept disc's edge.
    assert!(fixture::band_hits_box(-30.0, 22.0, 30.0, 22.0, 19.5, 29.5, 2.0, &b));
    // Flush z (band floor at the box top) is contact, not a crash.
    assert!(!fixture::band_hits_box(14.0, 15.0, 14.0, 15.0, 22.0, 30.0, 2.0, &b));
    // Degenerate segment (pure plunge), inside the footprint, window
    // ending below the steel: silent.
    assert!(!fixture::band_hits_box(14.0, 15.0, 14.0, 15.0, 10.0, 17.999, 2.0, &b));
    // Non-finite clearance fails CLOSED.
    assert!(fixture::band_hits_box(f64::NAN, 15.0, 30.0, 15.0, 19.5, 29.5, 2.0, &b));
}

#[test]
fn a_shank_that_clears_stock_but_hits_planted_steel_warns_fixture_hit() {
    // The stage-B headline through the world: the SAME geometry as an
    // ordinary model, wearing the marker, reaches verify only through
    // fixture::boxes_for - and with nothing tagged the field is inert.
    let world = new_world();
    let mut history = History::default();
    // The region reaches past the ramp (x from -5): this test is about
    // the fixture law, and a ramp descending below the stock top within
    // r of a pocket wall is a true gouge since #42's whole-chord reading.
    let region = rect(-10.0, 0.0, 30.0, 30.0);
    let tool = deep_tool();
    let path = Toolpath::new(vec![Move::new(
        MoveKind::Feed,
        p(-5.0, 15.0, 5.0),
        p(6.0, 15.0, -4.0),
        Some(400.0),
    )]);

    assert!(fixture::boxes_for(&world, None).is_empty());
    assert_eq!(
        verify(&path, &ctx(&region, &tool, &fixture::boxes_for(&world, None), None)),
        Vec::new()
    );

    plant_jaw(&world, &mut history, "Vise1");
    let boxes = fixture::boxes_for(&world, None);
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0], jaw_box(), "a fixture box is the model's world AABB");
    assert_eq!(
        verify(&path, &ctx(&region, &tool, &boxes, None)),
        vec![Warning::FixtureHit {
            at: 0,
            fixture_part: FixturePart::Shank,
        }]
    );

    // Untagging demotes to a plain model: geometry stays, the collision
    // law stops seeing it.
    history.execute(
        &world,
        UntagFixture::new(&world, "Vise1").expect("it is a fixture"),
    );
    assert!(model::find_by_name(&world, "Vise1").is_some());
    assert!(model::facets_of(&world, &model::find_by_name(&world, "Vise1").unwrap()).is_some());
    assert!(fixture::boxes_for(&world, None).is_empty());
    assert_eq!(
        verify(&path, &ctx(&region, &tool, &fixture::boxes_for(&world, None), None)),
        Vec::new()
    );
    assert!(history.undo(&world)); // re-tag
    assert_eq!(fixture::list(&world, None), vec!["Vise1".to_string()]);
}

#[test]
fn a_leaning_shank_reports_steel_the_prismatic_law_cannot_reach() {
    // Pose (M5.1 A) through the swept-band law: the same tip, the same
    // jaw, four poses. Upright at x -12 the tool is all column: shank
    // x -21..-3, holder x -32..8 - every band falls short of steel at
    // x >= 10, and the flute (cutting air over an empty region) hits
    // nothing. Lean 20 degrees toward +x and the shank's AXIS LINE
    // physically crosses the jaw's footprint at z 56..62 - and the
    // law's ENCLOSING vertical column (shifted to the band's
    // mid-height point, widened by the half-length's spread) reports
    // the Shank hit the prismatic law must miss. Lean the other way
    // and the enclosure stays where the tool is: silent. The enclosing
    // model can over-report a graze; it never hides a crash, and the
    // exact vertical takes the law's original expressions bit-for-bit.
    let tool = deep_tool();
    let b = jaw_box();
    let (sa, ca) = (20f64.to_radians().sin(), 20f64.to_radians().cos());
    let chord = 0.01;
    let at = |axis: Option<[f64; 3]>| {
        let mut m = Move::new(MoveKind::Feed, p(-12.0, 15.0, 5.0), p(-12.0, 15.0, -4.0), Some(400.0));
        m.axis = axis;
        bands_vs_fixtures(
            &Toolpath::new(vec![m]),
            &tool,
            &[b],
            None,
            chord,
        )
    };
    assert_eq!(at(None), Vec::new(), "the upright column clears the jaw");
    assert_eq!(at(Some([0.0, 0.0, 1.0])), Vec::new(), "an exact vertical is the same law");
    assert_eq!(
        at(Some([sa, 0.0, ca])),
        vec![Warning::FixtureHit { at: 0, fixture_part: FixturePart::Shank }],
        "leaning INTO the jaw is reported"
    );
    assert_eq!(at(Some([-sa, 0.0, ca])), Vec::new(), "leaning AWAY stays silent");
}

#[test]
fn a_rapid_dipping_through_a_jaw_is_a_hit_and_through_air_is_not() {
    let region = rect(200.0, 200.0, 230.0, 230.0);
    let tool = deep_tool();
    let path = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(14.0, 15.0, 30.0), p(14.0, 15.0, 50.0), None),
        Move::new(MoveKind::Rapid, p(14.0, 15.0, 80.0), p(14.0, 15.0, 90.0), None),
    ]);
    let boxes = [jaw_box()];
    assert_eq!(
        verify(&path, &ctx(&region, &tool, &boxes, None)),
        vec![Warning::FixtureHit {
            at: 0,
            fixture_part: FixturePart::Rapid,
        }]
    );
    assert_eq!(verify(&path, &ctx(&region, &tool, &[], None)), Vec::new());
}

#[test]
fn the_machine_table_is_derived_from_stated_travel() {
    let world = new_world();
    // No machine: no table, by the unstated-limit law.
    assert!(fixture::table_box(&world, None).is_none());

    machine::set(&world, "vmc.mch", Some(&vmc()));
    let t = fixture::table_box(&world, None).expect("the file states its travel");
    assert_eq!(t.max, p(100.0, 100.0, -100.0));
    assert_eq!(t.min.x, -100.0);
    assert_eq!(t.min.y, -100.0);
    assert_eq!(t.min.z, -100.0 - fixture::MAX_FIXTURE_DIM);
    // The virtual row shows up in the list and the collision set.
    assert_eq!(fixture::list(&world, None), vec![MACHINE_TABLE.to_string()]);
    assert_eq!(fixture::boxes_for(&world, None), vec![t]);

    // An axis with no stated stroke means no table - a machine we
    // cannot place the top on is a machine we cannot collide-check.
    let mut quiet = vmc();
    quiet.axes[2].travel = None;
    machine::set(&world, "quiet.mch", Some(&quiet));
    assert!(fixture::table_box(&world, None).is_none());
    assert!(fixture::list(&world, None).is_empty());

    machine::set(&world, "", None);
    assert!(fixture::table_box(&world, None).is_none());
}

#[test]
fn cutting_below_the_reachable_z_hits_the_table_and_above_it_is_clean() {
    // The table's headline through verify: the derived box is just
    // another member of the collision set; a normal surface cut never
    // touches it, a plunge past the machine's lowest Z always reports.
    let world = new_world();
    machine::set(&world, "vmc.mch", Some(&vmc()));
    let region = rect(-50.0, -50.0, 50.0, 50.0);
    let tool = deep_tool();
    let boxes = fixture::boxes_for(&world, None);

    let surface = Toolpath::new(vec![Move::new(
        MoveKind::Feed,
        p(0.0, 0.0, 1.0),
        p(2.0, 0.0, -2.0),
        Some(400.0),
    )]);
    assert_eq!(
        verify(&surface, &ctx(&region, &tool, &boxes, None)),
        Vec::new()
    );

    let beyond = Toolpath::new(vec![Move::new(
        MoveKind::Feed,
        p(0.0, 0.0, -90.0),
        p(0.0, 0.0, -104.0),
        Some(200.0),
    )]);
    // Named as what meets the steel: the 30 mm flute column spans
    // z [-104, -74] and dips 4 mm INTO the table top at -100, while the
    // shank band [-74, -34] stays wholly above it. The flute itself is
    // in the steel - reported as "flute", never smuggled through the
    // old label that called every band-fallback hit a "shank".
    assert_eq!(
        verify(&beyond, &ctx(&region, &tool, &boxes, None)),
        vec![Warning::FixtureHit {
            at: 0,
            fixture_part: FixturePart::Flute,
        }]
    );
}

#[test]
fn the_spindle_band_lives_past_the_holder_only_the_mch_beyond() {
    // The tall steel at x 30 stands outside the shank (9) and holder
    // (20) cylinders but inside a 30 mm spindle radius, in the band
    // that starts where the holder ends (z_deep + 130). The region
    // reaches past the ramp (see the shank test above, #42).
    let region = rect(-10.0, 0.0, 30.0, 30.0);
    let tool = deep_tool();
    let path = Toolpath::new(vec![Move::new(
        MoveKind::Feed,
        p(-5.0, 15.0, 5.0),
        p(6.0, 15.0, -4.0),
        Some(400.0),
    )]);
    let steel = [FixtureBox {
        min: p(30.0, 5.0, 130.0),
        max: p(38.0, 25.0, 160.0),
    }];
    // Without stated spindle geometry the same steel is invisible:
    // pre-stage-B verdicts do not move.
    assert_eq!(verify(&path, &ctx(&region, &tool, &steel, None)), Vec::new());
    assert_eq!(
        verify(&path, &ctx(&region, &tool, &steel, Some((30.0, 40.0)))),
        vec![Warning::FixtureHit {
            at: 0,
            fixture_part: FixturePart::Spindle,
        }]
    );
    // One report per move, worst element first: steel spanning the
    // whole stack reports the shank, not a pile-up.
    let tall = [FixtureBox {
        min: p(10.0, 10.0, 26.0),
        max: p(38.0, 25.0, 166.0),
    }];
    assert_eq!(
        verify(&path, &ctx(&region, &tool, &tall, Some((30.0, 40.0)))),
        vec![Warning::FixtureHit {
            at: 0,
            fixture_part: FixturePart::Shank,
        }]
    );
}

#[test]
fn the_fixture_commands_roundtrip() {
    let world = new_world();
    let mut history = History::default();

    // Tag, untag (via the invert of a re-tag), move, delete - and every
    // inverse restores bytes, not re-derivations.
    add_model(
        &world,
        "Vise1",
        box_facets(jaw_box().min, jaw_box().max),
    );
    let placed = Location {
        pos: p(250.0, 0.0, 0.0),
        ..Location::default()
    };
    let why = MoveFixture::new(&world, "Vise1", placed).map(drop).err();
    assert!(
        matches!(why, Some(CommandError::Invalid(_))),
        "an untagged model is not a fixture to move: {why:?}"
    );
    history.execute(
        &world,
        TagFixture::new(&world, "Vise1", None).expect("a free model tags fine"),
    );
    let entity = model::find_by_name(&world, "Vise1").expect("tagged");
    assert_eq!(
        fixture::def_of(&world, &entity),
        Some(FixtureDef::default()),
        "the marker is the whole state"
    );
    assert_eq!(model::facets_of(&world, &entity).map(|f| f.len()), Some(1));
    assert!(model::is_fixture(&world, &entity));

    history.execute(
        &world,
        MoveFixture::new(&world, "Vise1", placed).expect("the fixture exists"),
    );
    assert_eq!(model::transform_of(&world, &entity), Some(placed));
    // The box follows the pose: world AABB, transformed - conservative
    // by construction (module law).
    assert_eq!(
        fixture::boxes_for(&world, None)[0],
        FixtureBox {
            min: p(260.0, 10.0, 40.0),
            max: p(268.0, 20.0, 70.0),
        }
    );
    assert!(history.undo(&world));
    assert_eq!(model::transform_of(&world, &entity), Some(Location::default()));

    // Delete: marker, placement, and geometry all come back verbatim.
    history.execute(
        &world,
        MoveFixture::new(&world, "Vise1", placed).expect("moved"),
    );
    history.execute(
        &world,
        DeleteFixture::new(&world, "Vise1").expect("the fixture exists"),
    );
    assert!(model::find_by_name(&world, "Vise1").is_none());
    assert!(history.undo(&world));
    // Re-read by name: the undo's spawn is a fresh handle.
    let entity = model::find_by_name(&world, "Vise1").expect("restored");
    assert_eq!(fixture::def_of(&world, &entity), Some(FixtureDef::default()));
    assert_eq!(model::transform_of(&world, &entity), Some(placed));
    assert_eq!(model::facets_of(&world, &entity).map(|f| f.len()), Some(1));

    // The plain tree delete keeps the marker through its undo too -
    // undo must not quietly demote a vise to a raw solid.
    history.execute(
        &world,
        DeleteEntity::new(&world, "Vise1").expect("it is there"),
    );
    assert!(model::find_by_name(&world, "Vise1").is_none());
    assert!(history.undo(&world));
    let entity = model::find_by_name(&world, "Vise1").expect("restored again");
    assert!(model::is_fixture(&world, &entity));
    assert_eq!(fixture::list(&world, None), vec!["Vise1".to_string()]);
}

#[test]
fn re_binding_a_fixture_is_undoable_as_a_re_bind() {
    let world = new_world();
    let mut history = History::default();
    // Materialize stage A's furniture so there are two contexts.
    history.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    history.execute(&world, CreateSetup::new(&world, None, None).expect("Setup2"));
    plant_jaw(&world, &mut history, "Vise1");

    // The implicit law: a fixture bound to nothing rides the FIRST
    // setup, and a named context sees only its own.
    let setups = setup_names(&world);
    assert_eq!(setups.len(), 2);
    assert_eq!(fixture::list(&world, Some(setups[0].as_str())), vec!["Vise1".to_string()]);
    assert!(fixture::list(&world, Some(setups[1].as_str())).is_empty());

    history.execute(
        &world,
        TagFixture::new(&world, "Vise1", Some(setups[1].as_str())).expect("re-bind to Setup2"),
    );
    assert!(fixture::list(&world, Some(setups[0].as_str())).is_empty());
    assert_eq!(fixture::list(&world, Some(setups[1].as_str())), vec!["Vise1".to_string()]);

    // Undo restores the PREVIOUS binding - it is not an untag.
    assert!(history.undo(&world));
    let entity = model::find_by_name(&world, "Vise1").expect("still tagged");
    assert_eq!(
        fixture::def_of(&world, &entity),
        Some(FixtureDef::default()),
        "undo lands back on the implicit first setup, not on no-fixture"
    );
    assert_eq!(fixture::list(&world, Some(setups[0].as_str())), vec!["Vise1".to_string()]);
}

fn setup_names(world: &flecs_ecs::prelude::World) -> Vec<String> {
    camcraft_app::setup::names(world)
}

#[test]
fn construction_refuses_nonsense_before_the_history_sees_it() {
    let world = new_world();
    // A name that is not a model: the machine table row, then furniture.
    for name in ["", "  ", MACHINE_TABLE] {
        let why = TagFixture::new(&world, name, None).map(drop).err();
        assert!(
            matches!(why, Some(CommandError::Invalid(_) | CommandError::NotFound(_))),
            "{name:?}: {why:?}"
        );
    }
    assert!(matches!(
        MoveFixture::new(&world, MACHINE_TABLE, Location::default()),
        Err(CommandError::Invalid(_))
    ));
    assert!(matches!(
        DeleteFixture::new(&world, MACHINE_TABLE),
        Err(CommandError::Invalid(_))
    ));

    add_model(&world, "Block1", box_facets(p(0.0, 0.0, 0.0), p(30.0, 30.0, 20.0)));
    assert!(
        matches!(
            MoveFixture::new(&world, "Block1", Location::default()),
            Err(CommandError::Invalid(_))
        ),
        "an untagged model is not a fixture to move"
    );
    // Tagging furniture is refused: a stock wearing a fixture marker
    // would double the collision set against itself.
    assert!(matches!(
        TagFixture::new(&world, "Ghost", None),
        Err(CommandError::NotFound(_))
    ));
    // A binding must name a setup that exists.
    assert!(matches!(
        TagFixture::new(&world, "Block1", Some("Ghost")),
        Err(CommandError::NotFound(_))
    ));
    // Nonsense placement never enters the history.
    history_execute_tag(&world, "Block1");
    assert!(matches!(
        MoveFixture::new(
            &world,
            "Block1",
            Location {
                pos: p(f64::NAN, 0.0, 0.0),
                ..Location::default()
            }
        ),
        Err(CommandError::Invalid(_))
    ));
    assert!(matches!(
        MoveFixture::new(
            &world,
            "Block1",
            Location {
                pos: Point3::ZERO,
                rot: [0.0, 0.0, 0.0, 0.0],
            }
        ),
        Err(CommandError::Invalid(_))
    ));
    // And the refused attempts changed nothing.
    assert_eq!(
        model::transform_of(&world, &model::find_by_name(&world, "Block1").unwrap()),
        Some(Location::default())
    );
    assert_eq!(fixture::boxes_for(&world, None), vec![{
        // The one tag that DID run, at its identity pose.
        FixtureBox {
            min: p(0.0, 0.0, 0.0),
            max: p(30.0, 30.0, 20.0),
        }
    }]);
}

fn history_execute_tag(world: &flecs_ecs::prelude::World, name: &str) {
    let mut history = History::default();
    history.execute(
        world,
        TagFixture::new(world, name, None).expect("tag the block"),
    );
}

#[test]
fn workholding_never_stales_a_fresh_toolpath() {
    // The live-gate law (M3.2 B, the M2.3 precedent): what the program
    // clamps to is verification input, never a stamp input. Every steel
    // action - plant, move, untag, load the machine, bind to another
    // setup - must leave a generated op's status Ok. What a moved vise
    // changes is the post's ANSWER (the gate refuses now-here), not the
    // op's freshness. This is the test the stamp-digest draft would have
    // failed, and the UI vote/sim tests' law made mandatory.
    let (world, mut history) = face_world();
    let op = model::find_by_name(&world, "Face1").expect("Face1");
    assert_eq!(operation::status_of(&world, &op), OpStatus::Ok);

    // A machine with a stated table and spindle appears: the derived
    // steel joins the world, the stamp does not care.
    machine::set(&world, "vmc.mch", Some(&vmc()));
    assert_eq!(
        operation::status_of(&world, &op),
        OpStatus::Ok,
        "loading a machine must not re-generate the program (the roomy-one law)"
    );

    // Planted steel, moved steel, removed steel - statuses unmoved.
    plant_jaw(&world, &mut history, "Vise1");
    assert_eq!(operation::status_of(&world, &op), OpStatus::Ok);
    history.execute(
        &world,
        MoveFixture::new(
            &world,
            "Vise1",
            Location {
                pos: p(10.0, 0.0, 0.0),
                ..Location::default()
            },
        )
        .expect("placed"),
    );
    assert_eq!(operation::status_of(&world, &op), OpStatus::Ok);
    history.execute(
        &world,
        UntagFixture::new(&world, "Vise1").expect("untagged"),
    );
    assert_eq!(operation::status_of(&world, &op), OpStatus::Ok);

    // The setup-context boundary survives as a GATE boundary: steel
    // bound to the other setup never enters this op's boxes. Two
    // setups: the first materializes (the implicit one gets a name),
    // the second is the other context this law speaks of.
    history.execute(
        &world,
        CreateSetup::new(&world, None, None).expect("the first setup"),
    );
    history.execute(
        &world,
        CreateSetup::new(&world, None, None).expect("a second setup"),
    );
    let second = camcraft_app::setup::names(&world)
        .into_iter()
        .nth(1)
        .expect("two setups");
    add_model(&world, "Clamp1", box_facets(p(50.0, 0.0, 0.0), p(60.0, 20.0, 30.0)));
    history.execute(
        &world,
        TagFixture::new(&world, "Clamp1", Some(second.as_str())).expect("bound to the second setup"),
    );
    assert_eq!(operation::status_of(&world, &op), OpStatus::Ok);
    assert!(
        !fixture::boxes_for(&world, None)
            .iter()
            .any(|b| b.min.y == 0.0 && b.max.x == 60.0),
        "the other setup's steel must not enter this context's box list"
    );
    assert!(
        fixture::boxes_for(&world, Some(&second))
            .iter()
            .any(|b| b.max.x == 60.0),
        "and the bound steel IS present in its own context"
    );
}

#[test]
fn a_moved_vise_blocks_the_post_without_staling_anything() {
    // The other half of the live-gate law, end to end: fresh op, clean
    // world; plant a jaw INTO the shank band; the op is still Ok, but
    // the verification gate now refuses by name, part and move. Then
    // slide the vise clear: same fresh path, gate passes - no
    // regeneration happened anywhere in the story.
    let (world, mut history) = face_world();
    let op = model::find_by_name(&world, "Face1").expect("Face1");
    assert!(posting::verification_gate(&world).is_empty());

    // The steel stands inside the FACE's own window: z 18..22 across
    // the stock top, so the cutting flute (window 19.5 up its flute
    // length) crosses its footprint line after line. The shank starts
    // above 22 - the hit the gate must name is the flute column, the
    // law's last fallback: steel where the edge sweeps, never meant
    // to be cut.
    add_model(
        &world,
        "Vise1",
        box_facets(p(10.0, 10.0, 18.0), p(18.0, 20.0, 22.0)),
    );
    history.execute(
        &world,
        TagFixture::new(&world, "Vise1", None).expect("tagged"),
    );
    assert_eq!(
        operation::status_of(&world, &op),
        OpStatus::Ok,
        "the steel is the gate's business, not the stamp's"
    );
    let problems = posting::verification_gate(&world);
    assert!(
        problems
            .iter()
            .any(|pr| pr.starts_with("Face1:") && pr.contains("strikes a fixture")),
        "the gate must name op and hit: {problems:?}"
    );

    // Slide it clear - the same fresh toolpath now posts.
    history.execute(
        &world,
        MoveFixture::new(
            &world,
            "Vise1",
            // Far above the tallest band the tool stack can ever
            // sweep - no z overlap at any move, from any law point.
            Location {
                pos: p(0.0, 400.0, 200.0),
                ..Location::default()
            },
        )
        .expect("moved clear"),
    );
    assert_eq!(operation::status_of(&world, &op), OpStatus::Ok);
    assert!(
        posting::verification_gate(&world).is_empty(),
        "a moved-clear vise releases the same fresh program"
    );
}
