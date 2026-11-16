//! A Contour on an open run picked on the STOCK entity (#1928): the tool
//! never rides the picked line silently. The run obeys the Side setting -
//! Outside stands the tool one radius plus the stock to leave off the
//! stock, Inside one radius into it, Auto away from the part - and a run
//! with no side to read, or a tool that would enter the part, is refused
//! by name. Before the fix the tool centre rode the picked line.
//!
//! Gate map (#1928): covers app::operation (generate), motion
//! machining2d (contour_run_stock, contour_run_side), app::posting.

mod area_support;

use area_support::*;
use camcraft_app::commands::{GenerateOperation, SetStockSourceOnly};
use camcraft_app::geometry::{Mesh, Triangle};
use camcraft_app::posting;

/// The open chain of `entity`'s top rim at `top` whose points all
/// satisfy `pick`.
fn rim_of(world: &World, entity: &str, top: f64, pick: impl Fn(&[f64; 3]) -> bool) -> FeatureSel {
    let e = model::find_by_name(world, entity).expect("model");
    let f = model::features_of(world, &e).expect("features");
    feature::chains(&f)
        .into_iter()
        .find(|c| !c.closed && {
            let p = feature::chain_points(&f, c);
            p.len() >= 2 && p.iter().all(|q| (q[2] - top).abs() < 1e-9 && pick(q))
        })
        .map(|c| FeatureSel::Chain(c.id))
        .expect("the rim edge is its own open chain")
}

fn stock_name(world: &World) -> String {
    camcraft_app::setup::stock_entity_name(world, None)
}

/// The stock's north top edge (y = the stock's max y) and that y.
fn stock_north(world: &World) -> (Vec<GeoSel>, f64) {
    let (lo, hi) = stock_box(world);
    let name = stock_name(world);
    let f = rim_of(world, &name, hi.z, |q| (q[1] - hi.y).abs() < 1e-9 && q[0] > lo.x - 1e-9);
    (pick(&name, f), hi.y)
}

/// Generate "C1" from `picks` with `side`, `leave` stock to leave.
fn generate(world: &World, picks: Vec<GeoSel>, side: Side, leave: f64) -> Result<(Toolpath, usize), String> {
    let mut p = contour(10.0, 0.0, side);
    if let OpParams::Contour { stock_to_leave, .. } = &mut p {
        *stock_to_leave = leave;
    }
    let o = op(SrcRef::Selection(picks), p, AreaControls::default());
    run(world, &o)
}

/// The cutting feeds below the top (Z 10): where the tool works.
fn cutting(t: &Toolpath) -> Vec<Point3> {
    feeds(t).into_iter().filter(|p| p.z < 10.0 - EPS).collect()
}

/// The XY distances (min, max) of `pts` to the line y = `y`.
fn span(pts: &[Point3], y: f64) -> (f64, f64) {
    let d: Vec<f64> = pts.iter().map(|p| (p.y - y).abs()).collect();
    (d.iter().copied().fold(f64::INFINITY, f64::min), d.iter().copied().fold(f64::NEG_INFINITY, f64::max))
}

#[test]
fn a_stock_run_is_cut_one_radius_off_the_line_by_its_side() {
    let w = block_world();
    let (picks, y) = stock_north(&w);
    // Outside and Auto (the part lies south): north of the line.
    // Inside: south of it, into the stock.
    for (side, sign) in [(Side::Auto, 1.0), (Side::Outside, 1.0), (Side::Inside, -1.0)] {
        let (t, warnings) = generate(&w, picks.clone(), side, 0.0).unwrap_or_else(|e| panic!("{side:?}: {e}"));
        let f = cutting(&t);
        assert!(!f.is_empty(), "{side:?}: the run is cut");
        let (lo, hi) = span(&f, y);
        assert!((lo - R).abs() < EPS && (hi - R).abs() < EPS, "{side:?}: min {lo:.4} max {hi:.4} off the line, want {R}");
        assert!(f.iter().all(|p| (p.y - y) * sign > 0.0), "{side:?}: on the wrong side: {f:?}");
        assert_eq!(warnings, 0, "{side:?}: nothing gouges");
        assert!(t.notes.iter().any(|n| n.contains("open run on the stock")), "{side:?}: {:?}", t.notes);
    }
}

#[test]
fn stock_to_leave_moves_the_stock_run_further_off() {
    let w = block_world();
    let (picks, y) = stock_north(&w);
    let (t, _) = generate(&w, picks, Side::Outside, 0.5).expect("outside");
    let (lo, hi) = span(&cutting(&t), y);
    assert!((lo - (R + 0.5)).abs() < EPS && (hi - (R + 0.5)).abs() < EPS, "min {lo:.4} max {hi:.4}");
}

#[test]
fn a_stock_run_posts_clean() {
    let w = block_world();
    let (picks, _) = stock_north(&w);
    let mut h = History::default();
    let p = contour(10.0, 0.0, Side::Inside);
    h.execute(&w, CreateOperation::new(&w, "C1", TOOL, SrcRef::Selection(picks), p).expect("create"));
    h.execute(&w, GenerateOperation::new(&w, "C1").expect("generate"));
    let e = model::find_by_name(&w, "C1").expect("op");
    assert!(model::toolpath_of(&w, &e).is_some(), "a toolpath");
    assert!(posting::post_setup_unwritten(&w).is_ok(), "{:?}", posting::post_setup_unwritten(&w).err());
}

#[test]
fn auto_on_a_stock_run_with_no_part_is_refused_by_name() {
    let w = world_with(None, 0.0, 10.0);
    let (picks, _) = stock_north(&w);
    let e = generate(&w, picks.clone(), Side::Auto, 0.0).expect_err("no part to cut away from");
    assert!(e.contains("Inside") && e.contains("Outside"), "{e}");
    // An explicit side is enough.
    let (t, _) = generate(&w, picks, Side::Outside, 0.0).expect("outside");
    assert!(!cutting(&t).is_empty());
}

#[test]
fn a_stock_run_whose_tool_enters_the_part_is_refused_by_name() {
    // A 44 x 44 stock round the 40 x 40 part: one radius into the stock
    // from its north edge (y 22) reaches y 12 - inside the part.
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0))));
    let loc = Location { pos: Point3::new(0.0, 0.0, 0.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 44.0, dy: 44.0, dz: 10.0 }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let (picks, _) = stock_north(&world);
    let e = generate(&world, picks.clone(), Side::Inside, 0.0).expect_err("into the part");
    assert!(e.contains("cuts the part"), "{e}");
    // Away from the part stays legal.
    assert!(generate(&world, picks, Side::Auto, 0.0).is_ok());
}

/// Add `TOOL` from the catalog, overridden to a flat endmill of `diameter`.
fn add_flat_tool(world: &World, h: &mut History, name: &str, diameter: f64) {
    let mut tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    tool.diameter = diameter;
    tool.corner_r = 0.0;
    tool.shank_d = diameter;
    tool.holder_d = diameter;
    h.execute(world, AddTool::new(world, name, tool).expect("add tool"));
}

#[test]
fn auto_on_a_stock_run_with_part_changing_sides_between_levels_is_refused_by_name() {
    let world = new_world();
    let mut h = History::default();
    // Upper half sits south of the stock's north edge (y = 35) over Z [5, 10];
    // lower half sits north of it over Z [0, 5].
    h.execute(&world, ImportModel::from_mesh("upper", cube(Point3::new(-20.0, 10.0, 5.0), Point3::new(20.0, 20.0, 10.0))));
    h.execute(&world, ImportModel::from_mesh("lower", cube(Point3::new(-20.0, 50.0, 0.0), Point3::new(20.0, 60.0, 5.0))));
    let shape = StockShape::Box { dx: 70.0, dy: 70.0, dz: 10.0 };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("stock"));
    add_flat_tool(&world, &mut h, TOOL, 10.0);
    let (picks, _) = stock_north(&world);
    let contour_stepped = |side: Side| {
        let mut p = contour(10.0, 0.0, side);
        if let OpParams::Contour { stepdown, .. } = &mut p {
            *stepdown = 5.0;
        }
        op(SrcRef::Selection(picks.clone()), p, AreaControls::default())
    };
    let e = run(&world, &contour_stepped(Side::Auto)).expect_err("part flips sides between levels");
    assert!(e.contains("on one side") && e.contains("on the other"), "{e}");
    assert!(e.contains("Inside") && e.contains("Outside"), "{e}");
    assert!(run(&world, &contour_stepped(Side::Outside)).is_ok());
    assert!(run(&world, &contour_stepped(Side::Inside)).is_ok());
}

#[test]
fn a_stock_run_with_non_positive_offset_is_refused_by_name() {
    let world = new_world();
    let mut h = History::default();
    let shape = StockShape::Box { dx: 70.0, dy: 70.0, dz: 10.0 };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("stock"));
    add_flat_tool(&world, &mut h, TOOL, 0.001);
    let (picks, _) = stock_north(&world);
    let e = generate(&world, picks.clone(), Side::Outside, 0.0).expect_err("sub-tolerance offset");
    assert!(e.contains("open run's offset") && e.contains("it must be positive"), "{e}");
    assert!(generate(&world, picks, Side::Outside, 0.01).is_ok());
}

/// A 70 x 70 x 10 slab with a narrow smooth U-cove (width 6 mm at the
/// mouth, 3 mm at the semicircle bottom) on its north edge, star-shaped
/// around the origin.
fn notched_stock_mesh() -> Mesh {
    let mut pts = vec![(35.0, -35.0), (35.0, 35.0), (3.0, 35.0)];
    let n = 12;
    for i in 0..=n {
        let a = -std::f64::consts::PI * f64::from(i) / f64::from(n);
        pts.push((1.5 * a.cos(), 20.0 + 1.5 * a.sin()));
    }
    pts.extend([(-3.0, 35.0), (-35.0, 35.0), (-35.0, -35.0)]);
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let mut f = Vec::with_capacity(pts.len() * 4);
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        f.push(Triangle::new(Point3::new(0.0, 0.0, 10.0), p(a, 10.0), p(b, 10.0)));
        f.push(Triangle::new(Point3::new(0.0, 0.0, 0.0), p(b, 0.0), p(a, 0.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 0.0), p(b, 10.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 10.0), p(a, 10.0)));
    }
    Mesh::from_facets(f)
}

#[test]
fn a_stock_run_whose_offset_folds_onto_itself_is_refused_by_name() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("cove_src", notched_stock_mesh()));
    let shape = StockShape::FromModel { source: "cove_src".into() };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("stock"));
    h.execute(&world, SetStockSourceOnly::new(&world, true).expect("raw stock only"));
    add_flat_tool(&world, &mut h, TOOL, 10.0);
    add_flat_tool(&world, &mut h, "SMALL", 1.0);
    let name = stock_name(&world);
    let cove = rim_of(&world, &name, 10.0, |q| q[0].abs() <= 3.0 + 1e-6);
    let picks = pick(&name, cove);
    let e = generate(&world, picks.clone(), Side::Outside, 0.0).expect_err("D10 folds inside the cove");
    assert!(e.contains("folds onto itself"), "{e}");
    let mut small_op = op(SrcRef::Selection(picks), contour(10.0, 0.0, Side::Outside), AreaControls::default());
    small_op.tool = "SMALL".into();
    assert!(run(&world, &small_op).is_ok());
}
