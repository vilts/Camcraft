//! M2.4 A acceptance: the catalog's two roughing names over the
//! pocket's ladders. 2D Adaptive is the pocket run with the
//! `Adaptive` strategy pinned and the engagement door open; 2D
//! Offset is the pocket run with `Waterline` pinned and the rings
//! spaced by the stepover alone. The kinds are the name, the seed
//! and the digest tag - `machining2d::pocket` still does the work -
//! so the law this file enforces is byte-identity with the pocket
//! that means the same thing, the door that must be live actually
//! flowing, the retype carrying the numbers and pinning the ladder,
//! and the registry rows (5, 6, in the 2D mill's run) holding the
//! table's shape.
//!
//! Provenance: catalog naming per Autodesk's published HSMWorks
//! user documentation (behavior only); the ladders themselves are
//! ours (M8.1's engagement study, the waterline tiling from the
//! pocket's own law).
//!
//! Gate map (M4.4): covers app::commands, app::geometry, motion::machining2d,
//! motion::machining3d, app::model, app::operation, app::schema, geometry::geometry::stock,
//! tools::tools, app::world.

mod common;

use flecs_ecs::prelude::World;

use common::gen_to_cltext;

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, KindInfo, OpParams, OpStatus, OpType, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn add_tool(world: &World, history: &mut History, name: &str) {
    let cmd = AddTool::new(world, name, catalog_tool(name));
    history.execute(world, cmd.expect("catalog tool"));
}

/// The job every equivalence test runs: 60x60x20 box stock (z 0..20),
/// an 8 mm four-flute, the whole footprint hollowed to z=10 - the
/// region is the stock's own silhouette, so no model feature can make
/// the twins and the pocket disagree about anything but their name.
fn job(tag: &str, params: OpParams) -> (World, History, Toolpath) {
    job_on(tag, params, 60.0)
}

/// [`job`] on a `side` x `side` box: the HSM plan (M11.4 C) the
/// Adaptive kind now runs measures every cut against the stock, so its
/// equivalence laws ride a smaller footprint to stay in the debug lane -
/// the same laws, the same tool, a quarter of the area or less.
fn job_on(tag: &str, params: OpParams, side: f64) -> (World, History, Toolpath) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: side,
                dy: side,
                dz: 20.0,
            },
            Location::default(),
        )
        .expect("box stock"),
    );
    add_tool(&world, &mut history, "EM-0800-4FL");
    history.execute(
        &world,
        CreateOperation::new(&world, tag, "EM-0800-4FL", SrcRef::Stock, params)
            .expect("create"),
    );
    let cmd = GenerateOperation::new(&world, tag).expect("generate");
    history.execute(&world, cmd);
    let e = model::find_by_name(&world, tag).expect("op exists");
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok, "{tag} reads Stale against its own lone world");
    let path = model::toolpath_of(&world, &e).expect("cached").path;
    (world, history, path)
}

fn text(params: OpParams, tag: &str) -> String {
    let (_w, _h, path) = job(tag, params);
    gen_to_cltext(&path)
}

/// The side the Adaptive (HSM) equivalence laws run on - see [`job_on`].
const HSM_SIDE: f64 = 24.0;

fn text_hsm(params: OpParams, tag: &str) -> String {
    let (_w, _h, path) = job_on(tag, params, HSM_SIDE);
    gen_to_cltext(&path)
}

/// The shared numbers: every parameter the twins and the pocket all
/// have, held equal so the ONLY differences are the kind tag and
/// whatever the pin replaces.
fn pocket(strategy: PocketStrategy, eng: f64) -> OpParams {
    OpParams::Pocket {
        links: camcraft_app::links::Links::PLANAR,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(10.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::load_of_degrees(eng),
        tilt: None,
        blend_turns: false,
    }
}

fn adaptive(eng: f64) -> OpParams {
    OpParams::Adaptive2D {
        links: camcraft_app::links::Links::PLANAR,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(10.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::load_of_degrees(eng),
        tilt: None,
    }
}

fn offset() -> OpParams {
    OpParams::Offset2D {
        links: camcraft_app::links::Links::PLANAR,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(10.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
    }
}

#[test]
fn adaptive_kind_is_the_pockets_engagement_ladder() {
    // Two worlds, the same job: the catalog name and the pocket set
    // to Adaptive cut the same program byte for byte - the pin in
    // generate.rs is the only thing the name changes.
    let (_w, _h, t) = job_on("ADP", adaptive(45.0), HSM_SIDE);
    assert!(t.is_chained() && t.all_finite(), "a real engagement ladder");
    assert!(!t.moves.is_empty(), "and not an empty one");
    assert_eq!(
        text_hsm(adaptive(45.0), "ADP2"),
        text_hsm(pocket(PocketStrategy::Adaptive, 45.0), "PKT"),
        "the name must not move the G-code"
    );
}

#[test]
fn offset_kind_is_the_pockets_ring_tiling() {
    // Style-A rings: the pocket's Waterline tiling under a roughing
    // schedule, and the engagement value a Pocket still digests is
    // inert here - the stepover spaces the rings.
    assert_eq!(
        text(offset(), "OFF"),
        text(pocket(PocketStrategy::Waterline, 45.0), "PKT"),
        "2D Offset is the waterline ladder named"
    );
}

#[test]
fn the_engagement_door_is_live_on_adaptive_only() {
    // The field the kind exposes must actually reach the kernel:
    // 30° and 90° cut different ladders, and each equals the pocket
    // set to the same angle - so it is the number flowing, not a
    // constant wearing the field's name.
    let a30 = text_hsm(adaptive(30.0), "A30");
    let a90 = text_hsm(adaptive(90.0), "A90");
    assert_ne!(a30, a90, "the door bites or the kind is a lie");
    assert_eq!(a90, text_hsm(pocket(PocketStrategy::Adaptive, 90.0), "P90"));
    assert_eq!(a30, text_hsm(pocket(PocketStrategy::Adaptive, 30.0), "P30"));
}

#[test]
fn the_kind_tag_separates_the_twins_from_the_pocket() {
    // The digest law (M2.4 A): the strategy byte drops out of the
    // twins because the kind tag at the head already pins which
    // ladder runs. So a retype Pocket(Adaptive) -> Adaptive2D goes
    // Stale once (a different tag) and regenerates byte-identically
    // (the same path) - the mechanism saying so, as every strategy
    // rename always did. And the twins never collide with each other.
    let src = SrcRef::Stock;
    let w = new_world();
    let mut h = History::default();
    h.execute(
        &w,
        SetStock::new(&w, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default())
            .expect("box stock"),
    );
    add_tool(&w, &mut h, "EM-0800-4FL");
    let st = |p: &OpParams| operation::current_stamp(&w, "EM-0800-4FL", &src, p, "X", None);
    let pkt = pocket(PocketStrategy::Adaptive, 45.0);
    let adp = adaptive(45.0);
    assert_ne!(st(&pkt), st(&adp), "the twins are a different row of the table");
    assert_eq!(st(&adp), st(&adaptive(45.0)), "the same numbers, the same digest");
    assert_ne!(st(&adp), st(&offset()), "the twins are different rows");
    // The engagement digest law: the value rides the Adaptive row
    // (it changes the path) - and cannot ride the Offset row, which
    // has no such field to feed it.
    assert_ne!(st(&adaptive(30.0)), st(&adaptive(90.0)));
}

#[test]
fn a_retype_carries_the_numbers_and_pins_the_ladder() {
    // Feed and geometry ride; the hidden choices are the kind's own.
    let p = pocket(PocketStrategy::Zigzag, 60.0);
    let re = operation::retype_params(&p, OpType::Adaptive2D.ordinal());
    let OpParams::Adaptive2D { feed, stepover, entry, helix_radius, optimal_load, retract_z, .. } = &re else {
        panic!("the Adaptive2D row seeds an Adaptive2D");
    };
    assert_eq!((*feed, *stepover, *helix_radius), (800.0, 3.0, 1.0), "shared numbers ride");
    assert_eq!((*entry, *optimal_load, retract_z.is_none()), (Entry3D::Helix, camcraft_app::load::DEFAULT_OPTIMAL_LOAD, true), "the ladder and its door seed at the catalog default (M11.3: new pocket-family ops helix in) - a Zigzag pocket's 60 was never in play");
    // The Offset row has no engagement field to carry INTO: the
    // pocket's 60 is dropped, not hidden - checked by the seed
    // compiling against the variant that has no such field, and by
    // the digest below ignoring what the name pins out of the shape.
    assert!(matches!(operation::retype_params(&p, OpType::Offset2D.ordinal()), OpParams::Offset2D { feed: 800.0, stepover: 3.0, .. }));
    // And back: a twin retyped to Pocket lands on the pocket's
    // explicit-choice-free default (Auto), because no twin field
    // spells a strategy to carry.
    assert!(matches!(operation::retype_params(&adaptive(45.0), KindInfo::ALL.iter().position(|r| r.ty == OpType::Pocket).expect("pocket row")), OpParams::Pocket { strategy: PocketStrategy::Auto, .. }));
}

#[test]
fn registry_rows_hold_the_twins_in_the_mill2d_run() {
    // The row law: tag == index across the table, and the new names
    // sit inside the 2D mill's contiguous run (M6.2's ribbon law),
    // right after Trace.
    for (i, row) in KindInfo::ALL.iter().enumerate() {
        assert_eq!(row.tag as usize, i, "row {}: tag says {}", i, row.tag);
    }
    let (a, o) = (OpType::Adaptive2D.ordinal(), OpType::Offset2D.ordinal());
    assert_eq!((a, o), (5, 6), "the twins are the run's rows after Trace");
    assert_eq!(KindInfo::ALL[a].ty, OpType::Adaptive2D);
    assert_eq!(KindInfo::ALL[o].ty, OpType::Offset2D);
    assert_eq!(KindInfo::ALL[a].label, "2D Adaptive");
    assert_eq!(KindInfo::ALL[o].label, "2D Offset");
}

#[test]
fn the_twins_inherit_the_closed_chain_law() {
    // All three hollow a closed region, so none may end mid-edge
    // (selection.rs's law, now naming whichever kind was asked).
    use camcraft_app::geometry::feature;
    use camcraft_app::schema::Operation;
    for (name, p) in [("2D Adaptive", adaptive(45.0)), ("2D Offset", offset())] {
        // The box the pocket law was proved on (operations.rs's
        // fixture): at this tessellation a wall edge is its own open
        // run - the rim law's refusal by name, now with the twins'
        // name in the sentence.
        let world = new_world();
        let mut history = History::default();
        add_tool(&world, &mut history, "EM-0600-4FL");
        history.execute(
            &world,
            ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
        );
        let e = model::find_by_name(&world, "part").expect("part");
        let f = model::features_of(&world, &e).expect("features");
        let open = feature::chains(&f)
            .into_iter()
            .find(|c| !c.closed)
            .map(|c| FeatureSel::Chain(c.id))
            .expect("box chains are open runs");
        let op = Operation {
            tool: "EM-0600-4FL".into(),
            src: SrcRef::Selection(vec![GeoSel { entity: "part".to_owned(), feature: open }]),
            params: p,
            stamp: None,
            setup: None,
            area: Default::default(),
            coolant: Default::default(),
            after: Default::default(),
            suppressed: false,
        };
        let err = match operation::generate(&world, &op, "") {
            Ok((path, _)) => panic!("{name} generated from an open run: {} moves", path.moves.len()),
            Err(e) => e.to_string(),
        };
        assert!(err.contains("closed chain"), "{name}: {err}");
        assert!(err.contains("cannot end mid-edge"), "{name}: {err}");
        assert!(err.contains(name), "the refusal names the kind the user picked: {err}");
    }
}
