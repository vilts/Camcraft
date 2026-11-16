//! #339 slice 5: links routed over the setup's fixtures. A clamp
//! standing between two pockets, taller than the clearance plane: the
//! link that crossed it at the plane struck it (the gate refused the
//! post); regenerated, the link rises over the clamp and posts clean.
//! A clamp taller than the machine's Z stroke refuses generation by
//! name; a rapid run left at depth becomes retract / traverse / plunge;
//! a path whose links clear is left byte for byte.
//!
//! Gate map (M4.4): covers app::fixture, sim::fixture, app::operation (fixture_route,
//! generate, job), app::posting, sim::verify.

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::commands::{
    ActivateSetup, AddTool, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, SetSetupMachine, SetStock,
    TagFixture,
};
use camcraft_app::fixture::FixtureBox;
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::links::Links;
use camcraft_app::machine::{self, AxisLimits, Machine};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::fixture_route::{plan_links, LinkSteel, ROUTE_GAP};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::{bands_vs_fixtures, rapids_vs_fixtures, FixturePart, SteelBand, ToolGeom, Warning};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The clamp's top: well over the 25 mm clearance plane.
const CLAMP_TOP: f64 = 40.0;

/// Two 20 x 20 x 20 blocks, 60 mm apart, on one 120 x 40 x 20 stock: a
/// pocket over the part's footprint cuts both, linking between them at
/// the clearance plane over the gap.
fn two_pockets() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let mut f: Vec<Triangle> = cube(p(-50.0, 30.0, 0.0), p(-30.0, 50.0, 20.0)).facets.to_vec();
    f.extend(cube(p(30.0, 30.0, 0.0), p(50.0, 50.0, 20.0)).facets.iter().cloned());
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(f)));
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 120.0, dy: 40.0, dz: 20.0 }, Location { pos: p(0.0, 40.0, 0.0), ..Default::default() }).expect("stock"),
    );
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
    let params = OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: Height::fixed(22.0),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(15.0),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        tilt: None,
        links: Links::PLANAR,
    };
    let op = CreateOperation::new(&world, "Contour1", "EM-0400-2FL", SrcRef::Entity("part".into()), params);
    h.execute(&world, op.expect("contour op"));
    h.execute(&world, GenerateOperation::new(&world, "Contour1").expect("generate"));
    (world, h)
}

fn path_of(world: &World) -> Toolpath {
    let op = model::find_by_name(world, "Contour1").expect("Contour1");
    model::toolpath_of(world, &op).expect("generated").path
}

fn warnings_of(world: &World) -> Vec<Warning> {
    let op = model::find_by_name(world, "Contour1").expect("Contour1");
    model::toolpath_of(world, &op).expect("generated").warnings
}

/// A strap clamp across the gap between the blocks, standing to
/// `top`; closed and outward (its mesh judges as well as its box).
fn plant_clamp(world: &World, h: &mut History, top: f64) {
    let e = model::spawn(world, "Clamp1");
    model::set_facets(world, &e, cube(p(-5.0, 10.0, 0.0), p(5.0, 70.0, top)).facets.to_vec().into());
    model::set_rev(world, &e, 1);
    h.execute(world, TagFixture::new(world, "Clamp1", None).expect("tagged"));
}

fn struck(problems: &[String]) -> bool {
    problems.iter().any(|pr| pr.starts_with("Contour1:") && pr.contains("strikes a fixture"))
}

/// Every move crossing the clamp's mid-plane x = 0 within its length.
fn crossings(t: &Toolpath) -> Vec<&Move> {
    t.moves
        .iter()
        .filter(|m| {
            let (a, b) = (m.from, m.to);
            if a.x.min(b.x) > 0.0 || a.x.max(b.x) < 0.0 || a.x == b.x {
                return false;
            }
            let y = a.y + (b.y - a.y) * (0.0 - a.x) / (b.x - a.x);
            (8.0..=72.0).contains(&y)
        })
        .collect()
}

#[test]
fn a_link_over_a_clamp_between_two_parts_rises_and_posts() {
    let (world, mut h) = two_pockets();
    let before = path_of(&world);
    assert!(posting::verification_gate(&world).is_empty(), "no steel, nothing struck");
    let over = crossings(&before);
    assert!(!over.is_empty() && over.iter().all(|m| m.kind == MoveKind::Rapid), "the link crosses the gap: {over:?}");
    assert!(over.iter().all(|m| m.to.z < CLAMP_TOP), "at the clearance plane, under the clamp's top");

    plant_clamp(&world, &mut h, CLAMP_TOP);
    // Today's verdict on the link as laid: the gate refuses the post.
    let problems = posting::verification_gate(&world);
    assert!(struck(&problems), "the clearance-plane link strikes the clamp: {problems:?}");

    h.execute(&world, GenerateOperation::new(&world, "Contour1").expect("regenerate"));
    let after = path_of(&world);
    let problems = posting::verification_gate(&world);
    assert!(problems.is_empty(), "the routed link posts clean: {problems:?}");
    assert!(
        !warnings_of(&world).iter().any(|w| matches!(w, Warning::FixtureHit { .. })),
        "and generation warns of no steel: {:?}",
        warnings_of(&world)
    );
    for m in crossings(&after) {
        assert_eq!(m.kind, MoveKind::Rapid, "{m:?}");
        assert!(m.from.z >= CLAMP_TOP + ROUTE_GAP - 1e-6 && m.to.z >= CLAMP_TOP + ROUTE_GAP - 1e-6, "over the clamp: {m:?}");
    }
    // Only the link moved: every cut is the one laid before.
    let cuts = |t: &Toolpath| t.moves.iter().filter(|m| m.kind != MoveKind::Rapid).copied().collect::<Vec<_>>();
    assert_eq!(cuts(&before), cuts(&after), "the cuts are untouched");
    assert!(after.is_chained(), "the path stays chained");
    assert!(after.notes.iter().any(|n| n.contains("raised over the workholding") && n.contains("Clamp1")), "{:?}", after.notes);
}

#[test]
fn a_clamp_the_links_already_clear_leaves_the_path_byte_for_byte() {
    let (world, mut h) = two_pockets();
    let before = path_of(&world);
    // A low clamp, under every link: nothing to route.
    plant_clamp(&world, &mut h, 10.0);
    h.execute(&world, GenerateOperation::new(&world, "Contour1").expect("regenerate"));
    assert_eq!(path_of(&world), before, "a clear link is never re-laid");
    assert!(posting::verification_gate(&world).is_empty(), "{:?}", posting::verification_gate(&world));
}

#[test]
fn a_clamp_taller_than_the_z_stroke_refuses_generation_by_name() {
    let (world, mut h) = two_pockets();
    let axis = |n: &str, lo: f64, hi: f64| AxisLimits { name: n.to_string(), travel: Some((lo, hi)), ..Default::default() };
    let mch = Machine {
        model: "Short VMC".to_string(),
        axes: vec![axis("X", -200.0, 200.0), axis("Y", -200.0, 200.0), axis("Z", -100.0, 100.0)],
        ..Default::default()
    };
    machine::set(&world, "short.mch", Some(&mch));
    // Reachable steel still routes under the stated stroke.
    plant_clamp(&world, &mut h, CLAMP_TOP);
    h.execute(&world, GenerateOperation::new(&world, "Contour1").expect("routes under the stroke"));
    assert!(posting::verification_gate(&world).is_empty(), "{:?}", posting::verification_gate(&world));

    let e = model::find_by_name(&world, "Clamp1").expect("clamp");
    model::set_facets(&world, &e, cube(p(-5.0, 10.0, 0.0), p(5.0, 70.0, 400.0)).facets.to_vec().into());
    model::set_rev(&world, &e, 2);
    let err = match GenerateOperation::new(&world, "Contour1") {
        Ok(_) => panic!("a 400 mm clamp under a 100 mm stroke must refuse"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains("cannot clear fixture 'Clamp1'") && err.contains("Z 400.000"), "{err}");
}

#[test]
fn the_z_stroke_cap_reads_the_ops_own_setups_machine_not_the_active_one() {
    // #471/#482: the op lives in Setup1 (the implicit first setup); the
    // ACTIVE setup is Setup2. Only Setup1's stroke may cap the route.
    let (world, mut h) = two_pockets();
    let axis = |n: &str, lo: f64, hi: f64| AxisLimits { name: n.to_string(), travel: Some((lo, hi)), ..Default::default() };
    let mill = |model: &str, z_top: f64| Machine {
        model: model.to_string(),
        kind: Some("mill".to_string()),
        axes: vec![axis("X", -200.0, 200.0), axis("Y", -200.0, 200.0), axis("Z", -100.0, z_top)],
        ..Default::default()
    };
    for _ in 0..2 {
        h.execute(&world, CreateSetup::new(&world, None, None).expect("setup"));
    }
    let mount = |h: &mut History, setup: &str, m: Machine| {
        let file = format!("{}.mch", m.model);
        h.execute(&world, SetSetupMachine::new(&world, setup, Some((file, Some(m)))).expect("mount"));
    };
    mount(&mut h, "Setup1", mill("Tall VMC", 1000.0));
    mount(&mut h, "Setup2", mill("Short VMC", 100.0));
    h.execute(&world, ActivateSetup::new(&world, Some("Setup2")).expect("activate"));
    // A 400 mm clamp across the links: over the short stroke, under the
    // tall one. It starts at y 40, so the holder at the path's
    // start point (0, 0) stands clear of it - a link's own end column
    // under steel is no height's to clear.
    let e = model::spawn(&world, "Clamp1");
    model::set_facets(&world, &e, cube(p(-5.0, 40.0, 0.0), p(5.0, 70.0, 400.0)).facets.to_vec().into());
    model::set_rev(&world, &e, 1);
    h.execute(&world, TagFixture::new(&world, "Clamp1", None).expect("tagged"));
    h.execute(&world, GenerateOperation::new(&world, "Contour1").expect("routes on its own tall machine"));
    let notes = path_of(&world).notes;
    assert!(notes.iter().any(|n| n.contains("raised over the workholding") && n.contains("Z 405.000")), "{notes:?}");
    assert!(!struck(&posting::verification_gate(&world)), "{:?}", posting::verification_gate(&world));

    // Swap the steel: now the op's own machine is the short one, and the
    // active setup's tall stroke must not rescue it.
    mount(&mut h, "Setup1", mill("Short VMC", 100.0));
    mount(&mut h, "Setup2", mill("Tall VMC", 1000.0));
    let err = match GenerateOperation::new(&world, "Contour1") {
        Ok(_) => panic!("a 400 mm clamp under its own setup's 100 mm stroke must refuse"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains("cannot clear fixture 'Clamp1'"), "{err}");
}

/// The 4 mm end mill [`stack`] reads: flute r2 x 15, shank r2 x 25,
/// holder r15 x 50.
fn geom() -> ToolGeom {
    ToolGeom {
        radius: 2.0,
        nose_radius: 0.0,
        flute_len: 15.0,
        shank_len: 25.0,
        shank_radius: 2.0,
        holder_radius: 15.0,
        holder_len: 50.0,
    }
}

/// The gate's own fixture laws on a run, as the generate hook hands
/// them to the planner: (c2) bands and (d2) rapids, the whole upright
/// stack over each move's true z span (#424).
fn gate(path: &Toolpath, boxes: &[(String, FixtureBox)]) -> bool {
    let b: Vec<FixtureBox> = boxes.iter().map(|(_, b)| *b).collect();
    let t = geom();
    !(bands_vs_fixtures(path, &t, &b, None, 0.01).is_empty() && rapids_vs_fixtures(path, &t, &b).is_empty())
}

/// The fixture-free stack of a 4 mm end mill: flute, shank, holder.
fn stack() -> Vec<SteelBand> {
    vec![
        SteelBand { part: FixturePart::Shank, lo: 15.0, hi: 40.0, r: 2.0 },
        SteelBand { part: FixturePart::Holder, lo: 40.0, hi: 90.0, r: 15.0 },
        SteelBand { part: FixturePart::Flute, lo: 0.0, hi: 15.0, r: 2.0 },
    ]
}

/// A cut, a rapid run that stays at depth, a cut.
fn stay_down() -> Toolpath {
    Toolpath::new(vec![
        Move::new(MoveKind::Feed, p(-40.0, 0.0, 10.0), p(-30.0, 0.0, 10.0), Some(800.0)),
        Move::new(MoveKind::Rapid, p(-30.0, 0.0, 10.0), p(30.0, 0.0, 10.0), None),
        Move::new(MoveKind::Feed, p(30.0, 0.0, 10.0), p(40.0, 0.0, 10.0), Some(800.0)),
    ])
}

#[test]
fn a_stay_down_rapid_link_becomes_retract_traverse_plunge() {
    let post = FixtureBox { min: p(-3.0, -3.0, 0.0), max: p(3.0, 3.0, 30.0) };
    let boxes = vec![("Bolt".to_string(), post)];
    let stack = stack();
    let steel = LinkSteel { boxes: &boxes, stack: &stack, ceiling: None };
    let t = plan_links(stay_down(), &steel, |q| gate(q, steel.boxes)).expect("routable");
    let h = 30.0 + ROUTE_GAP;
    let want = [
        (MoveKind::Feed, p(-40.0, 0.0, 10.0), p(-30.0, 0.0, 10.0)),
        (MoveKind::Rapid, p(-30.0, 0.0, 10.0), p(-30.0, 0.0, h)),
        (MoveKind::Rapid, p(-30.0, 0.0, h), p(30.0, 0.0, h)),
        (MoveKind::Rapid, p(30.0, 0.0, h), p(30.0, 0.0, 10.0)),
        (MoveKind::Feed, p(30.0, 0.0, 10.0), p(40.0, 0.0, 10.0)),
    ];
    let got: Vec<_> = t.moves.iter().map(|m| (m.kind, m.from, m.to)).collect();
    assert_eq!(got, want);
    assert_eq!(t.notes.len(), 1, "{:?}", t.notes);

    // The same run clear of the bolt (moved off its line) is untouched.
    let far = vec![("Bolt".to_string(), FixtureBox { min: p(-3.0, 30.0, 0.0), max: p(3.0, 36.0, 30.0) })];
    let steel = LinkSteel { boxes: &far, stack: &stack, ceiling: None };
    assert_eq!(plan_links(stay_down(), &steel, |q| gate(q, steel.boxes)).expect("routable"), stay_down());
    // No steel at all: untouched.
    let steel = LinkSteel { boxes: &[], stack: &stack, ceiling: None };
    assert_eq!(plan_links(stay_down(), &steel, |_| true).expect("routable"), stay_down());
}

#[test]
fn steel_over_the_links_own_end_is_left_as_laid_and_said() {
    // An arm overhanging the run's start column: the holder meets it on
    // any retract from there, whatever the traverse height.
    let arm = FixtureBox { min: p(-40.0, -5.0, 60.0), max: p(-20.0, 5.0, 70.0) };
    let boxes = vec![("Arm".to_string(), arm)];
    let stack = stack();
    let steel = LinkSteel { boxes: &boxes, stack: &stack, ceiling: None };
    let t = plan_links(stay_down(), &steel, |q| gate(q, steel.boxes)).expect("not a refusal");
    assert_eq!(t.moves, stay_down().moves, "left as laid");
    assert!(t.notes.iter().any(|n| n.contains("no height clears it")), "{:?}", t.notes);
}

#[test]
fn a_ceiling_under_the_steel_refuses_and_over_it_caps_the_rise() {
    let post = FixtureBox { min: p(-3.0, -3.0, 0.0), max: p(3.0, 3.0, 30.0) };
    let boxes = vec![("Bolt".to_string(), post)];
    let stack = stack();
    let steel = LinkSteel { boxes: &boxes, stack: &stack, ceiling: Some(29.0) };
    let err = plan_links(stay_down(), &steel, |q| gate(q, steel.boxes)).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err.contains("moves 1..1, (-30.000, 0.000) to (30.000, 0.000)") && err.contains("'Bolt'") && err.contains("Z 29.000"), "{err}");
    let steel = LinkSteel { boxes: &boxes, stack: &stack, ceiling: Some(32.0) };
    let t = plan_links(stay_down(), &steel, |q| gate(q, steel.boxes)).expect("capped");
    assert!(t.moves.iter().map(|m| m.to.z).fold(f64::MIN, f64::max) == 32.0, "{:?}", t.moves);
}
