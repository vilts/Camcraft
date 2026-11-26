//! #2720: every operation kind that accepts a tilt (3+2 index) refuses a
//! gouge at generation, and the post is blocked by name. The stock sim
//! does not judge posed moves (`sim::stock_sim::POSED_REFUSAL`), so under
//! a tilt the operation's own verdict is the only guard (#2600). Each
//! kind below cuts into a correct box part upright, at A 90 and at A -90,
//! on the 4th-axis mill; each case is its own world, so one op's verdict
//! cannot hide another's. Slot has its own suite (`tilted_gouge`).
//!
//! Provenance: our own fixtures (a box part in a box stock) over the
//! depth laws of `operation::generate` (`depth_law_of`), the keep law and
//! the stock-source law; no vendor material consulted.
//!
//! Gate map: covers app::operation (generate's tilted arm, depth_law_of,
//! keep_law, mark_law), app::posting.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{cube, Point3, IDENTITY_Q};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{seed_params, AreaControls, Height, OpParams, OpType, SrcRef, Tilt};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;

/// The stock: [0, 40] x [-25, 25] x [-25, 25], so its skin stands 25 mm
/// from the X axis upright and at A +-90.
const SKIN: f64 = 25.0;
/// The part: [5, 35] x [-15, 15] x [-15, 15]; its skin stands 15 mm from
/// the X axis in the three poses.
const PART: f64 = 15.0;
const MILL: &str = "EM-0600-4FL";
const CHAMFER_MILL: &str = "CM-1000-90";

/// The three poses: upright (the guard), and the two sides of the bar.
const POSES: [(&str, Option<f64>); 3] = [("upright", None), ("a90", Some(90.0)), ("a-90", Some(-90.0))];

fn tilt(a: Option<f64>) -> Option<Tilt> {
    a.map(|a_deg| Tilt { basis_version: 1, a_deg, b_deg: 0.0, origin: [0.0, 0.0, 0.0] })
}

/// The part in the stock on the 4th-axis mill, with `tool`. `flush`: the
/// part fills the stock (for the 2D Chamfer, which breaks the stock's
/// edge). `neighbour`: a second part, [37, 40] x [-15, 15] x [-15, 15].
fn job(tool: &str, flush: bool, neighbour: bool) -> (World, History) {
    let world = new_world_with_machine();
    let mut h = History::default();
    let (name, t) = catalog().into_iter().find(|(n, _)| *n == tool).expect("catalog entry");
    h.execute(&world, AddTool::new(&world, name, t).expect("tool"));
    let (lo, hi) = if flush {
        (Point3::new(0.0, -SKIN, -SKIN), Point3::new(40.0, SKIN, SKIN))
    } else {
        (Point3::new(5.0, -PART, -PART), Point3::new(35.0, PART, PART))
    };
    h.execute(&world, ImportModel::from_mesh("part", cube(lo, hi)));
    if neighbour {
        h.execute(&world, ImportModel::from_mesh("neighbour", cube(Point3::new(37.0, -PART, -PART), Point3::new(40.0, PART, PART))));
    }
    let loc = Location { pos: Point3::new(20.0, 0.0, -SKIN), rot: IDENTITY_Q };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 2.0 * SKIN, dz: 2.0 * SKIN }, loc).expect("stock"));
    let b = model::stock_world_aabb_for(&world, None).expect("stock box");
    assert!((b.min.z + SKIN).abs() < 1e-9 && (b.max.z - SKIN).abs() < 1e-9 && b.min.x.abs() < 1e-9, "stock at {:?}..{:?}", b.min, b.max);
    (world, h)
}

fn new_world_with_machine() -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let asset = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/tormach-770mx-microarc4.mch");
    h.execute(&world, LoadMachine::from_file(&world, &asset).expect("mount"));
    world
}

/// One case: the kind's params (its tilt set per pose), its source, its
/// area controls and its tool.
struct Case {
    label: &'static str,
    params: OpParams,
    src: SrcRef,
    area: AreaControls,
    tool: &'static str,
    flush: bool,
    neighbour: bool,
}

fn whole() -> SrcRef {
    SrcRef::Entity("part".into())
}

fn case(label: &'static str, params: OpParams, src: SrcRef) -> Case {
    Case { label, params, src, area: AreaControls::default(), tool: MILL, flush: false, neighbour: false }
}

fn set_tilt(p: &mut OpParams, t: Option<Tilt>) {
    match p {
        OpParams::Face { tilt, .. }
        | OpParams::Contour { tilt, .. }
        | OpParams::Pocket { tilt, .. }
        | OpParams::Adaptive2D { tilt, .. }
        | OpParams::Offset2D { tilt, .. }
        | OpParams::Engrave { tilt, .. }
        | OpParams::Chamfer2D { tilt, .. }
        | OpParams::Circular { tilt, .. }
        | OpParams::Slot { tilt, .. }
        | OpParams::Trace { tilt, .. } => *tilt = t,
        other => panic!("{:?} takes no tilt", other.kind()),
    }
}

fn seeded(kind: OpType) -> OpParams {
    let p = seed_params(kind.ordinal());
    assert_eq!(p.kind(), kind);
    p
}

/// Face of the stock down to 1 mm under the part's skin.
fn face() -> Case {
    let mut p = seeded(OpType::Face);
    if let OpParams::Face { cut_z, clearance_z, top_z, stepover, .. } = &mut p {
        (*cut_z, *clearance_z, *top_z, *stepover) = (Height::fixed(PART - 1.0), Height::fixed(40.0), None, 4.0);
    }
    case("face", p, SrcRef::Stock)
}

/// A contour of the whole part on its Outside, with a second part 2 mm
/// past its +X wall: the 6 mm cutter rides through the neighbour. The gap
/// is along X, the tilt axis, so the neighbour stands beside the wall in
/// every pose. A contour cannot be put into its own part: Inside on a
/// whole section, and a negative stock to leave, are refused at create.
fn contour() -> Case {
    let mut p = seeded(OpType::Contour);
    if let OpParams::Contour { side, lead, top_z, bottom_z, retract_z, clearance_z, stepdown, tab_count, .. } = &mut p {
        (*side, *lead, *tab_count) = (Side::Outside, Lead::None, 0);
        (*top_z, *bottom_z, *retract_z, *clearance_z) = (Height::fixed(PART), Height::fixed(PART - 3.0), Height::fixed(30.0), Height::fixed(40.0));
        *stepdown = 3.0;
    }
    Case { neighbour: true, ..case("contour", p, whole()) }
}

/// A pocket-family kind on the whole part, its area offset 3 mm into the
/// part (the #1603 probe).
fn keep_kind(label: &'static str, kind: OpType) -> Case {
    let mut p = seeded(kind);
    match &mut p {
        OpParams::Pocket { top_z, bottom_z, clearance_z, retract_z, stepdown, stepover, .. }
        | OpParams::Adaptive2D { top_z, bottom_z, clearance_z, retract_z, stepdown, stepover, .. }
        | OpParams::Offset2D { top_z, bottom_z, clearance_z, retract_z, stepdown, stepover, .. } => {
            // One level, 3 mm under the part's skin: the smallest cut that
            // keeps the adaptive kind under the 60 s debug budget.
            (*top_z, *bottom_z, *clearance_z, *retract_z) = (Height::fixed(PART + 1.0), Height::fixed(PART - 3.0), Height::fixed(40.0), None);
            (*stepdown, *stepover) = (4.0, 3.0);
        }
        _ => unreachable!("a pocket-family kind"),
    }
    Case { area: AreaControls { offset: 3.0, ..Default::default() }, ..case(label, p, whole()) }
}

/// An Engrave or a Trace of the whole part, 7 mm under its skin on a 3 mm
/// mark budget (the #1595 probe).
fn mark(label: &'static str, kind: OpType) -> Case {
    let mut p = seeded(kind);
    match &mut p {
        OpParams::Engrave { top_z, bottom_z, clearance_z, retract_z, stepdown, .. }
        | OpParams::Trace { top_z, bottom_z, clearance_z, retract_z, stepdown, .. } => {
            (*top_z, *bottom_z, *clearance_z, *retract_z) = (Height::fixed(PART), Height::fixed(PART - 7.0), Height::fixed(40.0), None);
            *stepdown = 10.0;
        }
        _ => unreachable!("a mark kind"),
    }
    case(label, p, whole())
}

/// A 2D Chamfer of the stock's edge where the part fills the stock: it
/// breaks the part's edge (the #1631 probe).
fn chamfer() -> Case {
    let mut p = seeded(OpType::Chamfer2D);
    if let OpParams::Chamfer2D { width, angle_deg, top_z, clearance_z, retract_z, side, .. } = &mut p {
        (*width, *angle_deg, *side) = (2.0, 45.0, Side::Outside);
        (*top_z, *clearance_z, *retract_z) = (Height::fixed(SKIN), Height::fixed(40.0), None);
    }
    Case { tool: CHAMFER_MILL, flush: true, ..case("chamfer", p, SrcRef::Stock) }
}

/// A Circular of the stock over the part, 1 mm under the part's skin.
fn circular() -> Case {
    let mut p = seeded(OpType::Circular);
    if let OpParams::Circular { center, radius, vault, top_z, bottom_z, clearance_z, retract_z, stepdown, stepover, .. } = &mut p {
        (*center, *radius, *vault, *stepover) = (Point2::new(20.0, 0.0), 8.0, false, 2.0);
        (*top_z, *bottom_z, *clearance_z, *retract_z) = (Height::fixed(SKIN), Height::fixed(PART - 1.0), Height::fixed(40.0), None);
        *stepdown = 6.0;
    }
    case("circular", p, SrcRef::Stock)
}

/// Generate `c` in `pose`; true when the op carries a blocking Gouge. The
/// post is then checked to be refused (by name when tilted, where the
/// stock sim does not judge), with no file written.
fn convicted(c: &Case, pose: &str, a: Option<f64>) -> bool {
    let (world, mut h) = job(c.tool, c.flush, c.neighbour);
    let mut params = c.params.clone();
    set_tilt(&mut params, tilt(a));
    h.execute(&world, CreateOperation::new(&world, "K1", c.tool, c.src.clone(), params.clone()).expect("create"));
    if !c.area.is_default() {
        h.execute(&world, EditOperationParams::with_area(&world, "K1", params, c.area.clone()).expect("area"));
    }
    h.execute(&world, GenerateOperation::new(&world, "K1").expect("generate"));
    let tp = model::find_by_name(&world, "K1").and_then(|e| model::toolpath_of(&world, &e)).expect("cached path");
    assert!(!tp.path.moves.is_empty(), "{} {pose}: the cut was generated: {:?}", c.label, tp.path.notes);
    // The 2D Chamfer of the stock answers with its own verdict (#1631).
    let gouge = tp.warnings.iter().any(|w| matches!(w, Warning::Gouge { .. } | Warning::StockChamferGouge { .. }));
    let out = std::env::temp_dir().join(format!("camcraft-2720-{}-{pose}-{}.nc", c.label, std::process::id()));
    let _ = std::fs::remove_file(&out);
    let posted = posting::post_setup(&world, &out);
    let wrote = out.exists();
    let _ = std::fs::remove_file(&out);
    if gouge {
        let err = posted.err().unwrap_or_else(|| panic!("{} {pose}: a convicted op must block the post", c.label));
        eprintln!("{} {pose}: convicted; post refused: {err}", c.label);
        assert!(err.contains("post blocked"), "{} {pose}: {err}", c.label);
        assert!(a.is_none() || err.contains("cuts past the finish surface"), "{} {pose}: refused by name: {err}", c.label);
        assert!(!wrote, "{} {pose}: no file is written", c.label);
    } else {
        eprintln!("{} {pose}: NOT convicted; post {}; warnings {:?}", c.label, if posted.is_ok() { "WROTE" } else { "refused" }, tp.warnings);
    }
    gouge
}

/// Each of `poses` of `c` is convicted at generation and never posts.
fn assert_refused_in(c: &Case, poses: &[(&str, Option<f64>)]) {
    let missed: Vec<&str> = poses.iter().filter(|(pose, a)| !convicted(c, pose, *a)).map(|(pose, _)| *pose).collect();
    assert!(missed.is_empty(), "{}: a cut into the part is not convicted in pose(s) {missed:?}", c.label);
}

fn assert_refused_in_every_pose(c: &Case) {
    assert_refused_in(c, &POSES);
}

#[test]
fn a_tilted_face_into_the_part_is_refused() {
    assert_refused_in_every_pose(&face());
}

#[test]
fn a_tilted_contour_into_a_neighbour_part_is_refused() {
    assert_refused_in_every_pose(&contour());
}

#[test]
fn a_tilted_pocket_offset_into_the_part_is_refused() {
    assert_refused_in_every_pose(&keep_kind("pocket", OpType::Pocket));
}

// The adaptive ladder is the slow kind in debug: one test for each tilted
// pose keeps each test under the 60 s budget. The upright pose (over 60 s
// here) is the keep law's own suite (#1603), not repeated.
#[test]
fn a_tilted_adaptive_offset_into_the_part_is_refused() {
    assert_refused_in(&keep_kind("adaptive", OpType::Adaptive2D), &POSES[1..2]);
}

#[test]
fn a_tilted_adaptive_offset_into_the_part_is_refused_at_a_minus_90() {
    assert_refused_in(&keep_kind("adaptive", OpType::Adaptive2D), &POSES[2..3]);
}

#[test]
fn a_tilted_offset2d_offset_into_the_part_is_refused() {
    assert_refused_in_every_pose(&keep_kind("offset2d", OpType::Offset2D));
}

#[test]
fn a_tilted_engrave_deeper_than_a_mark_is_refused() {
    assert_refused_in_every_pose(&mark("engrave", OpType::Engrave));
}

#[test]
fn a_tilted_trace_deeper_than_a_mark_is_refused() {
    assert_refused_in_every_pose(&mark("trace", OpType::Trace));
}

#[test]
fn a_tilted_chamfer_of_the_stock_into_the_part_is_refused() {
    assert_refused_in_every_pose(&chamfer());
}

#[test]
fn a_tilted_circular_into_the_part_is_refused() {
    assert_refused_in_every_pose(&circular());
}
