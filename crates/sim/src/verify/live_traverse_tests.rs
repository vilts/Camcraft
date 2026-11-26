//! #3077: unit pins of the live-tool first-traverse law (`live_traverse`) - our own cases; no vendor material consulted.

use super::*;

const TOL: f64 = 1e-6;

fn b(lo: [f64; 3], hi: [f64; 3]) -> FixtureBox {
    FixtureBox { min: Point3::new(lo[0], lo[1], lo[2]), max: Point3::new(hi[0], hi[1], hi[2]) }
}

/// A plain Ø4 drill: 20 of flute, no shank or holder wider than it.
fn drill() -> ToolGeom {
    ToolGeom { radius: 2.0, nose_radius: 0.0, flute_len: 20.0, shank_radius: 2.0, shank_len: 10.0, holder_radius: 0.0, holder_len: 0.0 }
}

/// A start at radius 15 on +X, Z 10, a Ø4 drill: the swept solid is X 15
/// and out, Y -2..2, Z 8 and up.
fn start() -> LiveStart {
    LiveStart { start: Point3::new(15.0, 0.0, 10.0), spindle: [0.0, 0.0], tool: drill() }
}

fn steel(s: &LiveStart, turning: &[FixtureBox], fixed: &[FixtureBox]) -> Result<Option<LiveHit>, LiveUnjudged> {
    live_first_traverse(s, &LiveSolids { turning, fixed, ..LiveSolids::default() }, TOL, 0.0)
}

fn work_box(s: &LiveStart, bx: FixtureBox, excess: f64) -> Result<Option<LiveHit>, LiveUnjudged> {
    live_first_traverse(s, &LiveSolids { work_boxes: &[bx], ..LiveSolids::default() }, TOL, excess)
}

#[test]
fn a_tailstock_over_the_hole_is_in_the_way_and_a_chuck_under_it_is_not() {
    let chuck = b([-40.0, -40.0, -110.0], [40.0, 40.0, -80.0]);
    let tail = b([-30.0, -30.0, 25.0], [30.0, 30.0, 60.0]);
    assert_eq!(steel(&start(), &[chuck], &[]), Ok(None));
    let hit = steel(&start(), &[chuck], &[tail]);
    let Ok(Some(LiveHit::Steel { index: 0, turning: false, along_mm, .. })) = hit else { panic!("{hit:?}") };
    assert!((along_mm - 30.0 * 2f64.sqrt()).abs() < 1e-9, "{along_mm}");
    // A thin tailstock centre inside the start's radius is passed.
    let centre = b([-5.0, -5.0, 25.0], [5.0, 5.0, 60.0]);
    assert_eq!(steel(&start(), &[], &[centre]), Ok(None));
    // Fixed steel is read revolved: a block on the far side counts.
    let far = b([-30.0, -10.0, 20.0], [-20.0, 10.0, 30.0]);
    assert!(matches!(steel(&start(), &[], &[far]), Ok(Some(LiveHit::Steel { turning: false, .. }))));
}

#[test]
fn steel_that_turns_is_read_in_the_station_band() {
    let jaw = |top: f64| b([-30.0, -30.0, -40.0], [30.0, 30.0, top]);
    // The band starts at Z 8 (the tip at Z 10, less the drill's radius).
    assert_eq!(steel(&start(), &[jaw(7.9)], &[]), Ok(None));
    assert!(matches!(steel(&start(), &[jaw(8.1)], &[]), Ok(Some(LiveHit::Steel { turning: true, .. }))));
    // A jaw on the far side of the spindle stands out of the station's
    // half-plane: C turned it there before X and Z move.
    let far = b([-30.0, -10.0, -40.0], [-20.0, 10.0, 30.0]);
    assert_eq!(steel(&start(), &[far], &[]), Ok(None));
    // A jaw 3 mm beside the station's line (the drill is 2 mm each side).
    let beside = b([20.0, 3.0, -40.0], [30.0, 10.0, 30.0]);
    assert_eq!(steel(&start(), &[beside], &[]), Ok(None));
    let touching = b([20.0, 1.9, -40.0], [30.0, 10.0, 30.0]);
    assert!(matches!(steel(&start(), &[touching], &[]), Ok(Some(LiveHit::Steel { .. }))));
}

#[test]
fn a_face_that_only_touches_the_first_points_radius_passes_unless_the_model_is_coarse() {
    // A flange whose outer face is X 15 exactly, over the hole: the tip
    // slides down its face and never enters it.
    let flange = b([10.0, -5.0, 13.0], [15.0, 5.0, 15.0]);
    assert_eq!(work_box(&start(), flange, 0.0), Ok(None));
    // 0.01 mm past the first point's radius is inside the swept solid.
    let proud = b([10.0, -5.0, 13.0], [15.01, 5.0, 15.0]);
    let hit = work_box(&start(), proud, 0.0);
    let Ok(Some(LiveHit::Work { index: 0, along_mm, z_mm })) = hit else { panic!("{hit:?}") };
    assert!((along_mm - 15.01).abs() < 1e-9 && (13.0..=15.0).contains(&z_mm), "{along_mm} {z_mm}");
    // #1053: past the tolerance the excess reads untightened.
    assert_eq!(work_box(&start(), flange, 0.05), Ok(None));
}

#[test]
fn a_coarse_models_excess_tightens_the_tolerance_while_one_is_left() {
    // A face 0.005 proud: inside a 0.01 tolerance, past it tightened by
    // a 0.007 excess (0.003 left).
    let proud = b([10.0, -5.0, 13.0], [15.005, 5.0, 15.0]);
    let at = |excess: f64| live_first_traverse(&start(), &LiveSolids { work_boxes: &[proud], ..LiveSolids::default() }, 0.01, excess);
    assert_eq!(at(0.0), Ok(None));
    assert!(matches!(at(0.007), Ok(Some(LiveHit::Work { .. }))));
    // The steel is never a model: no excess tightens it.
    let s = live_first_traverse(&start(), &LiveSolids { turning: &[proud], ..LiveSolids::default() }, 0.01, 0.007);
    assert_eq!(s, Ok(None));
}

#[test]
fn the_shank_counts_only_from_where_it_stands_behind_the_tip() {
    // A Ø4 drill, 20 of flute, then a Ø12 shank: a lump at Y 3..5 out to
    // X 30 is beside the flute (|y| <= 2) and short of the shank (X 35).
    let wide = ToolGeom { shank_radius: 6.0, ..drill() };
    let s = LiveStart { tool: wide, ..start() };
    let lump = b([10.0, 3.0, -5.0], [30.0, 5.0, 20.0]);
    assert_eq!(work_box(&s, lump, 0.0), Ok(None));
    // Out to X 36 it meets the shank's band.
    let long = b([10.0, 3.0, -5.0], [36.0, 5.0, 20.0]);
    assert!(matches!(work_box(&s, long, 0.0), Ok(Some(LiveHit::Work { .. }))));
    // Past the whole stack the widest band runs on: the turret.
    let beyond = b([60.0, 4.0, -5.0], [70.0, 5.0, 20.0]);
    assert!(matches!(work_box(&s, beyond, 0.0), Ok(Some(LiveHit::Work { .. }))));
}

#[test]
fn the_spindle_line_off_the_origin_moves_the_band() {
    // The start at radius 15 about X 100: the band is X 115 and out.
    let off = LiveStart { start: Point3::new(115.0, 0.0, 10.0), spindle: [100.0, 0.0], ..start() };
    let near = b([110.0, -5.0, 0.0], [120.0, 5.0, 30.0]);
    assert!(matches!(steel(&off, &[near], &[]), Ok(Some(LiveHit::Steel { turning: true, .. }))));
    let world_origin = b([10.0, -5.0, 0.0], [20.0, 5.0, 30.0]);
    assert_eq!(steel(&off, &[world_origin], &[]), Ok(None));
}

#[test]
fn a_square_bar_is_read_at_the_face_the_station_looks_at() {
    // A 44 square bar (corner radius 31.1): a station on its face's
    // normal at radius 27 clears it; one on its diagonal at 27 does
    // not, as its corner stands at 31.1 there.
    let bar = b([-22.0, -22.0, -40.0], [22.0, 22.0, 20.0]);
    assert_eq!(work_box(&LiveStart { start: Point3::new(27.0, 0.0, 10.0), ..start() }, bar, 0.0), Ok(None));
    let d = 27.0 / 2f64.sqrt();
    let hit = work_box(&LiveStart { start: Point3::new(d, d, 10.0), ..start() }, bar, 0.0);
    assert!(matches!(hit, Ok(Some(LiveHit::Work { index: 0, .. }))), "{hit:?}");
}

#[test]
fn a_flange_over_the_hole_is_read_where_its_facet_crosses_the_band() {
    // One facet from (40, 0, 0) to (0, 0, 20) and (0, 1, 20): its
    // vertices in the band are inside the start's radius, but where it
    // crosses Z 8 it reaches X 24.
    let t = Triangle::new(Point3::new(40.0, 0.0, 0.0), Point3::new(0.0, 0.0, 20.0), Point3::new(0.0, 1.0, 20.0));
    let work: &[&[Triangle]] = &[&[t]];
    let hit = live_first_traverse(&start(), &LiveSolids { work, ..LiveSolids::default() }, TOL, 0.0);
    let Ok(Some(LiveHit::Work { index: 0, along_mm, z_mm })) = hit else { panic!("{hit:?}") };
    // (The band's floor stands the touch tolerance over Z 8.)
    assert!((along_mm - 24.0).abs() < 1e-5 && (z_mm - 8.0).abs() < 1e-5, "{along_mm} {z_mm}");
    // A plain bar face of radius 10 is inside the start's radius.
    let bar = Triangle::new(Point3::new(10.0, 0.0, -80.0), Point3::new(10.0, 0.0, 20.0), Point3::new(0.0, 10.0, 20.0));
    let work: &[&[Triangle]] = &[&[bar]];
    assert_eq!(live_first_traverse(&start(), &LiveSolids { work, ..LiveSolids::default() }, TOL, 0.0), Ok(None));
}

#[test]
fn a_start_or_a_tool_that_cannot_be_read_is_not_judged() {
    let mut s = start();
    s.start.z = f64::NAN;
    assert_eq!(steel(&s, &[], &[]), Err(LiveUnjudged));
    let mut s = start();
    s.tool.shank_radius = f64::INFINITY;
    assert_eq!(steel(&s, &[], &[]), Err(LiveUnjudged));
    let mut s = start();
    s.tool.flute_len = -1.0;
    assert_eq!(steel(&s, &[], &[]), Err(LiveUnjudged));
    // A start on the spindle line has no station direction.
    let on_axis = LiveStart { start: Point3::new(0.0, 0.0, 10.0), ..start() };
    assert_eq!(steel(&on_axis, &[], &[]), Err(LiveUnjudged));
    assert_eq!(live_first_traverse(&start(), &LiveSolids::default(), TOL, f64::NAN), Err(LiveUnjudged));
}
