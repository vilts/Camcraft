//! #534 (part of #290 D7): the wire EDM laws at the post gate. A
//! generated Wire Contour is judged in 2.5D against the part and the
//! stock through the whole height the wire passes: a correct die and a
//! correct punch pass and the report names the piece each frees (the
//! die's waste slug, the punch itself); a kerf into the part and a
//! missing start hole block the post by name.
//!
//! Gate map (#534): covers app::posting (posting_wire), app::operation
//! (wire_verify), app::machine::wire_edm, sim::verify::wire.
//! Provenance: our own fixtures. The condition is SYNTHETIC - invented
//! numbers of a plausible size, from no machine's manual (#290 D4,
//! D10); the laws cite their sources in `sim::verify::wire`.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::operation::{Height, OpParams, SrcRef, WireContourOp, WireSide};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

const WIRE: &str = include_str!("edm/wire-edm.mch");
/// Synthetic: wire 0.25, gap 0.04 - a kerf offset of 0.165.
const TABLE: &str = r#"[{ "id": "C110", "name": "rough", "gap": 0.04, "wire_diameter": 0.25, "role": "rough" }]"#;

/// A 40 x 20 outline with 2 mm corners in 3-degree facets, CCW.
fn outline() -> Vec<(f64, f64)> {
    let (r, hx, hy) = (2.0, 18.0, 8.0);
    let mut pts = Vec::new();
    for (cx, cy, a0) in [(hx, hy, 0.0_f64), (-hx, hy, 90.0), (-hx, -hy, 180.0), (hx, -hy, 270.0)] {
        for s in 0..=30 {
            let t = (a0 + 3.0 * f64::from(s)).to_radians();
            pts.push((cx + r * t.cos(), cy + r * t.sin()));
        }
    }
    pts
}

/// The outline extruded from z = 0 to z = 12.
fn prism(pts: &[(f64, f64)]) -> Mesh {
    let (z0, z1) = (0.0, 12.0);
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let mut f = Vec::new();
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        f.push(Triangle::new(Point3::new(0.0, 0.0, z1), p(a, z1), p(b, z1)));
        f.push(Triangle::new(Point3::new(0.0, 0.0, z0), p(b, z0), p(a, z0)));
        f.push(Triangle::new(p(a, z0), p(b, z0), p(b, z1)));
        f.push(Triangle::new(p(a, z0), p(b, z1), p(a, z1)));
    }
    Mesh::from_facets(f)
}

/// A wire EDM document: the synthetic condition, the 40 x 20 prism and
/// a 60 x 60 block of stock `dz` tall around it.
fn doc(dz: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let mut v: serde_json::Value = serde_json::from_str(WIRE).expect("template json");
    v["edm"]["conditions"] = serde_json::from_str(TABLE).expect("conditions json");
    let m = parse_mch(&v.to_string()).expect("parses");
    h.execute(&world, LoadMachine::new(&world, "wire.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, ImportModel::from_mesh("part", prism(&outline())));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz }, Location::default()).expect("stock");
    h.execute(&world, stock);
    (world, h)
}

fn cut(world: &World, h: &mut History, name: &str, side: WireSide, start: Option<[f64; 2]>) {
    let op = WireContourOp { top_z: Height::fixed(12.0), bottom_z: Height::fixed(0.0), condition: "C110".into(), side, start, tabs: None, capture: true, burnout: None, taper_deg: 0.0, clearance: camcraft_app::operation::DEFAULT_CLEARANCE, four_axis: None, skims: Vec::new() };
    let c = CreateOperation::new(world, name, "", SrcRef::Entity("part".into()), OpParams::WireContour(op)).expect("op");
    h.execute(world, c);
    if start.is_some() {
        h.execute(world, GenerateOperation::new(world, name).expect("generates"));
    }
}

/// Done means: a clean die and a clean punch pass, and the slug is
/// identified for each - the die frees its waste, the punch frees itself.
#[test]
fn a_correct_die_and_a_correct_punch_pass_and_name_the_piece_they_free() {
    let (world, mut h) = doc(12.0);
    cut(&world, &mut h, "Die1", WireSide::Die, Some([0.0, 0.0]));
    cut(&world, &mut h, "Punch1", WireSide::Punch, Some([-25.0, 0.0]));
    let c = posting::wire_clause(&world);
    assert!(c.problems.is_empty(), "{:?}", c.problems);
    let line = c.line.expect("the judged line");
    assert!(line.starts_with(posting::WIRE_VERDICT), "{line}");
    let (die, punch) = line.split_once("; Punch1").expect("both judged");
    assert!(die.contains("Die1 frees") && die.ends_with("(waste) into the stated capture"), "the die's slug is waste: {line}");
    assert!(punch.contains("frees") && punch.ends_with("(the part) into the stated capture"), "the punch drops: {line}");
    let report = posting::verification_report(&world);
    assert!(report.problems.iter().all(|p| !p.contains("wire")), "{:?}", report.problems);
    assert!(report.not_judged.is_some_and(|n| n.contains(posting::WIRE_VERDICT)), "said, not silent");
}

/// Done means: a cut that enters the part blocks the post by name. The
/// die's model is its 12 mm opening, but the plate is 20 mm: above the
/// opening the vertical wire burns the plate itself.
#[test]
fn a_kerf_into_the_part_blocks_the_post_naming_the_move() {
    let (world, mut h) = doc(20.0);
    cut(&world, &mut h, "Die1", WireSide::Die, Some([0.0, 0.0]));
    let problems = posting::verification_gate(&world);
    let gouge = problems.iter().find(|p| p.contains("gouge")).expect("blocked");
    assert!(gouge.starts_with("Die1: wire gouge: the kerf of move 1 enters the part"), "{gouge}");
    assert!(problems.iter().any(|p| p.contains("Die1") && p.contains("not clear for threading")), "{problems:?}");
}

/// Done means: a missing start hole is refused by name at the gate.
#[test]
fn a_missing_start_hole_blocks_the_post() {
    let (world, mut h) = doc(12.0);
    cut(&world, &mut h, "Die1", WireSide::Die, None);
    let problems = posting::verification_gate(&world);
    assert!(problems.iter().any(|p| p.starts_with("Die1: wire no start hole is stated")), "{problems:?}");
}
