//! #3316: Pass start Inside / Outside, measured along each WHOLE level
//! with the shape mask (`common/shape_mask.rs`, the progressive rim
//! engagement meter). The second read's case: a stock 84 x 24 (its reach
//! for the tool, 100 x 40) around a round part of radius 15, a tool of
//! radius 4 and a step of 4. The ring there splits; a split piece carries
//! stock-side and part-side arcs, and Outside must still cut no move at
//! the full tool width, never cross the part wall and cut the band whole.
//!
//! Gate map: covers camcraft_motion::machining2d (pass_start, level_rings,
//! level_order, ring_side, pocket, pocket_avoid, ladder_links) and
//! camcraft_geometry::geo2d (clip_line).
//! Provenance: our own regression of the second read on PR #3353; no
//! vendor material consulted.

mod common;
#[path = "pass_start_levels/one_ring_set.rs"]
mod one_ring_set;
#[path = "pass_start_levels/ring_links.rs"]
mod ring_links;
#[path = "pass_start_levels/pocket_outside.rs"]
mod pocket_outside;

use camcraft_motion::cl::Toolpath;
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{
    CornerRelief, PASS_START_NOTE, PassStart, PocketParams, PocketStrategy, StartSide, pocket_avoid_started,
    pocket_started, pocket_started_on,
};
use camcraft_motion::tools::ToolGeom;
use camcraft_motion::verify::{AVOID_VERIFY_TOL, AvoidItem, AvoidSpec, Cutter, avoid_hits};
use common::shape_mask::{CutDepth, MaskSpec, ShapeMask};

const R: f64 = 4.0;
const STEP: f64 = 4.0;
const Z: f64 = -2.0;

fn region(loops: Vec<Vec<Point2>>) -> Region {
    let loops = loops.into_iter().map(|l| Loop::new(l).expect("loop")).collect();
    Region::new(loops).expect("region")
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point2> {
    vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]
}

fn circle(r: f64) -> Vec<Point2> {
    (0..256).map(|k| {
        let a = k as f64 * std::f64::consts::TAU / 256.0;
        Point2::new(r * a.cos(), r * a.sin())
    })
    .collect()
}

fn params() -> PocketParams {
    PocketParams {
        tool_diameter: 2.0 * R,
        stepover: STEP,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        retract_z: Some(2.0),
        top_z: 0.0,
        bottom_z: Z,
        stepdown: 2.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Waterline,
        entry: Entry3D::Helix,
        helix_radius: 1.5,
        optimal_load: 0.25,
        blend_turns: false,
        blend_radius: None,
        links: Links::PLANAR,
    }
}

/// The stock, the part, and the area the tool's disc may sweep: the
/// stock grown by the tool's diameter (the Outside reach) less the part.
fn band() -> (Region, Region, Region) {
    let stock = region(vec![rect(-42.0, -12.0, 42.0, 12.0)]);
    let part = region(vec![circle(15.0)]);
    let reach = stock.offset(2.0 * R).expect("reach").difference(&part).expect("band");
    (stock, part, reach)
}

fn mask<'a>(name: &'a str, expected: &'a Region, path: &'a Toolpath) -> ShapeMask {
    ShapeMask::evaluate(&MaskSpec {
        name,
        expected,
        path,
        tool_diameter: 2.0 * R,
        cell: 0.1,
        depth: CutDepth::NearZ { z: Z, tol: 1e-3 },
        diff_dir: None,
    })
    .expect("mask")
}

/// The widest cut the start-side plan may take: one step and a quarter
/// radius (5 mm). A full-width cut reads 2R (8 mm) on the meter; the
/// old order of whole loops read 7.97 mm on the band, along a part-side
/// arc of a split piece.
const ENGAGEMENT: f64 = STEP + 0.25 * R;

/// The three laws on one path: nothing outside the disc's area (the part
/// wall is never crossed), the material cut whole, and no cut wider than
/// [`ENGAGEMENT`] along the whole level.
fn check(name: &str, reach: &Region, material: &Region, path: &Toolpath, max_uncut: f64) {
    mask(name, reach, path).assert_no_overcut();
    let m = mask(name, material, path);
    let got = m.metrics();
    eprintln!("{name}: max cut width {:.3} at {:?}, uncut {:.3} mm2", got.max_cut_width, got.worst_cut_point, got.uncut_area);
    m.assert_bounded_leftover(max_uncut, None);
    m.assert_max_engagement(ENGAGEMENT);
}

/// #3316 second read: Outside on the split band cuts no move at the full
/// tool width along the whole level, crosses no wall (no overcut) and
/// cuts the band whole (bounded leftover).
#[test]
fn outside_band_split_by_the_part_cuts_no_move_at_full_width() {
    let (stock, part, reach) = band();
    let path = pocket_started(&reach, &params(), &[], &CornerRelief::default(), PassStart::Outside).expect("plan");
    check("pass_start_outside_band", &reach, &stock.difference(&part).expect("material"), &path, 1.0);
}

/// The mirror: a ring of stock round a bore that is already open (the
/// hole is air), the disc's area grown into the bore by the diameter.
/// Inside starts at the bore and walks out, one step a ring.
#[test]
fn inside_from_an_open_bore_cuts_no_move_at_full_width() {
    let material = region(vec![circle(40.0), circle(15.0)]);
    let reach = region(vec![circle(40.0), circle(15.0 - 2.0 * R)]);
    let path = pocket_started(&reach, &params(), &[], &CornerRelief::default(), PassStart::Inside).expect("plan");
    // The round wall's edge cells read 7.3 mm2 uncut on every order (Auto
    // too): raster, not leftover; no interior cell is uncut.
    check("pass_start_inside_bore", &reach, &material, &path, 8.0);
}

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let p = |x, y, z| Point3::new(x, y, z);
    let v = |i: usize| p([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)]
        .into_iter()
        .flatten()
        .collect()
}

/// Outside with a "Do not touch" pin in the band (`pocket_avoid`): the
/// keep-out's edge is a wall, cut last; no move at the full tool width
/// and nothing into the protected pin.
#[test]
fn outside_band_with_a_do_not_touch_pin_cuts_no_move_at_full_width() {
    let (stock, part, reach) = band();
    let spec = AvoidSpec {
        items: vec![AvoidItem { label: "pin".into(), tris: box_facets([26.0, -3.0, -10.0], [30.0, 3.0, 5.0]), edges: vec![] }],
        cutter: Cutter::flat(&ToolGeom {
            radius: R,
            nose_radius: 0.0,
            flute_len: 10.0,
            shank_radius: R,
            shank_len: 20.0,
            holder_radius: 12.0,
            holder_len: 20.0,
        }),
        clearance: 0.5,
    };
    let path = pocket_avoid_started(&reach, &params(), &spec, &CornerRelief::default(), PassStart::Outside).expect("plan");
    let hits = avoid_hits(&path, &spec, AVOID_VERIFY_TOL).expect("verify");
    assert!(hits.is_empty(), "the pin is touched: {hits:?}");
    let pin = region(vec![rect(26.0, -3.0, 30.0, 3.0)]);
    let expected = stock.difference(&part).and_then(|m| m.difference(&pin)).expect("material");
    mask("pass_start_outside_pin", &reach, &path).assert_no_overcut();
    let m = mask("pass_start_outside_pin", &expected, &path);
    let got = m.metrics();
    eprintln!("outside pin: max cut width {:.3} at {:?}, uncut {:.3} mm2", got.max_cut_width, got.worst_cut_point, got.uncut_area);
    m.assert_max_engagement(ENGAGEMENT);
    // Only the keep-out's own band (the pin grown by its clearance and
    // pad) may stay; the rest of the band is cut whole.
    let kept_out = pin.offset(1.5).expect("keep-out band");
    m.assert_bounded_leftover(60.0, Some(&kept_out));
}


/// The notes of one plan that the op page shows as warnings.
fn warnings(reach: &Region, side: StartSide) -> Vec<String> {
    let path = pocket_started_on(reach, &params(), &[], &CornerRelief::default(), &side).expect("plan");
    path.notes.into_iter().filter(|n| n.starts_with(PASS_START_NOTE)).collect()
}

/// #3316: each case that starts a level with a cut of the full tool
/// width is named on the path; the open cases say nothing.
#[test]
fn the_wall_first_and_narrow_cases_are_named_on_the_path() {
    let (stock, _, reach) = band();
    let on = |start, around_part| StartSide { start, stock: Some(stock.clone()), around_part };
    assert_eq!(warnings(&reach, on(PassStart::Outside, true)), Vec::<String>::new(), "Outside on a band starts in air");
    let part = warnings(&reach, on(PassStart::Inside, true));
    assert!(part.len() == 1 && part[0].contains("the part wall"), "Inside on a band: {part:?}");
    // A bore that is already open: Inside starts in air, Outside on the wall.
    let ring = region(vec![circle(40.0), circle(15.0)]);
    let bore = region(vec![circle(40.0), circle(15.0 - 2.0 * R)]);
    let side = |start| StartSide { start, stock: Some(ring.clone()), around_part: false };
    assert_eq!(warnings(&bore, side(PassStart::Inside)), Vec::<String>::new(), "Inside from an open bore");
    let wall = warnings(&bore, side(PassStart::Outside));
    assert!(wall.len() == 1 && wall[0].contains("outer edge"), "Outside on a closed pocket: {wall:?}");
    // A pocket with an island: Inside starts on the island.
    let pocket = region(vec![rect(-30.0, -30.0, 30.0, 30.0), rect(-5.0, -5.0, 5.0, 5.0)]);
    let block = region(vec![rect(-50.0, -50.0, 50.0, 50.0)]);
    let island = warnings(&pocket, StartSide { start: PassStart::Inside, stock: Some(block.clone()), around_part: false });
    assert!(island.len() == 1 && island[0].contains("island wall"), "Inside round an island: {island:?}");
    // A ring part: the stock inside it is a part of its own. Outside
    // starts it on its own edge, a wall in the stock; Inside has no hole
    // there and cuts it from the middle, as Auto.
    let square = region(vec![rect(-40.0, -40.0, 40.0, 40.0)]);
    let annulus = region(vec![circle(25.0), circle(15.0)]);
    let ringed = square.offset(2.0 * R).and_then(|r| r.difference(&annulus)).expect("reach");
    let on = |start| StartSide { start, stock: Some(square.clone()), around_part: true };
    let inner = warnings(&ringed, on(PassStart::Outside));
    assert!(inner.iter().any(|n| n.contains("outer edge of this area is a wall")), "the enclosed stock: {inner:?}");
    let core = warnings(&ringed, on(PassStart::Inside));
    assert!(core.iter().any(|n| n.contains("1 part(s) of this area have no hole")), "the enclosed stock: {core:?}");
    // A horseshoe part whose mouth is narrower than the tool: the stock in
    // its cavity is reached from the start side by no ring, so its first
    // cut takes the full width - named.
    let ring = region(vec![rect(-20.0, -20.0, 20.0, 20.0), rect(-12.0, -12.0, 12.0, 12.0)]);
    let shoe = ring.difference(&region(vec![rect(10.0, -1.0, 21.0, 1.0)])).expect("horseshoe");
    let cavity = square.offset(2.0 * R).and_then(|r| r.difference(&shoe)).expect("reach");
    let narrow = warnings(&cavity, on(PassStart::Outside));
    assert!(narrow.iter().any(|n| n.contains("full tool width where the area is narrower")), "the cavity: {narrow:?}");
}

/// The start-side plan's walks meet only at shared points: the
/// verifier's crossing law finds no two PLANAR feeds that cross (ring
/// piece ends on the walls, lead-in chords, joins). The helix entries
/// are the entry law's: two re-entries in one area can bore side by
/// side (#3316 follow-up), so pairs with a descending move are not this
/// plan's.
#[test]
fn the_start_side_plan_posts_no_crossing_feeds() {
    let (_, _, reach) = band();
    let mut fine = params();
    fine.stepover = 3.0;
    for (start, p) in [(PassStart::Outside, params()), (PassStart::Inside, params()), (PassStart::Outside, fine), (PassStart::Inside, fine)] {
        let path = pocket_started(&reach, &p, &[], &CornerRelief::default(), start).expect("plan");
        let planar = |i: usize| path.moves.get(i).is_some_and(|m| (m.from.z - m.to.z).abs() < 1e-9);
        let crossed: Vec<[usize; 2]> = camcraft_motion::verify::crossed_pairs(&path, 0.01)
            .into_iter()
            .filter(|pr| planar(pr[0]) && planar(pr[1]))
            .collect();
        assert!(crossed.is_empty(), "{start:?} step {}: {crossed:?}", p.stepover);
    }
}

/// What no tool of radius `r` reaches in `material` (its inside corners):
/// the material less its opening by the tool disc, grown by a cell.
fn unreachable(material: &Region, r: f64) -> Region {
    let opened = material.offset(-r).and_then(|o| o.offset(r)).expect("opening");
    material.difference(&opened).and_then(|u| u.offset(0.2)).expect("unreachable")
}

/// The leftover law on one fixture: the plan cuts every place the tool
/// reaches (an Auto plan's area), none of it stays standing.
fn cuts_whole(name: &str, reach: &Region, material: &Region, side: StartSide) {
    let path = pocket_started_on(reach, &params(), &[], &CornerRelief::default(), &side).expect("plan");
    mask(name, reach, &path).assert_no_overcut();
    let m = mask(name, material, &path);
    let got = m.metrics();
    eprintln!("{name}: uncut {:.3} mm2, {} interior uncut cell(s)", got.uncut_area, got.interior_uncut_cells);
    let allowed = unreachable(material, R);
    m.assert_bounded_leftover(allowed.area().abs() + 8.0, Some(&allowed));
}

/// #3316 second read of 9939a6238: material nested in a hole (the stock
/// inside a ring part) is cut whole, Outside and Inside.
#[test]
fn the_stock_inside_a_ring_part_is_cut_whole() {
    let square = region(vec![rect(-40.0, -40.0, 40.0, 40.0)]);
    let annulus = region(vec![circle(25.0), circle(15.0)]);
    let reach = square.offset(2.0 * R).and_then(|r| r.difference(&annulus)).expect("reach");
    let material = square.difference(&annulus).expect("material");
    for start in [PassStart::Outside, PassStart::Inside] {
        let side = StartSide { start, stock: Some(square.clone()), around_part: true };
        cuts_whole(&format!("pass_start_ringed_{}", start.label()), &reach, &material, side);
    }
}

/// A closed pocket with a hollow boss whose bore is cleared to the same
/// floor: the bore is material nested in the boss's hole. Outside and
/// Inside cut the pocket and the bore whole.
#[test]
fn a_pocket_with_a_hollow_boss_is_cut_whole() {
    let pocket = region(vec![rect(-40.0, -30.0, 40.0, 30.0), circle(18.0), circle(10.0)]);
    let block = region(vec![rect(-60.0, -50.0, 60.0, 50.0)]);
    for start in [PassStart::Outside, PassStart::Inside] {
        let side = StartSide { start, stock: Some(block.clone()), around_part: false };
        cuts_whole(&format!("pass_start_hollow_boss_{}", start.label()), &pocket, &pocket, side);
    }
}
