//! The cross drill's holder on box stock (issue #119): on one station
//! the holder is proven clear of the stock it really sweeps - the tube
//! about its hole line, so a box counts at its face - not of the box's
//! circumscribed (corner) circle. A holder that clears the face but not
//! the corner circle generates and posts; one that meets material in
//! its footprint is still refused by name. (The swing round the bar
//! keeps the corner radius: drill_reach's unit tests pin it.)
//!
//! Gate map (M4.4): covers app::operation (drill_reach's cross_holder,
//! drill_station, cross_entry's tube_reach, cross_spindle).
//! Lane shape (M4.4): all tests here run in seconds.
//!
//! Provenance: our own fixtures (a revolved r10 bar, a 60 box of raw
//! stock); no vendor material.

mod punched_bar;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::drilling::{CrossHole, CycleChoice};
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, cube};
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting};

const PART_R: f64 = 10.0;

/// The +X hole 3 mm into the r10 bar at z0: on the 60 box its mouth
/// moves out to the r30 face and its floor stays at x7 - 23 mm deep.
fn hole() -> CrossHole {
    CrossHole {
        origin: Point3::new(PART_R, 0.0, 0.0),
        dir: Point3::new(-1.0, 0.0, 0.0),
        radius: 2.0,
        entry_d: 0.0,
        floor_d: -3.0,
        through: false,
    }
}

fn op() -> Operation {
    Operation {
        tool: "DR-0400".into(),
        src: SrcRef::Entity("bar".into()),
        params: OpParams::Drill {
            break_through: None,
            break_through_from: Default::default(),
            clearance_z: Height::fixed(25.0),
            retract_z: None,
            top_z: Height::fixed(20.0),
            bottom_z: None,
            cycle: CycleChoice::Auto,
            peck: 0.0,
            dwell: 0.0,
            feed: 120.0,
            plunge_feed: 80.0,
            holes: vec![],
            cross: vec![hole()],
            picks: Default::default(),
        },
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

/// `part` on `word`'s machine in a 60 x 60 box of stock (z -20..20),
/// DR-0400 on a 24 mm flute and a 6 mm shank: its Ø44 holder's face
/// stands 30 mm behind the tip - at x37 at the floor, outside the r30
/// face, inside the r42.43 corner circle.
fn world(word: &str, part: Mesh) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -20.0), rot: IDENTITY_Q };
    let s = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 40.0 }, at);
    history.execute(&world, s.expect("stock"));
    let r = |name: &str, axis: [f64; 3]| AxisLimits { name: name.into(), rotary: true, axis: Some(axis), ..Default::default() };
    let axes = vec![r("A", [1.0, 0.0, 0.0]), r("B", [0.0, 1.0, 0.0]), r("C", [0.0, 0.0, 1.0])];
    let m = Machine { model: format!("TestBench {word}"), kind: Some(word.into()), axes, ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    let mut tool = catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t).expect("catalog entry");
    (tool.flute_len, tool.shank_len) = (24.0, 6.0);
    assert_eq!(tool.holder_d, 44.0, "the catalog holder this suite measures");
    history.execute(&world, AddTool::new(&world, "DR-0400", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("bar", part));
    (world, history)
}

fn drilled() -> Mesh {
    punched_bar::punched_bar(PART_R, punched_bar::FOOT, punched_bar::TOP, &[hole()])
}

/// Before #119 the holder was read against the box's corner circle in
/// its Z band: x37 inside r42.43, refused. It clears the r30 face along
/// its own line: the op generates fresh and posts.
#[test]
fn a_holder_clearing_the_box_face_but_not_its_corner_circle_generates_and_posts() {
    for word in ["lathe", "mill-turn"] {
        let (world, mut history) = world(word, drilled());
        let o = op();
        let r = operation::generate(&world, &o, "");
        assert!(r.is_ok(), "{word}: the holder at x37 clears the r30 face: {:?}", r.err());
        let create = CreateOperation::new(&world, "CD1", "DR-0400", o.src.clone(), o.params.clone());
        history.execute(&world, create.expect("create"));
        history.execute(&world, GenerateOperation::new(&world, "CD1").expect("generate"));
        let op_e = model::find_by_name(&world, "CD1").expect("op");
        assert_eq!(operation::status_of(&world, &op_e), OpStatus::Ok, "{word}: generates fresh");
        if word == "mill-turn" {
            let path = std::env::temp_dir().join(format!("camcraft-119-{}.nc", std::process::id()));
            let posted = posting::post_setup(&world, &path);
            let _ = std::fs::remove_file(&path);
            assert!(posted.is_ok(), "{word}: the box cross drill posts: {:?}", posted.err());
        }
    }
}

/// Material really in the holder's footprint is still refused by name:
/// a block standing out of the box face to x40 at y 6..12 (outside the
/// drill's and shank's band, inside the Ø44 holder's) - the holder's
/// face at x37 would sink into it. Mill-turn only: a lathe refuses the
/// lumpy part as not turned about one axis.
#[test]
fn a_holder_meeting_material_in_its_footprint_on_box_stock_is_refused_by_name() {
    let mut f = drilled().facets.to_vec();
    f.extend(cube(Point3::new(8.0, 6.0, -3.0), Point3::new(40.0, 12.0, 3.0)).facets.iter().copied());
    let (world, _h) = world("mill-turn", Mesh::from_facets(f));
    let e = operation::generate(&world, &op(), "").err().map(|e| e.to_string());
    let e = e.unwrap_or_else(|| panic!("the holder sank into the block at x40"));
    for s in ["DR-0400's holder", "dia. 44.000", "30.000 mm behind the tip", "37.000 mm out along its hole's line"] {
        assert!(e.contains(s), "the refusal names '{s}': {e}");
    }
    assert!(e.contains("inside material 40.000 mm out on that line"), "read at the block, not a corner: {e}");
}
