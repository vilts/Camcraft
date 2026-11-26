//! #3132: a 2D Contour whose lead-in and lead-out are both None adds no
//! XY lead travel at the seam - not even through the Arc entry, whose
//! vertical arc beside the wall ran back along the seam's tangent and
//! read as a linear lead in the viewport and the post. The op is edited
//! from a linear lead to None through the command history, the way the
//! Lead in / Lead out page applies its draft, and generated on the
//! user's own part (`testdata/models/Milling-test.step`). Linear and Arc
//! leads, and the Arc entry with a lead, keep their shapes. #3247: a tool
//! that is not centre-cutting is refused there, and the refusal names the
//! lead-in, not the Arc entry that is already set.
//!
//! Gate map: covers app::commands (EditOperationParams,
//! GenerateOperation) over camcraft_motion::machining2d::contour.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::Point3;
use camcraft_app::links::{Approach, Links};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{self, Height, HeightReference, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;
use camcraft_app::{model, posting};
use camcraft_motion::approach::CENTER_CUT_REFUSAL;
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-1000-4FL";

fn contour(lead: Lead, lead_out: Option<Lead>, entry: Entry3D) -> OpParams {
    // The user's values: Lead out > Type, both extensions and the
    // overlap stated explicitly.
    let approach = Approach { lead_out, ext_in: 0.0, ext_out: 0.0, overlap: 0.0, ..Approach::DEFAULT };
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 20.5),
        retract_z: Height::of(HeightReference::TopOfStock, 13.5),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z: Height::of(HeightReference::BottomOfStock, 0.0),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Auto,
        lead,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry,
        helix_radius: 1.0,
        blend_turns: false,
        blend_radius: None,
        tilt: None,
        links: Links { approach, ..Links::PLANAR },
        chain_overrides: Vec::new(), picks: Default::default(),
    }
}

/// The user's part with a contour at the seeded linear lead, generated.
fn job(entry: Entry3D) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let imp = ImportModel::new(&path).expect("the user's STEP imports");
    let name = imp.name.clone();
    h.execute(&world, imp);
    let loc = Location { pos: Point3::new(0.0, 0.0, -7.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz: 27.0 }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let seed = contour(Lead::Linear { length: 8.0 }, None, entry);
    h.execute(&world, CreateOperation::new(&world, "Op1", TOOL, SrcRef::Entity(name), seed).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Op1").expect("generate"));
    (world, h)
}

/// Apply `params` as the page's draft does, generate, read the path.
fn regenerate(world: &World, h: &mut History, params: OpParams) -> Toolpath {
    h.execute(world, EditOperationParams::new(world, "Op1", params).expect("edit"));
    h.execute(world, GenerateOperation::new(world, "Op1").expect("generate"));
    let e = model::find_by_name(world, "Op1").expect("op");
    model::toolpath_of(world, &e).expect("cached path").path.clone()
}

fn same_xy(a: Point3, b: Point3) -> bool {
    (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
}

/// The path's first move that changes XY other than a rapid: its index,
/// start and end.
fn first_xy_cut(t: &Toolpath) -> (usize, Point3, Point3) {
    let mut at = t.moves.first().map(|m| m.to).expect("moves");
    for (i, m) in t.moves.iter().enumerate().skip(1) {
        if m.kind != MoveKind::Rapid && !same_xy(at, m.to) {
            return (i, at, m.to);
        }
        at = m.to;
    }
    panic!("no XY cut in the path");
}

/// The cut starts on the ring itself: horizontal at the level, from a
/// point the ring's walk returns to at that level (the closed seam).
fn starts_on_ring(t: &Toolpath) -> bool {
    let (i, from, to) = first_xy_cut(t);
    let flat = (from.z - to.z).abs() < 1e-9;
    let back = t.moves[i + 1..]
        .iter()
        .take_while(|m| m.kind == MoveKind::Feed && (m.to.z - from.z).abs() < 1e-9)
        .any(|m| same_xy(m.to, from));
    flat && back
}

fn posted(world: &World, tag: &str) -> String {
    let nc = std::env::temp_dir().join(format!("camcraft-3132-{tag}-{}.nc", std::process::id()));
    posting::post_setup(world, &nc).expect("posts");
    let text = std::fs::read_to_string(&nc).expect("written");
    let _ = std::fs::remove_file(&nc);
    text
}

#[test]
fn arc_entry_with_both_leads_none_starts_the_cut_on_the_ring() {
    let (world, mut h) = job(Entry3D::Arc);
    let t = regenerate(&world, &mut h, contour(Lead::None, Some(Lead::None), Entry3D::Arc));
    let (i, from, to) = first_xy_cut(&t);
    assert!(starts_on_ring(&t), "move {i} {from:?} -> {to:?} is not on the ring: {:?}", t.notes);
    // Before the cut the tool only travels in rapids and straight down.
    assert!(t.moves[..i].iter().all(|m| m.kind == MoveKind::Rapid || m.arc.is_none()));
    assert!(t.notes.iter().any(|n| n.contains("arc stands down with no lead-in")), "{:?}", t.notes);
    let nc = posted(&world, "none");
    assert!(!nc.contains("G18") && !nc.contains("G19"), "no vertical arc entry posts");
}

#[test]
fn plunge_entry_with_both_leads_none_starts_the_cut_on_the_ring() {
    let (world, mut h) = job(Entry3D::Plunge);
    let t = regenerate(&world, &mut h, contour(Lead::None, Some(Lead::None), Entry3D::Plunge));
    assert!(starts_on_ring(&t), "{:?}", first_xy_cut(&t));
}

#[test]
fn linear_and_arc_leads_keep_their_shapes() {
    let (world, mut h) = job(Entry3D::Plunge);
    let t = regenerate(&world, &mut h, contour(Lead::Linear { length: 8.0 }, None, Entry3D::Plunge));
    let (i, from, to) = first_xy_cut(&t);
    assert!(!starts_on_ring(&t));
    assert!(t.moves[i].arc.is_none());
    let len = ((to.x - from.x).powi(2) + (to.y - from.y).powi(2)).sqrt();
    assert!((len - 8.0).abs() < 1e-3, "the linear lead is 8 mm: {len}");
    let t = regenerate(&world, &mut h, contour(Lead::Arc { radius: 5.0 }, None, Entry3D::Plunge));
    let (i, ..) = first_xy_cut(&t);
    assert!(!starts_on_ring(&t));
    assert!(t.moves[i].arc.is_some(), "the arc lead is an arc");
    // And back to None: the lead goes away again.
    let t = regenerate(&world, &mut h, contour(Lead::None, Some(Lead::None), Entry3D::Plunge));
    assert!(starts_on_ring(&t));
}

#[test]
fn arc_entry_with_a_linear_lead_still_descends_on_its_arc() {
    let (world, mut h) = job(Entry3D::Arc);
    let t = regenerate(&world, &mut h, contour(Lead::Linear { length: 8.0 }, None, Entry3D::Arc));
    let (_, from, to) = first_xy_cut(&t);
    assert!(from.z > to.z, "the wall arc descends while it travels: {from:?} -> {to:?}");
    assert!(!t.notes.iter().any(|n| n.contains("arc stands down")), "{:?}", t.notes);
}

/// #3247: the part and stock of `job`, with a tool that is not
/// centre-cutting, generated straight through `operation::generate`.
fn refusal_for(lead: Lead) -> String {
    let world = new_world();
    let mut h = History::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let imp = ImportModel::new(&path).expect("the user's STEP imports");
    let name = imp.name.clone();
    h.execute(&world, imp);
    let loc = Location { pos: Point3::new(0.0, 0.0, -7.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz: 27.0 }, loc).expect("stock"));
    let em = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "NC", Tool { center_cutting: false, ..em }).expect("tool"));
    let params = contour(lead, Some(Lead::None), Entry3D::Arc);
    let op = Operation {
        tool: "NC".into(),
        src: SrcRef::Entity(name),
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    operation::generate(&world, &op, "Op1").map(|_| String::new()).unwrap_or_else(|e| e.to_string())
}

#[test]
fn arc_entry_with_no_lead_in_refuses_a_non_centre_cutting_tool_and_names_the_lead_in() {
    let msg = refusal_for(Lead::None);
    assert!(msg.contains(CENTER_CUT_REFUSAL), "the refusal stands: {msg}");
    assert!(msg.contains("the Arc entry does not run when the lead-in is None"), "{msg}");
    assert!(msg.contains("set a Linear or Arc lead-in, or use a centre-cutting tool"), "{msg}");
    assert!(!msg.contains("pick the Arc entry"), "the Arc entry is already set: {msg}");
}
