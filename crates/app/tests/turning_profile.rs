//! M5.2 stage B turning tests at the app level: the profile kinds'
//! whole life through the document - rough and finish rides of the
//! turned silhouette, wired the facing way (family door by the
//! registry, tool door by name, heights as stations, the envelope
//! verdict as judge). What "right" means here: a finish ride over
//! the bar's neck generates clean, verifies through the same sphere
//! envelope the mill kinds stand before, and carries the kernel's
//! signature - ONE radial plunge at the window's bottom, feeds
//! whose stations ascend, a slide-out and a home rapid; the rough
//! ladder says it once per rung, rungs outermost first, and the
//! wall it leaves (stock to leave) is what the verdict lets stand.
//! The kernel's own laws are pinned one crate down (camcraft-motion's
//! turning_prof suite); this file is the wiring.
//!
//! Gate map (M4.4): covers camcraft_app::operation (the
//! ProfileRough/ProfileFinish arms of generate/heights/material/
//! params-digest, the two seeds), the family door through the
//! registry, the tool door at generation.
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
use camcraft_app::schema::Location;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
/// The house insert's nose (TN-1600-0R8) - the finish ride stands
/// this far off the skin by construction.
const NOSE: f64 = 0.8;

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

/// Bar-sized stock (#130): both fixture bars' own r 6 over their Z
/// span - the kernels rough from the stock, so a 40 mm box would turn
/// these part-based ladder counts into a 28 mm-radius roughing job.
fn bar_stock(world: &World, history: &mut History) {
    let cmd = SetStock::new(
        world,
        camcraft_app::stock::StockShape::Cylinder { diameter: 12.0, height: 32.0 },
        Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() },
    );
    history.execute(world, cmd.expect("bar stock"));
}

fn status(world: &World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("entity exists");
    camcraft_app::operation::status_of(world, &e)
}

fn cache(world: &World, name: &str) -> Option<camcraft_app::schema::Toolpath> {
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e))
}

/// Revolve a meridian polyline about Z - the turning suites' one
/// fixture law (the fit's coarseness note, stage A's idiom).
fn revolve(meridian: &[(f64, f64)]) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
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

/// The stage-A bar: wall r6 to z 2, neck r5 to the top, z -16..16.
fn bar_mesh() -> Mesh {
    revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)])
}

/// The bar with a deeper shoulder - neck r3.5 - so the rough ladder
/// has a shell worth rungs: e_max 2.5 against the nose's reach, two
/// rungs at stepdown 0.8. The flats-to-wall area ratio still stands
/// under the axis fit's domed-ground gate (the stage-B fixture law,
/// recorded in RECOMMENDED.md).
fn flanged_mesh() -> Mesh {
    revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (3.5, 2.0), (3.5, 16.0), (0.0, 16.0)])
}

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

fn bar_world_with(mesh: Mesh) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    bar_stock(&world, &mut history);
    add_tool(&world, &mut history, "TN-1600-0R8");
    history.execute(&world, ImportModel::from_mesh("bar", mesh));
    (world, history)
}

fn finish_params(bottom: f64, top: f64) -> OpParams {
    OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(top),
        bottom_z: Height::fixed(bottom),
        feed: 120.0,
        plunge_feed: 60.0,
        picks: Default::default(),
    }
}

fn rough_params(bottom: f64, top: f64, stepdown: f64, leave: f64) -> OpParams {
    OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(top),
        bottom_z: Height::fixed(bottom),
        stepdown,
        stock_to_leave: leave,
        feed: 150.0,
        plunge_feed: 80.0,
        picks: Default::default(),
    }
}

/// The ride's own skyline: the largest radius a FEED ends at. A
/// radial LEAD starts above it, at the clearance radius; every
/// other same-station move is the ride stepping over a shoulder's
/// shadow, which is lawful - the classifier the motion suite
/// argues with, ported to read world points.
fn skyline(path: &camcraft_app::cl::Toolpath) -> f64 {
    use camcraft_app::cl::MoveKind;
    let rho = |q: Point3| (q.x * q.x + q.y * q.y).sqrt();
    path.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed)
        .fold(0.0f64, |acc, m| acc.max(rho(m.to)))
}

/// (plunges, descending-stations-ok, min/max feed radius) read
/// through the world - the rides' signature: a plunge is a FEED at
/// one station starting above the skyline; everything else must
/// GAIN station.
fn ride_signature(path: &camcraft_app::cl::Toolpath) -> (usize, bool, f64, f64) {
    use camcraft_app::cl::MoveKind;
    let rho = |q: Point3| (q.x * q.x + q.y * q.y).sqrt();
    let sky = skyline(path);
    let mut plunges = 0;
    let mut ascending = true;
    let (mut r_min, mut r_max) = (f64::INFINITY, 0.0f64);
    for m in &path.moves {
        if m.kind != MoveKind::Feed {
            continue;
        }
        r_min = r_min.min(rho(m.to));
        r_max = r_max.max(rho(m.from).max(rho(m.to)));
        let plunge = (m.to.z - m.from.z).abs() <= 1e-6 && rho(m.from) > sky + 1e-3;
        if plunge {
            plunges += 1;
        } else if m.to.z < m.from.z - 1e-6 {
            ascending = false;
        }
    }
    (plunges, ascending, r_min, r_max)
}

#[test]
fn a_profile_finish_ride_generates_and_verifies_clean() {
    // The neck window of the stage-A bar: one ride, bottom-up, and
    // the envelope verdict - the same judge facing stands before -
    // comes back Ok over the turned skin.
    let (world, mut history) = bar_world_with(bar_mesh());
    mount(&world, &mut history, "lathe");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "PF1",
            "TN-1600-0R8",
            SrcRef::Entity("bar".into()),
            finish_params(2.0, 16.0),
        )
        .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "PF1").expect("generate"));
    assert_eq!(status(&world, "PF1"), OpStatus::Ok, "the ride's envelope verdict came back inside the gate");
    let t = cache(&world, "PF1").expect("cached").path;
    let (plunges, ascending, r_min, r_max) = ride_signature(&t);
    assert_eq!(plunges, 1, "one radial plunge in at the window's bottom");
    assert!(ascending, "feeds never run the station down");
    // The neck wall is r5: the ride's center stands nose off the
    // axis there, and the step's circle claim at the window's foot
    // is the max it ever reaches - the plunge coming in from a
    // tolerance above that, nowhere further.
    assert!(r_min >= 5.0 + NOSE - 1e-6, "a ride point inside the nose's budget: {r_min}");
    assert!(r_max <= 6.0 + NOSE + 0.05 + 1e-6, "a ride point past the shoulder's reach: {r_max}");
    let last = t.moves.last().expect("a path has an end");
    assert!(last.kind == camcraft_app::cl::MoveKind::Rapid && last.to.z >= 25.0 - 1e-6, "the program ends home at the rapids plane: {last:?}");
}

#[test]
fn a_profile_ladder_runs_its_rungs_outermost_first() {
    // The flanged bar's window is the whole body: two rungs at
    // stepdown 0.8 against e_max 2.5 (the kernel's count, echoed by
    // the path), the outer one eating first - so the rungs' radius
    // maxima march DOWN the ladder, and the wall the last rung
    // stands off is the leave the verdict lets pass.
    let (world, mut history) = bar_world_with(flanged_mesh());
    mount(&world, &mut history, "lathe");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "PR1",
            "TN-1600-0R8",
            SrcRef::Entity("bar".into()),
            rough_params(-16.0, 16.0, 0.8, 0.25),
        )
        .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "PR1").expect("generate"));
    assert_eq!(status(&world, "PR1"), OpStatus::Ok, "the ladder's envelope verdict came back inside the gate");
    let t = cache(&world, "PR1").expect("cached").path;
    use camcraft_app::cl::MoveKind;
    let rho = |q: Point3| (q.x * q.x + q.y * q.y).sqrt();
    let sky = skyline(&t);
    let mut rung_peaks: Vec<f64> = Vec::new();
    for m in &t.moves {
        if m.kind != MoveKind::Feed {
            continue;
        }
        if (m.to.z - m.from.z).abs() <= 1e-6 && rho(m.from) > sky + 1e-3 {
            rung_peaks.push(0.0); // a plunge from the clear radius: a rung begins
        }
        let p = rung_peaks.last_mut().expect("a rung begins before its first cut");
        *p = (*p).max(rho(m.to)); // the ride's own reach, not the lead above it
    }
    assert_eq!(rung_peaks.len(), 2, "e_max 2.5 past reach 1.85 at stepdown 0.8 is two rungs");
    // The outer rung's skyline is the wall plus its distance - the
    // deepest d the ladder rides at; the ladder then steps in by
    // exactly the stepdown.
    // The ride never crosses the bottom's flat - that is facing's
    // ground - so the deepest cut anywhere in the ladder is the
    // last rung's leave wall off the neck: 3.5 + nose + leave.
    let (_, _, r_min, _) = ride_signature(&t);
    assert!(r_min >= 3.5 + NOSE + 0.25 - 1e-3, "a rung bit past its leave wall: {r_min}");
    assert!(
        (rung_peaks[0] - (6.0 + NOSE + 0.25 + 0.8)).abs() < 0.05,
        "the outer rung rides at nose + leave + stepdown off the wall: {}",
        rung_peaks[0]
    );
    assert!(
        (rung_peaks[0] - rung_peaks[1] - 0.8).abs() < 0.05,
        "the rungs step in by the stepdown, once each: {rung_peaks:?}"
    );
}

#[test]
fn the_profile_kinds_refuse_a_mill_tool_by_name() {
    // Same tool door facing states, in their own words.
    for (tag, params) in [
        (19, rough_params(-16.0, 16.0, 0.8, 0.25)),
        (20, finish_params(2.0, 16.0)),
    ] {
        let (world, mut history) = bar_world_with(bar_mesh());
        mount(&world, &mut history, "lathe");
        add_tool(&world, &mut history, "EM-0400-2FL");
        let name = format!("X{tag}");
        common::create_past_the_tool_law(&world, &mut history, &name, "EM-0400-2FL", SrcRef::Entity("bar".into()), params);
        let Err(err) = common::generate_past_the_tool_law(&world, &name) else {
            panic!("a flat nose cannot ride a profile");
        };
        let msg = err.to_string();
        assert!(msg.contains("turning insert") && msg.contains("EM-0400-2FL"), "{msg}");
        assert!(msg.contains("Profile"), "{msg}");
    }
}

#[test]
fn a_window_past_the_bar_refuses_by_name() {
    // Heights the bar does not stand in: the kernel's Empty refusal
    // rides the document as a generation error, never a silent
    // empty cache.
    let (world, mut history) = bar_world_with(bar_mesh());
    mount(&world, &mut history, "lathe");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "PF1",
            "TN-1600-0R8",
            SrcRef::Entity("bar".into()),
            finish_params(40.0, 60.0),
        )
        .expect("create"),
    );
    let Err(err) = GenerateOperation::new(&world, "PF1") else {
        panic!("a window in midair must refuse");
    };
    assert!(err.to_string().contains("nothing to turn"), "{err}");
}

#[test]
fn a_stepdown_past_the_nose_refuses_by_name() {
    // The ridge law read radially: 1.7 is past twice the house
    // nose's 0.8, and the words say so at the door.
    let (world, mut history) = bar_world_with(flanged_mesh());
    mount(&world, &mut history, "lathe");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "PR1",
            "TN-1600-0R8",
            SrcRef::Entity("bar".into()),
            rough_params(-16.0, 16.0, 1.7, 0.25),
        )
        .expect("create"),
    );
    let Err(err) = GenerateOperation::new(&world, "PR1") else {
        panic!("a sheet no rung touches must refuse, not stand");
    };
    assert!(err.to_string().contains("stepdown"), "{err}");
}

#[test]
fn the_profile_seeds_stand_on_the_bar() {
    // The registry's own numbers, end to end, the stage-A law: pick
    // the kind on a lathe document, Generate, and the seed's
    // stepdown clears the nose law beside the catalog insert.
    let (world, mut history) = bar_world_with(flanged_mesh());
    mount(&world, &mut history, "lathe");
    let c = Carried {
        feed: 150.0,
        plunge: 80.0,
        climb: true,
        clearance: Height::fixed(25.0),
        stepover: 3.0,
        stepdown: 5.0, // the mill's fold: both seeds must NOT carry it
        top: Height::fixed(16.0),
        bottom: Height::fixed(-16.0),
        retract: Height::fixed(20.0),
        tilt: None,
    };
    for ordinal in [OpType::ProfileRough as usize, OpType::ProfileFinish as usize] {
        let name = format!("S{ordinal}");
        let params = (KindInfo::ALL[ordinal].seed)(&c);
        history.execute(
            &world,
            CreateOperation::new(&world, &name, "TN-1600-0R8", SrcRef::Entity("bar".into()), params)
                .expect("create from the seed"),
        );
        history.execute(&world, GenerateOperation::new(&world, &name).expect("generate"));
        assert_eq!(status(&world, &name), OpStatus::Ok, "the seed's own numbers stand (kind {ordinal})");
    }
}
