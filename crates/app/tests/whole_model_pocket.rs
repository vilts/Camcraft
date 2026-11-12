//! Issue #12: a pocket-family op (2D Pocket, 2D Adaptive) whose source is
//! a whole PART clears the stock around it - the part's sections at the
//! ladder planes are islands, never the area to hollow. A part covering
//! the whole stock outline is refused by name.
//!
//! Gate map (#12): covers app::operation (area, job, material_region).

mod area_support;
mod common;

use area_support::*;
use camcraft_app::geometry::{Mesh, Triangle};

/// A 30 x 30 x [0, 10] block with a 12 x 12 boss to Z 20. Outward-wound.
fn block_with_boss() -> Mesh {
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    let mut f = Vec::new();
    let block = cube(Point3::new(-15.0, -15.0, 0.0), Point3::new(15.0, 15.0, 10.0));
    f.extend(block.facets[0..2].iter().cloned()); // bottom
    f.extend(block.facets[4..12].iter().cloned()); // walls
    let (o, i) = (15.0, 6.0);
    let oo = [(-o, -o), (o, -o), (o, o), (-o, o)];
    let ii = [(-i, -i), (i, -i), (i, i), (-i, i)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, 10.0);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(oo[k]), p(oo[n]), p(ii[n]), p(ii[k]), &mut f); // shoulder
    }
    let boss = cube(Point3::new(-i, -i, 10.0), Point3::new(i, i, 20.0));
    f.extend(boss.facets[2..12].iter().cloned()); // boss top + walls
    Mesh::from_facets(f)
}

fn world(mesh: Mesh, side: f64) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", mesh));
    let stock = StockShape::Box { dx: side, dy: side, dz: 20.0 };
    h.execute(&world, SetStock::new(&world, stock, Location::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    world
}

/// The chord sag the offset rings' corner arcs may show.
const CHORD: f64 = 0.01;

/// Distance from (x, y) to the square of half-width `h` on the origin.
fn to_square(x: f64, y: f64, h: f64) -> f64 {
    let dx = (x.abs() - h).max(0.0);
    let dy = (y.abs() - h).max(0.0);
    dx.hypot(dy)
}

/// No cutting move's tool disc enters the part's section at its Z, and
/// the path does clear the stock around the block.
/// `clear` is the least distance a tool centre keeps from the section.
fn clears_around_the_part(kind: OpParams, label: &str, clear: f64) {
    let world = world(block_with_boss(), 50.0);
    let o = op(SrcRef::Entity("part".into()), kind, AreaControls::default());
    let (path, warns) = run(&world, &o).unwrap_or_else(|e| panic!("{label}: whole-part source generates: {e}"));
    assert_eq!(warns, 0, "{label}: verify stays clean");
    let mut outside = 0usize;
    for m in path.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        for k in 0..=16 {
            let t = f64::from(k) / 16.0;
            // An arc is sampled on the arc (issue #42: its chord is not
            // where the tool goes - a G2 about the corner at R reads its
            // chord 0.30 inside R).
            let q = match m.arc.and_then(|a| a.point_at(m.from, m.to, t)) {
                Some(q) => q,
                None => Point3::new(
                    m.from.x + (m.to.x - m.from.x) * t,
                    m.from.y + (m.to.y - m.from.y) * t,
                    m.from.z + (m.to.z - m.from.z) * t,
                ),
            };
            if q.z >= 20.0 - EPS {
                continue;
            }
            let half = if q.z < 10.0 - EPS { 15.0 } else { 6.0 };
            let d = to_square(q.x, q.y, half);
            // The offset corners are chorded arcs: a few microns of
            // sag, far below any cut into the part.
            assert!(d >= clear, "{label}: feed at ({:.3}, {:.3}, {:.3}) is {d:.3} from the part section (R {R})", q.x, q.y, q.z);
            if q.z < 10.0 - EPS && d > R + 1.0 {
                outside += 1;
            }
        }
    }
    assert!(outside > 0, "{label}: the stock around the block is cleared");
}

#[test]
fn a_whole_part_pocket_clears_the_stock_around_the_part_never_its_section() {
    clears_around_the_part(pocket(20.0, 5.0), "2D Pocket", R - CHORD);
}

fn adaptive() -> OpParams {
    let mut p = operation::retype_params(&pocket(20.0, 5.0), OpType::Adaptive2D as usize);
    if let OpParams::Adaptive2D { top_z, bottom_z, stepdown, stepover, .. } = &mut p {
        *top_z = Height::fixed(20.0);
        *bottom_z = Height::fixed(5.0);
        *stepdown = 5.0;
        *stepover = 4.0;
    }
    assert_eq!(p.kind(), OpType::Adaptive2D);
    p
}

/// The region law under 2D Adaptive: the part's section is an island,
/// so no tool centre ever stands inside the part (the old reading put
/// the whole path inside the boss).
#[test]
fn a_whole_part_adaptive_clears_the_stock_around_the_part_never_its_section() {
    clears_around_the_part(adaptive(), "2D Adaptive", 0.0);
}

/// The full disc law under 2D Adaptive (issue #42): every feed keeps R
/// off the part, the block's outer corner rounded by a true G2 about it
/// at R - read on the arc, not on its chord.
#[test]
fn a_whole_part_adaptive_keeps_the_full_tool_radius_off_the_part() {
    clears_around_the_part(adaptive(), "2D Adaptive", R - CHORD);
}

#[test]
fn a_whole_part_pocket_whose_part_fills_the_stock_is_refused_by_name() {
    let world = world(cube(Point3::new(-25.0, -25.0, 0.0), Point3::new(25.0, 25.0, 20.0)), 50.0);
    let o = op(SrcRef::Entity("part".into()), pocket(20.0, 5.0), AreaControls::default());
    let err = run(&world, &o).expect_err("a part filling the stock leaves nothing to clear");
    assert!(err.contains("clears the stock around the part"), "{err}");
}

/// The verifier's backstop for issue #42, on the same block corner: the
/// generated path judged clean, its corner G2 at R clean, and the corner
/// cut straight - the arc's own chord (1.46 mm in) or a G1 tangent to a
/// 4.70 circle about the corner (0.30 mm in) - convicted at that move,
/// though both ends of each chord stand exactly R off the part.
#[test]
fn a_feed_chorded_across_the_parts_corner_reads_a_gouge() {
    use camcraft_app::cl::Move;
    use camcraft_app::geo2d::{Loop, Point2, Region};
    use camcraft_app::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};
    let sq = |h: f64| Loop::new(vec![Point2::new(-h, -h), Point2::new(h, -h), Point2::new(h, h), Point2::new(-h, h)]).expect("loop");
    // The part section as an island, the stock far off: only the
    // corner can convict.
    let region = Region::new(vec![sq(100.0), sq(15.0)]).expect("region");
    let tool = ToolGeom { radius: R, nose_radius: 0.0, flute_len: 40.0, shank_radius: R, shank_len: 40.0, holder_radius: 20.0, holder_len: 60.0 };
    let cx = VerifyContext {
        region: &region,
        stock: None,
        model: None,
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 20.0,
        clearance_z: 25.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    let gouges = |t: &Toolpath| -> Vec<usize> {
        verify(t, &cx).into_iter().filter_map(|w| if let Warning::Gouge { at } = w { Some(at) } else { None }).collect()
    };
    let p = |x: f64, y: f64| Point3::new(x, y, 5.0);
    let feed = |a: Point3, b: Point3| Move::new(MoveKind::Feed, a, b, Some(500.0));
    let corner = Point2::new(-15.0, 15.0);
    let around = |mid: Move| Toolpath::new(vec![feed(p(-20.0, 0.0), mid.from), mid, feed(mid.to, p(0.0, 20.0))]);
    let arc = feed(p(-20.0, 15.0), p(-15.0, 20.0)).with_arc(corner, false);
    assert_eq!(gouges(&around(arc)), Vec::<usize>::new(), "the G2 at R about the corner is the tangency law");
    let chord = feed(p(-20.0, 15.0), p(-15.0, 20.0));
    assert_eq!(gouges(&around(chord)), vec![1], "the arc's chord cuts 1.46 mm into the corner");
    // y - x = 30 + 4.70 sqrt 2: tangent to the 4.70 circle, ends on the
    // R lines past the corner.
    let c = 4.70 * std::f64::consts::SQRT_2 - 5.0;
    let shallow = feed(p(-20.0, 15.0 + c), p(-15.0 - c, 20.0));
    assert_eq!(gouges(&around(shallow)), vec![1], "a 0.30 mm intrusion mid-move convicts");

    // The generated path itself: clean, and with its corner G2 swapped
    // for the straight chord, convicted at exactly that move.
    let world = world(block_with_boss(), 50.0);
    let o = op(SrcRef::Entity("part".into()), adaptive(), AreaControls::default());
    let (path, _) = run(&world, &o).expect("generates");
    assert_eq!(gouges(&path), Vec::<usize>::new(), "the generated path keeps R off the part");
    let at = path
        .moves
        .iter()
        .position(|m| m.arc.is_some_and(|a| (a.center.x - corner.x).hypot(a.center.y - corner.y) < 1e-6) && m.from.z < 10.0 - EPS)
        .expect("the generated path rounds the block corner on a G2");
    let mut cut = path.clone();
    if let Some(m) = cut.moves.get_mut(at) {
        m.arc = None;
    }
    assert_eq!(gouges(&cut), vec![at], "the corner chorded straight convicts at its move");
}
