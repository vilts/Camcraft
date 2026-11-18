//! The Sodick wire post's tank and flushing words (#1499): a Wire Contour
//! on a machine whose file states submerged cutting and high pressure
//! flushing, posted by the app's own gate to its golden; the same punch
//! on a machine that states neither posts the #617 golden unchanged.
//! Provenance: the words are the 1998 Sodick "Wire EDM Machine
//! Operation" manual's (Mark 21 / EX21) T-code table - T84 high pressure
//! flushing, T94 submersed machining - in its Program Example 1 order
//! (T84, T94, then the C word). No other CAM product's post or output
//! was read. Not run on a Sodick control.
//!
//! Regenerate the golden after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test sodick_wire_dielectric`
//!
//! Gate map (#1499): covers app::posting (post_script), app::machine (the
//! edm tank / flushing keys through LoadMachine), the sodick-wire.cps script.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{parse_mch, Flushing, Machine, Tank};
use camcraft_app::operation::{self, Height, OpParams, SrcRef, WireContourOp, WireSide};
use camcraft_app::posting;
use camcraft_app::world::new_world;

const FIXTURE: &str = include_str!("edm/wire-iso-fixture.mch");
const SODICK: &str = "Sodick Wire EDM (Mark 21)";
const GOLDEN: &str = "tests/golden/nc/sodick-wire/wire_slug_punch_flushed.nc";
const PLAIN_GOLDEN: &str = "tests/golden/nc/sodick-wire/wire_slug_punch.nc";

fn cps() -> &'static str {
    camcraft_app::posting::builtin_post_text(SODICK).expect("registered")
}

/// A sharp 40 x 20 slug from z = 0 to 12, centred on the origin (the
/// #617 golden's part).
fn slug() -> Mesh {
    let pts = [(20.0, 10.0), (-20.0, 10.0), (-20.0, -10.0), (20.0, -10.0)];
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let mut f = Vec::new();
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        f.push(Triangle::new(Point3::new(0.0, 0.0, 12.0), p(a, 12.0), p(b, 12.0)));
        f.push(Triangle::new(Point3::new(0.0, 0.0, 0.0), p(b, 0.0), p(a, 0.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 0.0), p(b, 12.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 12.0), p(a, 12.0)));
    }
    Mesh::from_facets(f)
}

/// The fixture machine with the stated tank and flushing.
fn machine(tank: Option<Tank>, flushing: Option<Flushing>) -> Machine {
    let mut m = parse_mch(FIXTURE).expect("the fixture parses");
    if let Some(e) = m.edm.as_mut() {
        e.tank = tank;
        e.flushing = flushing;
    }
    m
}

/// The #617 punch, generated on `m`.
fn punch_doc(m: Machine) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::new(&world, "wire.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, AddTool::new(&world, "EDM-W025", camcraft_app::wire_tools::wire_tool(0.25)).expect("wire tool"));
    h.execute(&world, ImportModel::from_mesh("slug", slug()));
    let op = WireContourOp {
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side: WireSide::Punch,
        start: Some([-30.0, 0.0]),
        tabs: None,
        capture: true,
        taper_deg: 0.0,
        four_axis: None,
        burnout: None,
        clearance: operation::DEFAULT_CLEARANCE,
        skims: Vec::new(),
    };
    let c = CreateOperation::new(&world, "Punch1", "EDM-W025", SrcRef::Entity("slug".into()), OpParams::WireContour(op)).expect("op");
    h.execute(&world, c);
    h.execute(&world, GenerateOperation::new(&world, "Punch1").expect("generates"));
    world
}

fn post(world: &World, tag: &str) -> String {
    let out = std::env::temp_dir().join(format!("camcraft-sodick-dielectric-{tag}-{}.nc", std::process::id()));
    let posted = posting::post_script(world, &out, cps()).expect("the wire program posts");
    let _ = std::fs::remove_file(&out);
    posted.program
}

/// Done means: flushing and submerged cutting stated post the T words in
/// the manual's order to a new golden.
#[test]
fn a_submerged_high_flush_punch_posts_its_golden_through_the_gate() {
    let text = post(&punch_doc(machine(Some(Tank::Submerged), Some(Flushing::High))), "stated");
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(GOLDEN, &text).expect("write golden");
    }
    let want = std::fs::read_to_string(GOLDEN).expect("golden present");
    assert_eq!(text, want, "the Sodick flushed program drifted from its golden");

    // The golden against the #617 one: the header's tank / flushing line
    // becomes the two programmed words' lines, and T84 T94 stand between
    // T91 (wire threaded) and the C word - nothing else moves.
    let plain = std::fs::read_to_string(PLAIN_GOLDEN).expect("the #617 golden");
    let mut expect: Vec<&str> = Vec::new();
    for l in plain.lines() {
        match l {
            "(TANK FILL AND FLUSHING ARE NOT PROGRAMMED - SET THEM AT THE CONTROL)" => {
                expect.push("(T94 SUBMERGED MACHINING AFTER THREADING - NO FILL OR DRAIN WORD IS WRITTEN - CHECK THE TANK IS FULL)");
                expect.push("(T84 HIGH-PRESSURE FLUSHING AFTER THREADING)");
            }
            "T91" => expect.extend(["T91", "T84", "T94"]),
            other => expect.push(other),
        }
    }
    assert_eq!(text.lines().collect::<Vec<_>>(), expect, "{text}");
}

/// Done means: a machine that states nothing posts as today.
#[test]
fn a_machine_stating_neither_posts_the_617_golden_unchanged() {
    let text = post(&punch_doc(machine(None, None)), "plain");
    let want = std::fs::read_to_string(PLAIN_GOLDEN).expect("the #617 golden");
    assert_eq!(text, want);
    for w in ["T84", "T85", "T89", "T94"] {
        assert!(!text.lines().any(|l| l == w), "{w}\n{text}");
    }
}

/// The keys survive the project file: a saved machine reads them back.
#[test]
fn the_tank_and_flushing_round_trip_through_the_machine_json() {
    let m = machine(Some(Tank::NonSubmerged), Some(Flushing::Low));
    let v = serde_json::to_value(&m).expect("serializes");
    assert_eq!(v["edm"]["tank"], serde_json::json!("non-submerged"));
    assert_eq!(v["edm"]["flushing"], serde_json::json!("low"));
    let back: Machine = serde_json::from_value(v).expect("reads back");
    assert_eq!(back, m);
    let stated = FIXTURE.replace("\"awt\": true,", "\"awt\": true, \"tank\": \"submerged\", \"flushing\": \"high\",");
    let parsed = parse_mch(&stated).expect("a stated .mch parses");
    assert_eq!(parsed.edm.as_ref().map(|e| (e.tank, e.flushing)), Some((Some(Tank::Submerged), Some(Flushing::High))));
    let garbled = FIXTURE.replace("\"awt\": true,", "\"awt\": true, \"flushing\": \"medium\",");
    let err = parse_mch(&garbled).err().unwrap_or_default();
    assert!(err.contains("edm.flushing must be one of high, low, not \"medium\""), "{err}");
}
