//! M5.2 turning tests at the app level: the family door and the
//! turned kinds' whole life through the document - stage A's facing
//! ladder (stage B's profile rides are in turning_profile.rs). What
//! "right" means here: a turning op on a mill document is REFUSED by
//! name at the command door (not generated then warned about), a
//! mounted lathe opens the turning door and shuts the mill one, the
//! refusal text names both sides, undo puts the document back, and a
//! Facing over a turned bar generates, verifies through the same
//! envelope the mill kinds stand behind, and leaves the ladder's
//! signature in the cached path: passes that END on the axis, START
//! outside the skin, and never sink below the facing plane's
//! tangency. The kernel's own laws are pinned one crate down
//! (camcraft-motion's turning suite); this file is the wiring.
//!
//! Gate map (M4.4): covers camcraft_app::commands (family gate,
//! CreateOperation/GenerateOperation), camcraft_app::machine, post::machine (kind
//! read), camcraft_app::operation (the Facing arm of
//! generate/heights/material, the seed), camcraft_app::posting-free
//! generation path.
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{Carried, Height, KindInfo, OpParams, OpStatus, OpType, SrcRef};
use camcraft_app::schema::{Location, Toolpath};
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;

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

fn box_stock(world: &World, history: &mut History, d: f64) {
    let cmd = SetStock::new(
        world,
        camcraft_app::stock::StockShape::Box { dx: d, dy: d, dz: d },
        Location::default(),
    );
    history.execute(world, cmd.expect("box stock"));
}

fn status(world: &World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("entity exists");
    camcraft_app::operation::status_of(world, &e)
}

fn cache(world: &World, name: &str) -> Option<Toolpath> {
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e))
}

/// The turned bar, app-sized: wall r6 to z 2, neck r5 to the top,
/// capped both ends, z -16..16 - 16 turns, an honest revolve (the
/// fit's coarseness law, same as the rotary suites').
fn bar_mesh() -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in [(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)]
        .windows(2)
    {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(facets)
}

/// A declared machine by word - the stated-kind path of the `.mch`
/// grammar; the axis-inventory law below says the same thing and is
/// pinned in the machine suite.
fn machine(word: &str) -> Machine {
    Machine {
        model: format!("TestBench {word}"),
        kind: Some(word.to_string()),
        ..Default::default()
    }
}

fn mount(world: &World, history: &mut History, word: &str) {
    let cmd = LoadMachine::new(world, format!("{word}.mch"), Some(machine(word)));
    history.execute(world, cmd.expect("the declared kind is sane"));
}

fn facing_params(facing_z: f64) -> OpParams {
    OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(facing_z),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
    }
}

/// A document the turning kinds live in: bar in plain box stock, the
/// house insert on the rack. The lathe mount is the caller's move -
/// the door is what most of these tests are ABOUT.
fn bar_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 40.0);
    add_tool(&world, &mut history, "TN-1600-0R8");
    history.execute(&world, ImportModel::from_mesh("bar", bar_mesh()));
    (world, history)
}

#[test]
fn the_family_door_names_both_sides() {
    // No machine mounted is a mill document (the kind_of law): a
    // turning op is refused BEFORE the entity exists, and the words
    // say what is turning work and what to mount. The ribbon's
    // darkening is a courtesy; this door is the law.
    let (world, mut history) = bar_world();
    let Err(err) =
        CreateOperation::new(&world, "F1", "TN-1600-0R8", SrcRef::Entity("bar".into()), facing_params(14.0))
    else {
        panic!("a turning op on a mill document must refuse");
    };
    let msg = err.to_string();
    assert!(msg.contains("turning") && msg.contains("lathe"), "{msg}");
    assert!(msg.contains("Facing"), "{msg}");
    assert!(model::find_by_name(&world, "F1").is_none(), "a refusal creates nothing");
    // The lathe mount opens it - through the command, so the history
    // owns the change and undo shuts the door again.
    mount(&world, &mut history, "lathe");
    assert!(
        CreateOperation::new(&world, "F1", "TN-1600-0R8", SrcRef::Entity("bar".into()), facing_params(14.0))
            .is_ok(),
        "the mounted lathe admits its own family"
    );
    assert!(history.undo(&world), "the create rides the history");
    assert!(history.undo(&world), "and so does the mount");
    assert_eq!(camcraft_app::machine::kind_of(&world), camcraft_app::machine::Kind::Mill);
    assert!(
        CreateOperation::new(&world, "F2", "TN-1600-0R8", SrcRef::Entity("bar".into()), facing_params(14.0))
            .is_err(),
        "undoing the mount shuts the turning door again"
    );
}

#[test]
fn a_lathe_shuts_the_mill_door_and_a_millturn_opens_both() {
    let (world, mut history) = bar_world();
    // The same table reads the other way: a lathe document is no
    // place for a 2D Contour, and the refusal says so by the kind's
    // own name.
    mount(&world, &mut history, "lathe");
    add_tool(&world, &mut history, "EM-0400-2FL");
    let contour = OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: camcraft_app::machining2d::Side::Auto,
        lead: camcraft_app::machining2d::Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    };
    let Err(err) = CreateOperation::new(&world, "C1", "EM-0400-2FL", SrcRef::Stock, contour.clone())
    else {
        panic!("a mill op on a lathe document must refuse");
    };
    let msg = err.to_string();
    assert!(msg.contains("milling") && msg.contains("Contour"), "{msg}");
    assert!(msg.contains("lathe"), "{msg}");
    // A mill-turn is the one row that admits both families - the
    // door opens on the table, not on a hand-typed memory.
    mount(&world, &mut history, "mill-turn");
    assert!(
        CreateOperation::new(&world, "C2", "EM-0400-2FL", SrcRef::Stock, contour).is_ok(),
        "a mill-turn mills"
    );
    assert!(
        CreateOperation::new(
            &world,
            "F1",
            "TN-1600-0R8",
            SrcRef::Entity("bar".into()),
            facing_params(14.0)
        )
        .is_ok(),
        "and turns"
    );
}

#[test]
fn a_facing_pass_set_generates_and_verifies_clean() {
    // The whole life: mount, create, generate - and the envelope
    // verdict, the same judge every M4.4 kind stands before, comes
    // back Ok over the turned bar. The cached path carries the
    // kernel's signature read through the world: five stations
    // 14.0..16.0, each pass ending on the axis and starting beyond
    // the skin it crossed (the wall at this band is r 5, so beyond
    // 5.8), and no CL point below the plane's tangency (14 + the
    // 0.8 nose) or above the rapids plane's.
    let (world, mut history) = bar_world();
    mount(&world, &mut history, "lathe");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "F1",
            "TN-1600-0R8",
            SrcRef::Entity("bar".into()),
            facing_params(14.0),
        )
        .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "F1").expect("generate"));
    assert_eq!(status(&world, "F1"), OpStatus::Ok, "the envelope verdict came back inside the gate");
    let t = cache(&world, "F1").expect("cached").path;
    let rho = |q: Point3| (q.x * q.x + q.y * q.y).sqrt();
    let mut ends = 0usize;
    for m in &t.moves {
        if m.kind == camcraft_app::cl::MoveKind::Feed && rho(m.to) < 1e-9 {
            ends += 1;
            assert!(rho(m.from) >= 5.8 - 1e-9, "a pass that started inside the skin: {:?}", m.from);
        }
        for q in [m.from, m.to] {
            assert!(q.z >= 14.0 + 0.8 - 1e-9 && q.z <= 25.0 + 0.8 + 1e-9, "a CL point off the ladder: {q:?}");
        }
    }
    assert_eq!(ends, 5, "the 14..16 ladder is five stations of infeed at 0.5");
    let last = t.moves.last().expect("a path has an end");
    assert!(last.kind == camcraft_app::cl::MoveKind::Rapid && last.to.z > 25.0, "the program ends home above the rapids plane: {last:?}");
}

#[test]
fn the_facing_seed_faces_the_bar() {
    // The registry's own numbers, end to end: the seed's stepdown
    // must clear the nose law against the catalog insert it ships
    // beside (0.5 under 2 x 0.8), so a user who picks the kind,
    // mounts a lathe and presses Generate gets a path, not a
    // refusal - the one number the mill fold would have gotten
    // wrong.
    let (world, mut history) = bar_world();
    mount(&world, &mut history, "lathe");
    let c = Carried {
        feed: 150.0,
        plunge: 80.0,
        climb: true,
        clearance: Height::fixed(25.0),
        stepover: 3.0,
        stepdown: 5.0, // the mill's fold: the seed must NOT carry it
        top: Height::fixed(16.0),
        bottom: Height::fixed(14.0),
        retract: Height::fixed(20.0),
        tilt: None,
    };
    let params = (KindInfo::ALL[OpType::Facing as usize].seed)(&c);
    history.execute(
        &world,
        CreateOperation::new(&world, "S1", "TN-1600-0R8", SrcRef::Entity("bar".into()), params)
            .expect("create from the seed"),
    );
    history.execute(&world, GenerateOperation::new(&world, "S1").expect("generate"));
    assert_eq!(status(&world, "S1"), OpStatus::Ok, "the seed's own numbers stand");
}

#[test]
fn facing_refuses_a_mill_tool_by_name() {
    // The family door reads the KIND; the tool's side of the pair is
    // the generator's, and it speaks too: a Facing on an end mill
    // names the tool and what it is, instead of cutting the disk
    // with a cylinder's flat nose.
    let (world, mut history) = bar_world();
    mount(&world, &mut history, "lathe");
    add_tool(&world, &mut history, "EM-0400-2FL");
    common::create_past_the_tool_law(&world, &mut history, "F1", "EM-0400-2FL", SrcRef::Entity("bar".into()), facing_params(14.0));
    let Err(err) = common::generate_past_the_tool_law(&world, "F1") else {
        panic!("a flat nose cannot face a lathe");
    };
    let msg = err.to_string();
    assert!(msg.contains("turning insert") && msg.contains("EM-0400-2FL"), "{msg}");
}
