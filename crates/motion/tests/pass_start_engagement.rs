//! #3567 / #3748: a later pass of a 2D Offset "Pass starts at: Outside"
//! plan must not plunge or helix into uncut stock at a dead end. The case
//! is the hand trace of PR #3353: a 100 x 40 stock band around a round
//! part of radius 15, tool D8, stepover 4.
//!
//! The check is a 2-D material sim on a 0.25 mm grid. A later ENTRY is
//! each descending cutting move after the first floor feed. #3748 retired
//! the automatic dead-end ramp of #3567: round a part (the stock known,
//! #3744) the rings of one set link in the plane and every later entry
//! lands in cleared stock, with no ramp exemption. A Helix, Ramp or Plunge
//! that the user selects is used as selected and named once; the
//! recommended Arc enters from the air with no helix or ramp.
//!
//! Gate map: covers camcraft_motion::machining2d (pass_start,
//! level_rings, level_order, air_entry, descent, entry_warn). Provenance: our
//! own regression of the review of PR #3353; the radial-engagement
//! measure of the public milling handbooks; no vendor material consulted.

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::{DEFAULT_RAMP_ANGLE, Links};
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::{
    CornerRelief, PassStart, PocketParams, PocketStrategy, StartSide, OUTSIDE_AIR_REFUSAL, entry_advice, entry_warnings, pocket_started,
    pocket_started_on, rings_fallbacks,
};
use camcraft_motion::verify::Warning;

#[path = "pass_start_engagement/grid.rs"]
mod grid;
use grid::{Grid, R, seg_dist, sweep};

const STEP: f64 = 4.0;
const PART: f64 = 15.0;

/// The ramp angle the band's entry takes: the op sets none, so the ramp
/// fallback's default (`links::DEFAULT_RAMP_ANGLE`).
const RAMP_DEG: f64 = DEFAULT_RAMP_ANGLE;

/// How much steeper (deg) than [`RAMP_DEG`] a move may be and still be a
/// ramp: float noise only. A plunge is 90 deg.
const RAMP_TOL_DEG: f64 = 0.05;

/// How near (mm) to a planar floor feed a ramp's centre must stay to lie
/// on the pass path: the walks are fitted to arcs within microns, the
/// ramp rides the raw points.
const ON_PATH: f64 = 0.02;

fn lp(pts: Vec<Point2>) -> Loop {
    let Ok(l) = Loop::new(pts) else { panic!("fixture loop") };
    l
}

fn rect(hx: f64, hy: f64) -> Loop {
    lp(vec![Point2::new(-hx, -hy), Point2::new(hx, -hy), Point2::new(hx, hy), Point2::new(-hx, hy)])
}

fn circle(r: f64) -> Loop {
    let n = 256;
    lp((0..n)
        .map(|k| {
            let a = f64::from(k) * std::f64::consts::TAU / f64::from(n);
            Point2::new(r * a.cos(), r * a.sin())
        })
        .collect())
}

fn params(climb: bool) -> PocketParams {
    PocketParams {
        blend_radius: None,
        tool_diameter: 2.0 * R,
        stepover: STEP,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -3.0,
        stepdown: 5.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb,
        strategy: PocketStrategy::Waterline,
        entry: Entry3D::Helix,
        helix_radius: 1.6,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links: Links::PLANAR,
    }
}

fn is_floor(m: &Move, floor: f64) -> bool {
    m.kind != MoveKind::Rapid && (m.from.z - floor).abs() < 1e-9 && (m.to.z - floor).abs() < 1e-9
}

/// The XY chords of every planar floor feed of `path`: the pass paths.
fn floor_chords(path: &Toolpath, floor: f64) -> Vec<(Point2, Point2)> {
    path.moves.iter().filter(|m| is_floor(m, floor)).flat_map(|m| m.chords(0.005)).collect()
}

/// The steepest descent (deg) of a move, over its 3-D segments.
fn steepness(m: &Move) -> f64 {
    m.segments3(0.005)
        .iter()
        .map(|(a, b)| (a.z - b.z).abs().atan2((b.x - a.x).hypot(b.y - a.y)).to_degrees())
        .fold(0.0, f64::max)
}

/// #3567: a ramp along the pass - a feed that is not planar, no steeper
/// than the ramp angle, whose centre stays on a pass path (a planar
/// floor feed) all along. A plunge is too steep; a helix leaves the path.
fn is_ramp(m: &Move, paths: &[(Point2, Point2)]) -> bool {
    m.kind == MoveKind::Feed
        && steepness(m) <= RAMP_DEG + RAMP_TOL_DEG
        && sweep(m).iter().all(|&q| paths.iter().any(|&(u, v)| seg_dist(q, u, v) <= ON_PATH))
}

/// The stock (mm^2) each later entry cut: each cutting move below the
/// top, after the first floor feed, that is not a floor feed, read on the
/// material grid of `stock`, in path order.
fn entries(path: &Toolpath, stock: &Region, floor: f64, top: f64) -> Vec<f64> {
    let mut g = Grid::new(stock);
    let (mut seen, mut out) = (false, Vec::new());
    for m in path.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        if m.from.z.min(m.to.z) >= top - 1e-9 {
            continue;
        }
        let planar = is_floor(m, floor);
        let got: f64 = sweep(m).into_iter().map(|q| g.cut(q)).sum();
        if seen && !planar {
            out.push(got);
        }
        seen |= planar;
    }
    out
}

/// The stock (mm^2) that the later entries cut after the first floor
/// feed: each entry move, a ramp too (#3748: no ramp exemption).
fn late_entry_cut(path: &Toolpath, stock: &Region, floor: f64, top: f64) -> f64 {
    entries(path, stock, floor, top).iter().sum()
}

/// The band round the part with the stock known (#3744: one ring set),
/// from `start` with the lead-in `entry`.
fn start_side_path(start: PassStart, entry: Entry3D) -> (Toolpath, Region, PocketParams) {
    let (region, stock) = band();
    let mut p = params(true);
    p.entry = entry;
    // A Ramp entry with an angle ramps along the first cut (the chord
    // from the tool's XY over the part is blocked).
    if entry == Entry3D::Ramp {
        p.links.approach.ramp_angle = RAMP_DEG;
    }
    let side = StartSide { start, stock: Some(stock.clone()), around_part: true };
    let path = match pocket_started_on(&region, &p, &[], &CornerRelief::default(), &side) {
        Ok(t) => t,
        Err(e) => panic!("{e:?}"),
    };
    // The start-side plan is kept, not swapped for the Auto order.
    assert_eq!(rings_fallbacks(&path.notes), 0, "{:?}", path.notes);
    (path, stock, p)
}

/// A helix: a feed on an arc that descends.
fn is_helix(m: &Move) -> bool {
    m.arc.is_some() && m.from.z - m.to.z > 1e-9
}

/// A descending move that ends below the top `top`.
fn descends_below(m: &Move, top: f64) -> bool {
    m.to.z < top - 1e-9 && m.from.z - m.to.z > 1e-9
}

/// #3316 pin: a new pass of a split level starts with a lead-in from a
/// piece already cut - never a plunge, helix or ramp at the far wall into
/// uncut stock. #3748: no ramp exemption; the selected Helix is the first
/// entry only.
#[test]
fn outside_entries_after_the_first_land_in_cleared_stock() {
    let (path, stock, p) = start_side_path(PassStart::Outside, Entry3D::Helix);
    let late = late_entry_cut(&path, &stock, p.bottom_z, p.top_z);
    assert!(late < 1.0, "the later entries cut {late:.2} mm^2 of uncut stock");
    // The pin cannot pass empty: the band is cut at the floor.
    let floor = path.moves.iter().filter(|m| is_floor(m, p.bottom_z)).count();
    assert!(floor > 10, "only {floor} floor feeds, so the pin proves nothing");
}

/// #3567 review item 2: the slope law of `is_ramp`. A straight descending
/// feed on the pass path is a ramp only at or under the ramp angle; a
/// steeper one (60 deg, vertical) counts like a plunge.
#[test]
fn a_steep_straight_feed_is_not_a_ramp() {
    use camcraft_motion::cl::PathBuilder;
    use camcraft_motion::geometry::Point3;
    let floor = -3.0;
    let run = 3.0 / 60f64.to_radians().tan();
    let shallow = 3.0 / RAMP_DEG.to_radians().tan();
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, 10.0));
    b.push(MoveKind::Rapid, Point3::new(0.0, 0.0, 0.0), None);
    b.push(MoveKind::Feed, Point3::new(run, 0.0, floor), Some(300.0));
    b.push(MoveKind::Feed, Point3::new(run, 0.0, 0.0), Some(300.0));
    b.push(MoveKind::Feed, Point3::new(run, 0.0, floor), Some(300.0));
    b.push(MoveKind::Rapid, Point3::new(run, 0.0, 0.0), None);
    b.push(MoveKind::Feed, Point3::new(run + shallow, 0.0, floor), Some(300.0));
    b.push(MoveKind::Feed, Point3::new(-20.0, 0.0, floor), Some(1000.0));
    b.push(MoveKind::Feed, Point3::new(200.0, 0.0, floor), Some(1000.0));
    let path = b.finish();
    let paths = floor_chords(&path, floor);
    let kinds: Vec<bool> =
        path.moves.iter().filter(|m| m.kind == MoveKind::Feed && !is_floor(m, floor)).map(|m| is_ramp(m, &paths)).collect();
    // 60 deg down, vertical up, vertical down: not ramps; 3 deg down: a ramp.
    assert_eq!(kinds, vec![false, false, false, true], "{kinds:?}");
}

/// #3748 pin 3: no automatic ramp along the pass appears on a start-side
/// pass. The user's Helix runs (Inside and Outside), and no descending
/// feed is a ramp along the pass.
#[test]
fn start_side_pass_has_no_automatic_ramp() {
    for start in [PassStart::Outside, PassStart::Inside] {
        let (path, _, p) = start_side_path(start, Entry3D::Helix);
        let paths = floor_chords(&path, p.bottom_z);
        let ramps = path
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed && m.arc.is_none() && descends_below(m, p.top_z))
            .filter(|m| is_ramp(m, &paths))
            .count();
        assert_eq!(ramps, 0, "{start:?}: {ramps} ramp move(s) along the pass: {:?}", path.notes);
        assert!(path.moves.iter().any(is_helix), "{start:?}: the selected helix did not run: {:?}", path.notes);
    }
}

/// #3748 pin 1: a selected Helix, Ramp or Plunge on Outside is used as
/// selected, and the path names it exactly once, with the recommended Arc.
#[test]
fn a_selected_entry_is_kept_and_named_once() {
    for kind in [Entry3D::Helix, Entry3D::Ramp, Entry3D::Plunge] {
        let (path, _, p) = start_side_path(PassStart::Outside, kind);
        let Some(advice) = entry_advice(PassStart::Outside, kind) else { panic!("{kind:?}: no advice") };
        let named = path.notes.iter().filter(|n| n.contains(advice)).count();
        assert_eq!(named, 1, "{kind:?}: the advice is named {named} time(s): {:?}", path.notes);
        let mut down = path.moves.iter().filter(|m| descends_below(m, p.top_z));
        let kept = match kind {
            Entry3D::Helix => path.moves.iter().any(is_helix),
            Entry3D::Ramp => down.any(|m| m.kind == MoveKind::Feed && m.arc.is_none()),
            // The plunge stands in the stock box, not in the air outside it.
            _ => down.any(|m| m.kind == MoveKind::Plunge && m.to.x.abs() < 50.0 && m.to.y.abs() < 20.0),
        };
        assert!(kept, "{kind:?}: the selected entry did not run: {:?}", path.notes);
    }
}

/// #3748 pin 2: the Arc entry (the recommended one) on Outside has no
/// helix and no ramp: every descent below the top is a plunge, the first
/// one in the air outside the stock box, and no advice is written.
#[test]
fn the_arc_entry_has_no_helix_or_ramp_and_no_advice() {
    let (path, _, p) = start_side_path(PassStart::Outside, Entry3D::Arc);
    assert!(!path.moves.iter().any(is_helix), "a helix move: {:?}", path.notes);
    let descents: Vec<&Move> = path.moves.iter().filter(|m| descends_below(m, p.top_z)).collect();
    let Some(first) = descents.first() else { panic!("no descent") };
    let out = (first.to.x.abs() - 50.0).max(first.to.y.abs() - 20.0);
    assert!(first.kind == MoveKind::Plunge && out >= R - 1e-6, "the first descent is not a plunge in the air: {first:?}");
    let ramps: Vec<&&Move> = descents.iter().filter(|m| m.kind != MoveKind::Plunge).collect();
    assert!(ramps.is_empty(), "a descent that is not a plunge (a ramp): {ramps:?}");
    assert!(!path.notes.iter().any(|n| n.contains("is used as selected")), "{:?}", path.notes);
}

fn band() -> (Region, Region) {
    // The machining region reaches one tool radius past the stock, as an
    // Outside containment's area does; the part is the hole.
    let Ok(region) = Region::new(vec![rect(50.0 + R, 20.0 + R), circle(PART)]) else { panic!("region") };
    let Ok(stock) = Region::new(vec![rect(50.0, 20.0), circle(PART)]) else { panic!("stock") };
    (region, stock)
}

/// #3748: a dead end too short for a ramp no longer sends the area to the
/// Auto order (the #3567 dead-end ramp is retired): the pocket form of the
/// band (no stock known) keeps its start-side plan, and no note names a
/// ramp at a dead end.
#[test]
fn a_short_dead_end_no_longer_falls_back_to_auto() {
    let (region, _) = band();
    let mut p = params(true);
    p.links.approach.ramp_angle = 0.01;
    let outside = match pocket_started(&region, &p, &[], &CornerRelief::default(), PassStart::Outside) {
        Ok(t) => t,
        Err(e) => panic!("{e:?}"),
    };
    assert_eq!(rings_fallbacks(&outside.notes), 0, "{:?}", outside.notes);
    assert!(!outside.notes.iter().any(|n| n.contains("dead end") || n.contains("too short for a ramp")), "{:?}", outside.notes);
}

/// The pocket form of the band (no stock known, so no air room) with
/// "Pass starts at: Outside" and the lead-in `entry`.
fn band_pocket(entry: Entry3D) -> Result<(Toolpath, PocketParams), String> {
    band_pocket_as(PocketStrategy::Waterline, entry)
}

/// [`band_pocket`] with the pocket `strategy`.
fn band_pocket_as(strategy: PocketStrategy, entry: Entry3D) -> Result<(Toolpath, PocketParams), String> {
    let (region, _) = band();
    let mut p = params(true);
    p.strategy = strategy;
    p.entry = entry;
    if entry == Entry3D::Ramp {
        p.links.approach.ramp_angle = RAMP_DEG;
    }
    pocket_started(&region, &p, &[], &CornerRelief::default(), PassStart::Outside).map(|t| (t, p)).map_err(|e| format!("{e:?}"))
}

/// The warnings that name the selected `entry` of `path`, as the op posts
/// them: the advice (`EntryNotRecommended`) and the start-side warnings.
fn named_warnings(path: &Toolpath, entry: Entry3D) -> usize {
    let advised = entry_advice(PassStart::Outside, entry).is_some_and(|a| path.notes.iter().any(|n| n.contains(a)));
    let plunges = entry_warnings(path, advised).iter().filter(|w| matches!(w, Warning::PlungeIntoStock { .. })).count();
    usize::from(advised) + plunges
}

/// #3884 pin 1 (the maintainer's rule on #3741): the plain Outside pocket
/// with no stock and the Arc entry has no air room, so the entry is
/// refused by name - no automatic ramp or helix is posted.
#[test]
fn outside_pocket_arc_without_air_is_refused() {
    // Adaptive too: with no stock outline it falls back to Offset rings,
    // which has no air room either (#3884 review: it dived into stock).
    for strategy in [PocketStrategy::Waterline, PocketStrategy::OffsetRings, PocketStrategy::Adaptive] {
        match band_pocket_as(strategy, Entry3D::Arc) {
            Ok((path, _)) => panic!("{strategy:?}: the Arc entry with no air room was not refused: {:?}", path.notes),
            Err(e) => assert!(e.contains(OUTSIDE_AIR_REFUSAL), "{strategy:?}: {e}"),
        }
    }
}

/// #3884 pin 2: the same pocket with a selected Plunge or Ramp runs as
/// selected, and names the selected entry in exactly one warning. A Ramp
/// may also post `EntryStandIn`: it names the other entries the ladder
/// used (an axial dive, ramps along the pass at pass starts in material),
/// not the selected Ramp, so it is not counted here.
#[test]
fn outside_pocket_selected_entry_keeps_one_warning() {
    for entry in [Entry3D::Plunge, Entry3D::Ramp] {
        let (path, p) = match band_pocket(entry) {
            Ok(t) => t,
            Err(e) => panic!("{entry:?}: {e}"),
        };
        let mut down = path.moves.iter().filter(|m| descends_below(m, p.top_z));
        let kept = match entry {
            Entry3D::Plunge => path.moves.iter().filter(|m| m.kind == MoveKind::Plunge && descends_below(m, p.top_z)).count() > 1,
            _ => down.any(|m| m.kind == MoveKind::Feed && m.arc.is_none()),
        };
        assert!(kept, "{entry:?}: the selected entry did not run: {:?}", path.notes);
        assert_eq!(named_warnings(&path, entry), 1, "{entry:?}: {:?}", path.notes);
    }
}
