//! #1735: two picked open edges of a part are the walls of a band the
//! stock outline closes. The pocket family (2D Offset, 2D Pocket, 2D
//! Adaptive) cuts between the walls up to the stock edge; the area
//! extension moves the open ends out of the stock along the walls; the
//! tool never passes a wall and never rounds the end of one; a selection
//! with no clear band is refused by name; and a pass that cuts another
//! part of the setup is a blocking gouge - the post is refused.
//!
//! Provenance: our own fixture (a 40 x 40 x 20 block with a 16 mm slot
//! to Z 10, in a 70 x 70 stock); no vendor material consulted.
//!
//! Gate map: covers app::operation::open_band, app::operation::selection
//! (the pocket arm of selection_geometry_in), app::operation::boundary
//! (the lane clip), app::operation::boundary_ctx, app::operation::keep_law
//! (neighbour_parts), app::posting.

mod area_support;

use area_support::*;
use camcraft_app::commands::GenerateOperation;
use camcraft_app::geometry::{Mesh, Triangle};
use camcraft_app::posting;
use camcraft_app::verify::Warning;

/// Half the slot's width, and the stock's half side.
const W: f64 = 8.0;
const S: f64 = 35.0;

/// A 40 x 40 x [0, 20] block with a slot |x| <= 8 down to Z 10, open at
/// both ends (a U profile in XZ run along Y). Outward-wound.
fn slotted_block() -> Mesh {
    sheared_block(0.0)
}

/// The slotted block with every point lifted by `k * y`: at `k != 0` no
/// edge along the slot is level.
fn sheared_block(k: f64) -> Mesh {
    let prof = [(-20.0, 0.0), (20.0, 0.0), (20.0, 20.0), (W, 20.0), (W, 10.0), (-W, 10.0), (-W, 20.0), (-20.0, 20.0)];
    let p = |i: usize, y: f64| Point3::new(prof[i].0, y, prof[i].1 + k * y);
    let (y0, y1) = (-20.0, 20.0);
    let mut f = Vec::new();
    for [a, b, c] in [[0, 1, 4], [1, 2, 3], [1, 3, 4], [0, 4, 5], [0, 5, 6], [0, 6, 7]] {
        f.push(Triangle::new(p(a, y0), p(b, y0), p(c, y0)));
        f.push(Triangle::new(p(a, y1), p(c, y1), p(b, y1)));
    }
    for i in 0..prof.len() {
        let j = (i + 1) % prof.len();
        f.push(Triangle::new(p(i, y1), p(j, y1), p(j, y0)));
        f.push(Triangle::new(p(i, y1), p(j, y0), p(i, y0)));
    }
    Mesh::from_facets(f)
}

fn slot_world() -> World {
    world_with(Some(ImportModel::from_mesh("part", slotted_block())), 0.0, 20.0)
}

/// The open chain of `name` whose points all satisfy `pick`.
fn edge(world: &World, name: &str, pick: impl Fn(&[f64; 3]) -> bool) -> GeoSel {
    let e = model::find_by_name(world, name).expect("model");
    let f = model::features_of(world, &e).expect("features");
    let c = feature::chains(&f)
        .into_iter()
        .find(|c| !c.closed && {
            let p = feature::chain_points(&f, c);
            p.len() >= 2 && p.iter().all(&pick)
        })
        .expect("the edge is its own open chain");
    GeoSel { entity: name.into(), feature: FeatureSel::Chain(c.id) }
}

fn at(v: f64, want: f64) -> bool {
    (v - want).abs() < 1e-6
}

/// The slot's top edge at x = `x` (Z 20).
fn wall(world: &World, x: f64) -> GeoSel {
    edge(world, "part", |q| at(q[0], x) && at(q[2], 20.0))
}

fn walls(world: &World) -> SrcRef {
    SrcRef::Selection(vec![wall(world, -W), wall(world, W)])
}

fn kind(to: OpType, top: f64, bottom: f64) -> OpParams {
    let mut p = operation::retype_params(&pocket(top, bottom), to as usize);
    match &mut p {
        OpParams::Adaptive2D { top_z, bottom_z, stepdown, stepover, .. }
        | OpParams::Offset2D { top_z, bottom_z, stepdown, stepover, .. } => {
            *top_z = Height::fixed(top);
            *bottom_z = Height::fixed(bottom);
            *stepdown = 5.0;
            *stepover = 4.0;
        }
        _ => {}
    }
    assert_eq!(p.kind(), to);
    p
}

fn kinds() -> [(&'static str, OpParams); 3] {
    [
        ("2D Offset", kind(OpType::Offset2D, 20.0, 10.0)),
        ("2D Pocket", pocket(20.0, 10.0)),
        ("2D Adaptive", kind(OpType::Adaptive2D, 20.0, 10.0)),
    ]
}

/// The cutting feeds (below the top) of a generated path.
fn cuts(t: &Toolpath) -> Vec<Point3> {
    feeds(t).into_iter().filter(|p| p.z < 20.0 - EPS).collect()
}

fn refusal(world: &World, src: SrcRef, params: OpParams, area: AreaControls) -> String {
    run(world, &op(src, params, area)).expect_err("refused")
}

#[test]
fn two_open_slot_edges_cut_the_band_up_to_the_stock_edge() {
    let world = slot_world();
    for (name, params) in kinds() {
        let (path, warnings) = run(&world, &op(walls(&world), params, AreaControls::default())).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(warnings, 0, "{name}: a clean band has no findings");
        let c = cuts(&path);
        assert!(!c.is_empty(), "{name}: the band is cut");
        // Between the walls: the 5 mm tool's centre stays 5 mm off each.
        let reach_x = max_by(&c, |p| p.x.abs());
        assert!(reach_x <= W - R + EPS, "{name}: the tool passed a wall edge, |x| {reach_x}");
        // Up to the stock edge at both open ends (tool inside the stock),
        // past the ends of the walls at |y| 20.
        let (hi, lo) = (max_by(&c, |p| p.y), -max_by(&c, |p| -p.y));
        assert!((hi - (S - R)).abs() < 0.05 && (lo + (S - R)).abs() < 0.05, "{name}: the open ends reach y {lo}..{hi}");
        // Never under the slot's floor.
        assert!(c.iter().all(|p| p.z >= 10.0 - EPS), "{name}: a cut under the floor");
    }
}

#[test]
fn the_extension_moves_the_open_ends_out_of_the_stock_along_the_walls() {
    let world = slot_world();
    let ext = 7.0;
    for (name, params) in kinds() {
        let area = AreaControls { extension: ext, ..Default::default() };
        let (path, warnings) = run(&world, &op(walls(&world), params, area)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(warnings, 0, "{name}");
        let c = cuts(&path);
        // The pinned distance: the tool centre goes `ext` further at each
        // open end, 2 mm outside the stock edge.
        let (hi, lo) = (max_by(&c, |p| p.y), -max_by(&c, |p| -p.y));
        assert!((hi - (S - R + ext)).abs() < 0.05 && (lo + (S - R + ext)).abs() < 0.05, "{name}: the open ends reach y {lo}..{hi}");
        // The extension does not widen the band: not into a wall, and
        // not round the end of a wall where no part stands (|y| > 20).
        let reach_x = max_by(&c, |p| p.x.abs());
        assert!(reach_x <= W - R + EPS, "{name}: the tool rounded a wall end, |x| {reach_x}");
    }
}

#[test]
fn a_bottom_under_the_slot_floor_never_cuts_the_part() {
    // The part fills the band inside the block at Z 5: only the stock
    // beyond the block's ends is left to cut.
    let world = slot_world();
    let (path, warnings) =
        run(&world, &op(walls(&world), kind(OpType::Offset2D, 20.0, 5.0), AreaControls::default())).expect("generates");
    assert_eq!(warnings, 0);
    let c = cuts(&path);
    assert!(!c.is_empty());
    assert!(c.iter().all(|p| p.y.abs() >= 20.0 + R - EPS), "a cut over the part's floor");
}

#[test]
fn a_selection_with_no_clear_band_is_refused_by_name() {
    let world = slot_world();
    let offset = || kind(OpType::Offset2D, 20.0, 10.0);
    let none = AreaControls::default;
    // One edge: the refusal every kind has always given, word for word.
    let one = refusal(&world, SrcRef::Selection(vec![wall(&world, W)]), offset(), none());
    assert!(one.contains("is an open run - a 2D Offset cannot end mid-edge; select a closed chain"), "{one}");
    // Walls at different heights: a top edge and a floor edge.
    let floor = edge(&world, "part", |q| at(q[0], W) && at(q[2], 10.0));
    let high = refusal(&world, SrcRef::Selection(vec![wall(&world, -W), floor]), offset(), none());
    assert!(high.contains("are at different heights (Z 20") && high.contains("and Z 10"), "{high}");
    // Edges that are not level: the slot of a block sheared 0.1 in Z per Y.
    let slope = world_with(Some(ImportModel::from_mesh("part", sheared_block(0.1))), 0.0, 20.0);
    let rising = |x: f64| edge(&slope, "part", move |q| at(q[0], x) && q[2] > 15.0);
    let tilt = refusal(&slope, SrcRef::Selection(vec![rising(-W), rising(W)]), offset(), none());
    assert!(tilt.contains("is not level (it runs from Z 18") && tilt.contains("to Z 22"), "{tilt}");
    // Edges that do not face each other: a wall and the end of a land.
    let land = edge(&world, "part", |q| at(q[1], 20.0) && at(q[2], 20.0) && q[0] > W - 1e-6);
    let square = refusal(&world, SrcRef::Selection(vec![wall(&world, -W), land]), offset(), none());
    assert!(square.contains("do not face each other (they stand at 90 degrees"), "{square}");
    // Three walls.
    let outer = edge(&world, "part", |q| at(q[0], 20.0) && at(q[2], 20.0));
    let three = refusal(&world, SrcRef::Selection(vec![wall(&world, -W), wall(&world, W), outer]), offset(), none());
    assert!(three.contains("spell 3 separate runs - an open-edge band has exactly two walls"), "{three}");
    // Open edges beside a closed boundary.
    let top = GeoSel { entity: "part".into(), feature: top_face(&world, "part", 10.0) };
    let mixed = refusal(&world, SrcRef::Selection(vec![wall(&world, -W), wall(&world, W), top]), offset(), none());
    assert!(mixed.contains("mixes 2 open edges with closed boundaries"), "{mixed}");
    // A containment that lets the tool past the band.
    let on = AreaControls { containment: Some(Containment::On), ..Default::default() };
    let past = refusal(&world, walls(&world), offset(), on);
    assert!(past.contains("an open-edge band keeps the tool between its two edges"), "{past}");
}

#[test]
fn with_no_stock_nothing_closes_the_band() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", slotted_block()));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let why = refusal(&world, walls(&world), kind(OpType::Offset2D, 20.0, 10.0), AreaControls::default());
    assert!(why.contains("this setup has no stock"), "{why}");
}

#[test]
fn the_selection_account_names_the_band() {
    let world = slot_world();
    let SrcRef::Selection(items) = walls(&world) else { unreachable!() };
    let (notes, refused) = operation::selection_account(&world, None, &items, &kind(OpType::Offset2D, 20.0, 10.0));
    assert_eq!(refused, None);
    assert!(notes.iter().any(|n| n == "2 open edges closed by the stock outline into a band"), "{notes:?}");
}

/// The gouge law is not loosened: a second part standing in the band's
/// open end is cut by the pass, the operation carries a blocking gouge
/// and the post is refused, with nothing written.
#[test]
fn a_band_pass_that_cuts_another_part_blocks_the_post() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", slotted_block()));
    h.execute(&world, ImportModel::from_mesh("clamp", cube(Point3::new(-4.0, 24.0, 0.0), Point3::new(4.0, 32.0, 20.0))));
    let loc = Location { pos: Point3::new(0.0, 0.0, 0.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz: 20.0 }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "P1", TOOL, walls(&world), kind(OpType::Offset2D, 20.0, 10.0)).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "P1").expect("generate"));
    let e = model::find_by_name(&world, "P1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    assert!(!tp.path.moves.is_empty(), "the band was generated");
    let gouges = tp.warnings.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count();
    assert!(gouges > 0, "the pass through the clamp is a gouge: {:?}", tp.warnings);
    assert!(tp.warnings.iter().any(Warning::blocks_post));
    let out = std::env::temp_dir().join(format!("camcraft-open-band-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let err = posting::post_setup(&world, &out).expect_err("a gouge blocks the post");
    assert!(err.contains("post blocked"), "{err}");
    assert!(err.contains("gouge") && err.contains("cuts below the model"), "{err}");
    assert!(!out.exists(), "blocked output is never written");
}
