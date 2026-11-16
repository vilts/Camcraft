//! #540 (part of #290): the Wire Contour's no-core burnout. A 10 x 6
//! pocket in a die plate (#532's whole-model die: the model is the
//! opening) burns out from a start hole by offset rings and posts its
//! golden through wire-iso.cps on the synthetic fixture, judged by
//! #534's laws (the slug law reads that nothing drops); a clearance
//! below the kerf is refused at the command door; a pass that gouges a
//! wall blocks the post; the field rides the undo history and the file
//! (the MCP params door is the file mirror).
//!
//! Gate map (#540): covers app::operation (wire_op burnout branch,
//! wire_verify's burnout flag), app::posting (posting_wire), the
//! project mirror (wire_edm_dto), motion::wire_edm::burnout,
//! sim::verify::wire (the burnout slug law).
//! Provenance: our own tests over our own synthetic fixture; the
//! condition is SYNTHETIC (#290 D4, D10).

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef, WireContourOp, WireSide};
use camcraft_app::posting::{self, BUILTINS};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

const FIXTURE: &str = include_str!("edm/wire-iso-fixture.mch");
const GOLDEN: &str = "tests/golden/nc/wire-iso/wire_burnout_pocket.nc";

fn cps() -> &'static str {
    BUILTINS.iter().find(|(n, _)| *n == "Generic ISO Wire EDM").map(|(_, s)| *s).expect("registered")
}

/// The 10 x 6 pocket from z = 0 to 12, centred on the origin.
fn pocket() -> Mesh {
    let pts = [(5.0, 3.0), (-5.0, 3.0), (-5.0, -3.0), (5.0, -3.0)];
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

fn burnout(clearance: Option<f64>) -> WireContourOp {
    WireContourOp {
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side: WireSide::Die,
        start: Some([0.0, 0.0]),
        tabs: None,
        capture: false,
        burnout: clearance,
        taper_deg: 0.0,
        clearance: camcraft_app::operation::DEFAULT_CLEARANCE,
        four_axis: None,
        skims: Vec::new(),
    }
}

/// The fixture mounted, the pocket in a 30 x 30 plate `dz` tall.
fn doc(dz: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(FIXTURE).expect("the fixture parses");
    h.execute(&world, LoadMachine::new(&world, "wire-iso.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, ImportModel::from_mesh("pocket", pocket()));
    let stock = SetStock::new(&world, StockShape::Box { dx: 30.0, dy: 30.0, dz }, Location::default()).expect("stock");
    h.execute(&world, stock);
    (world, h)
}

fn burn(world: &World, h: &mut History, clearance: f64) {
    let op = OpParams::WireContour(burnout(Some(clearance)));
    let c = CreateOperation::new(world, "Burn1", "", SrcRef::Entity("pocket".into()), op).expect("op");
    h.execute(world, c);
    h.execute(world, GenerateOperation::new(world, "Burn1").expect("generates"));
}

fn params(world: &World) -> WireContourOp {
    let e = model::find_by_name(world, "Burn1").expect("op");
    match model::operation_of(world, &e).expect("op").params {
        OpParams::WireContour(w) => w,
        p => panic!("not a wire op: {p:?}"),
    }
}

/// Done means: the pocket burns out with nothing to drop - judged by
/// #534's laws at the gate - and posts its golden: threaded once at the
/// start hole, the rings in X Y only with no F, cut at the end.
#[test]
fn the_burnout_posts_its_golden_with_no_slug() {
    let (world, mut h) = doc(12.0);
    burn(&world, &mut h, 0.5);
    let clause = posting::wire_clause(&world);
    assert!(clause.problems.is_empty(), "{:?}", clause.problems);
    let line = clause.line.expect("judged, not skipped");
    assert!(line.contains("Burn1 frees nothing"), "{line}");
    let out = std::env::temp_dir().join(format!("camcraft-wire-burnout-{}.nc", std::process::id()));
    let posted = posting::post_script(&world, &out, cps()).expect("the burnout posts");
    let _ = std::fs::remove_file(&out);
    let text = posted.program;
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(GOLDEN, &text).expect("write golden");
    }
    assert_eq!(text, std::fs::read_to_string(GOLDEN).expect("golden present"), "the burnout drifted from its golden");
    let lines: Vec<&str> = text.lines().collect();
    let count = |w: &str| lines.iter().filter(|l| **l == w).count();
    assert_eq!((count("FX-THREAD"), count("FX-CUT")), (1, 1), "threaded once, cut once:\n{text}");
    let (th, cut) = (lines.iter().position(|l| *l == "FX-COMP-OFF").expect("comp off"), lines.iter().position(|l| *l == "FX-CUT").expect("cut"));
    let moves = &lines[th + 1..cut];
    assert!(moves.len() > 40 && moves.iter().all(|l| l.starts_with("G1 X") && !l.contains('Z') && !l.contains('F')), "{text}");
}

/// Done means: a clearance below the kerf is refused by name at the
/// command door; the field is undoable and rides the file (and so the
/// MCP params door, which patches the file mirror).
#[test]
fn a_clearance_below_the_kerf_is_refused_and_the_field_is_undoable() {
    let (world, mut h) = doc(12.0);
    let low = OpParams::WireContour(burnout(Some(0.1)));
    let err = CreateOperation::new(&world, "Low", "", SrcRef::Entity("pocket".into()), low).err().expect("refused");
    assert!(err.to_string().contains("wall clearance 0.1000 mm is below the kerf offset 0.1650 mm"), "{err}");
    let op = CreateOperation::new(&world, "Burn1", "", SrcRef::Entity("pocket".into()), OpParams::WireContour(burnout(None))).expect("op");
    h.execute(&world, op);
    let low = EditOperationParams::new(&world, "Burn1", OpParams::WireContour(burnout(Some(0.1))));
    assert!(low.is_err(), "the edit door refuses it too");
    h.execute(&world, EditOperationParams::new(&world, "Burn1", OpParams::WireContour(burnout(Some(0.4)))).expect("edit"));
    assert_eq!(params(&world).burnout, Some(0.4));
    let text = serde_json::to_string(&camcraft_app::project::collect(&world)).expect("json");
    assert!(text.contains("\"burnout_clearance\":0.4"), "the file carries it");
    let again = new_world();
    camcraft_app::project::open(&again, &text).expect("reopens");
    assert_eq!(params(&again).burnout, Some(0.4), "and reads it back");
    assert!(h.undo(&world));
    assert_eq!(params(&world).burnout, None, "undo restores the contour");
}

/// Done means: a pass that would gouge a wall blocks the post. The
/// plate is 20 mm but the pocket only 12: above it the burnout's rings
/// burn the plate itself, and no NC text is written.
#[test]
fn a_burnout_pass_that_gouges_a_wall_blocks_the_post() {
    let (world, mut h) = doc(20.0);
    burn(&world, &mut h, 0.5);
    let out = std::env::temp_dir().join(format!("camcraft-wire-burnout-gouge-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let err = posting::post_script(&world, &out, cps()).expect_err("a gouging burnout never posts");
    let wrote = out.exists();
    let _ = std::fs::remove_file(&out);
    assert!(err.starts_with("post blocked") && err.contains("Burn1: wire gouge: the kerf of move"), "{err}");
    assert!(!wrote, "no NC text is written");
}
