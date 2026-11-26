//! #2988 through the operation layer: a 2D Chamfer takes one or more
//! selected open edge chains, keeps connected picks in one pass, enters
//! and exits each chain in air, and declares only the selected edges for
//! stock verification.
//!
//! Gate map (#2988): covers app::operation (selection, generate, stitch),
//! app::posting, motion::machining2d (chamfer), sim::stock_sim (edge_break,
//! wall_anchor).

mod area_support;

use area_support::*;
use camcraft_app::commands::GenerateOperation;
use camcraft_app::posting;

/// The 90-degree chamfer mill the selected-edge ride needs.
const CONE: &str = "CM-0600-90";

fn world() -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(
        &world,
        ImportModel::from_mesh(
            "part",
            cube(
                Point3::new(-20.0, -20.0, 0.0),
                Point3::new(20.0, 20.0, 10.0),
            ),
        ),
    );
    h.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::FromModel {
                source: "part".into(),
            },
            Default::default(),
        )
        .expect("stock"),
    );
    let tool = catalog()
        .into_iter()
        .find(|(name, _)| *name == CONE)
        .map(|(_, tool)| tool)
        .expect("catalog cone");
    h.execute(&world, AddTool::new(&world, CONE, tool).expect("tool"));
    world
}

fn params() -> OpParams {
    OpParams::Chamfer2D {
        width: 1.0,
        angle_deg: 45.0,
        minimum_cone_radius: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: Some(Height::fixed(12.0)),
        top_z: Height::fixed(10.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Outside,
        tolerance: 0.01,
        tilt: None,
        picks: Default::default(),
    }
}

fn north(w: &World) -> GeoSel {
    GeoSel {
        entity: "part".into(),
        feature: rim_edge(w, |q| (q[1] - 20.0).abs() < 1e-9),
    }
}

fn east(w: &World) -> GeoSel {
    GeoSel {
        entity: "part".into(),
        feature: rim_edge(w, |q| (q[0] - 20.0).abs() < 1e-9),
    }
}

fn south(w: &World) -> GeoSel {
    GeoSel {
        entity: "part".into(),
        feature: rim_edge(w, |q| (q[1] + 20.0).abs() < 1e-9),
    }
}

fn generate_params(w: &World, picks: Vec<GeoSel>, params: OpParams) -> Toolpath {
    let mut h = History::default();
    h.execute(
        w,
        CreateOperation::new(w, "CHM", CONE, SrcRef::Selection(picks), params).expect("create"),
    );
    h.execute(w, GenerateOperation::new(w, "CHM").expect("generate"));
    let e = model::find_by_name(w, "CHM").expect("operation");
    model::toolpath_of(w, &e)
        .expect("cached chamfer")
        .path
        .clone()
}

fn generate(w: &World, picks: Vec<GeoSel>) -> Toolpath {
    generate_params(w, picks, params())
}

fn corner_feeds(path: &Toolpath) -> Vec<Point3> {
    path.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.to.z - 9.0).abs() < 1e-9)
        .map(|m| m.to)
        .collect()
}

#[test]
fn a_selected_edge_generates_and_verifies_one_open_chamfer() {
    let w = world();
    let path = generate(&w, vec![north(&w)]);
    assert_eq!(path.breaks.len(), 1, "one selected edge, one break");
    let edge = &path.breaks[0].edge;
    assert_eq!(edge.len(), 2, "the selected edge stays a two-point run");
    assert_ne!(edge.first(), edge.last(), "the edge is not closed");
    assert!(
        edge.iter()
            .all(|q| (q.y - 20.0).abs() < 1e-9 && q.z == 10.0),
        "only the selected top edge is declared: {edge:?}"
    );
    let feeds = corner_feeds(&path);
    assert_eq!(
        feeds.iter().filter(|q| (q.y - 20.0).abs() < 1e-9).count(),
        2,
        "the two selected ends are ridden: {feeds:?}"
    );
    let e = model::find_by_name(&w, "CHM").expect("operation");
    let cached = model::toolpath_of(&w, &e).expect("cached");
    assert!(
        cached.warnings.is_empty(),
        "the open ride is clean: {:?}",
        cached.warnings
    );
    assert!(
        posting::post_setup_unwritten(&w).is_ok(),
        "the selected chamfer verifies and posts"
    );
}

#[test]
fn a_selected_open_edge_offsets_by_the_minimum_radius() {
    let w = world();
    let mut p = params();
    if let OpParams::Chamfer2D {
        minimum_cone_radius,
        side,
        ..
    } = &mut p
    {
        *minimum_cone_radius = 2.0;
        *side = Side::Outside;
    }
    let path = generate_params(&w, vec![north(&w)], p);
    let tip: Vec<_> = path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.to.z - 7.0).abs() < 1e-9)
        .collect();
    assert!(
        tip.iter()
            .any(|m| (m.to.y - 22.0).abs() < 1e-9),
        "the axis is 2 mm beyond the selected north wall: {tip:?}"
    );
    assert!(
        path.breaks[0]
            .edge
            .iter()
            .all(|q| (q.y - 20.0).abs() < 1e-9 && q.z == 10.0),
        "the offset cone still credits the original edge: {:?}",
        path.breaks[0]
    );
    assert!((path.breaks[0].depth - 1.0).abs() < 1e-9);
    let e = model::find_by_name(&w, "CHM").expect("operation");
    assert!(
        model::toolpath_of(&w, &e)
            .expect("cached")
            .warnings
            .is_empty(),
        "the selected 2-to-3 mm cone band is clean"
    );
}

#[test]
fn connected_selected_edges_generate_one_open_pass() {
    let w = world();
    let path = generate(&w, vec![north(&w), east(&w)]);
    assert_eq!(path.breaks.len(), 1, "two connected edges form one chain");
    assert_eq!(
        path.breaks[0].edge.len(),
        3,
        "the joined chain keeps its corner"
    );
    assert_eq!(
        path.moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge)
            .count(),
        1,
        "one connected chain has one entry"
    );
    assert!(
        corner_feeds(&path)
            .iter()
            .any(|q| { (q.x - 20.0).abs() < 1e-9 && (q.y - 20.0).abs() < 1e-9 }),
        "the connected pass reaches the selected corner"
    );
}

#[test]
fn separate_selected_edges_generate_separate_open_passes() {
    let w = world();
    let path = generate(&w, vec![north(&w), south(&w)]);
    assert_eq!(path.breaks.len(), 2, "separate chains keep separate breaks");
    assert_eq!(
        path.moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge)
            .count(),
        2,
        "each chain is entered as a separate pass"
    );
    assert!(
        path.breaks
            .iter()
            .all(|b| b.edge.iter().all(|q| (q.y - 20.0).abs() < 1e-9)
                || b.edge.iter().all(|q| (q.y + 20.0).abs() < 1e-9)),
        "a break never joins the separated north and south edges: {:?}",
        path.breaks
    );
}

#[test]
fn open_chamfer_runs_with_boundary_area_settings_refuse_by_name() {
    let w = world();
    let mut o = op(
        SrcRef::Selection(vec![north(&w)]),
        params(),
        AreaControls {
            offset: 1.0,
            ..Default::default()
        },
    );
    o.tool = CONE.into();
    let why = run(&w, &o).expect_err("Boundary settings cannot shape open chamfer runs");
    assert!(
        why.contains("Boundary settings shape an area"),
        "{why}"
    );
    assert!(why.contains("open chamfer runs"), "{why}");
    assert!(why.contains("reset them"), "{why}");
}

#[test]
fn an_open_chamfer_does_not_credit_an_unselected_edge() {
    let w = world();
    let path = generate(&w, vec![north(&w)]);
    assert!(
        posting::verification_gate(&w).is_empty(),
        "the declared selected edge posts"
    );
    let e = model::find_by_name(&w, "CHM").expect("operation");
    let cached = model::toolpath_of(&w, &e).expect("cached");
    let mut moved = path;
    for m in &mut moved.moves {
        m.from.y -= 40.0;
        m.to.y -= 40.0;
    }
    let rebuilt = camcraft_app::schema::Toolpath::new(moved, cached.warnings);
    e.entity_view(&w).set(rebuilt);
    let gate = posting::verification_gate(&w);
    assert!(
        gate.iter().any(|line| line.starts_with("gouge:")),
        "moving the cut to the unselected south edge is still a gouge: {gate:?}"
    );
}
