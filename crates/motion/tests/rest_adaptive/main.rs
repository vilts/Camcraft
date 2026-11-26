//! 2D rest machining with the Adaptive strategy (issue 2031): planned on
//! the remaining stock earlier operations are PROVED to have left, read
//! here against an independent simulation of those operations. The rest
//! passes never rapid or plunge into material the simulation still
//! holds, pass the verifier, hold the optimal load at every requested
//! depth on the simulated stock, and cover the rest material in reach.
//! Unsupported stock, depth, entry and load cases are refused by name;
//! degenerate input errors instead of crashing.
//!
//! Gate map (issue 2031): covers camcraft_motion::machining2d::adaptive
//! (rest: stock, entry, peel, pass, runs, leads, canon, post;
//! raster::conservative), camcraft_motion::quality.

mod sim;

use camcraft_motion::cl::MoveKind;
use camcraft_motion::geo2d::{Point2, Region};
use camcraft_motion::links::{Approach, Links};
use camcraft_motion::machining2d::{
    rest_adaptive, rest_adaptive_previous, rest_levels, rest_stock_from_columns, GenError, PocketParams, RestParams,
    RestSource, RestStock, StockFloor, REST_ADAPTIVE_ENTERED, REST_ADAPTIVE_NO_STOCK, REST_STOCK_FILM,
};
use camcraft_motion::quality::rest_area;

use sim::{
    assert_load_held, assert_load_within, check, disc, filleted, poly, read, read_leaving, rect, refusal, rest_params, roughed,
    Sim, BOTTOM, FIRST, LOAD_CEILING_D80, LOAD_SLACK, R, TOOL_D, TOP,
};

/// Two 30 x 30 bays joined by a neck `w` wide and 10 long, inside
/// corners rounded to 3 mm.
fn bays(w: f64) -> Region {
    let (lo, hi) = (15.0 - w * 0.5, 15.0 + w * 0.5);
    let outline = [
        (0.0, 0.0), (30.0, 0.0), (30.0, lo), (40.0, lo), (40.0, 0.0), (70.0, 0.0),
        (70.0, 30.0), (40.0, 30.0), (40.0, hi), (30.0, hi), (30.0, 30.0), (0.0, 30.0),
    ];
    filleted(&poly(&outline), 2.9)
}

#[test]
fn four_corner_residues_are_cut_from_proved_stock_within_the_load() {
    // A 60 x 40 pocket with 3 mm corners: the 12 mm tool leaves four
    // corner residues (23 mm2 in all), at both depths.
    let region = filleted(&rect(0.0, 0.0, 60.0, 40.0), 3.0);
    let mut sim = Sim::new(&region);
    let stock = roughed(&region, &mut sim, BOTTOM);
    let before = rest_area(&sim.dex, &region, BOTTOM).unwrap();
    assert!(before > 20.0, "the fixture has residue to cut ({before:.1} mm2)");
    let p = rest_params();
    let path = rest_adaptive(&region, &p, &stock).unwrap();
    let r = read(&path, &region, &mut sim);
    assert_load_held(&r);
    assert!(r.left < 0.05, "{:.3} mm2 of rest material in reach is left standing", r.left);
    // One entry per corner and depth: a corner is finished before the
    // tool leaves it.
    assert_eq!(r.plunges, 8, "plunges");
    // Each pass is reached back along the one before it, not by riding
    // the walls round from the corner's first landing (issue 2109: 967
    // mm of air feed for 224 mm in cut before, 562 for 328 now).
    assert!(r.air <= 2.0 * r.cut, "{:.0} mm of air feed for {:.0} mm in cut", r.air, r.cut);
    // The passes stay at the corners: no feed crosses the pocket's middle.
    for m in path.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        let (dx, dy) = (m.to.x.min(60.0 - m.to.x), m.to.y.min(40.0 - m.to.y));
        assert!(dx < 14.0 && dy < 14.0, "a feed at ({}, {}) is outside the corners", m.to.x, m.to.y);
    }
    assert!(path.notes.iter().any(|n| n.contains("planned on the remaining stock of the earlier operations")));
    assert!(path.notes.iter().any(|n| n.contains("worst planned load") && n.contains("mm out of cut")));
}

#[test]
fn a_conventional_cut_leaving_stock_on_the_walls_keeps_every_law() {
    // The same corners cut the other way round, 0.3 mm left on the
    // walls, hopping at the clearance plane (no retract plane set). (3.5
    // mm corners: the cutter's center still turns on a 1.2 mm arc.)
    let region = filleted(&rect(0.0, 0.0, 60.0, 40.0), 3.5);
    let mut sim = Sim::new(&region);
    let stock = roughed(&region, &mut sim, BOTTOM);
    let p = PocketParams { climb: false, stock_to_leave: 0.3, retract_z: None, ..rest_params() };
    let path = rest_adaptive(&region, &p, &stock).unwrap();
    let r = read_leaving(&path, &region, &mut sim, 0.3);
    assert_load_held(&r);
    assert!(r.left < 0.05, "{:.3} mm2 of rest material in reach is left standing", r.left);
    // Nothing nearer the walls than the tool radius and the stock to leave.
    let legal = region.offset(-(R + 0.3 - 0.01)).unwrap();
    for m in path.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        assert!(legal.contains(Point2::new(m.to.x, m.to.y)), "a move at ({}, {}) cuts into the stock to leave", m.to.x, m.to.y);
    }
    // The climb plan travels each corner the other way.
    let climb = rest_adaptive(&region, &PocketParams { climb: true, ..p }, &stock).unwrap();
    assert_ne!(climb.moves, path.moves);
}

#[test]
fn an_island_and_disconnected_rest_areas_are_each_entered_from_proved_space() {
    // A 70 x 50 pocket round a 14 x 14 island; besides the four corner
    // residues, two pillars the earlier operations left standing in
    // cleared floor (rest areas that touch no wall).
    let island = rect(28.0, 18.0, 42.0, 32.0);
    let region = filleted(&rect(0.0, 0.0, 70.0, 50.0), 3.0).difference(&island).unwrap();
    let mut sim = Sim::new(&region);
    let rough = roughed(&region, &mut sim, BOTTOM);
    let pillars = [(Point2::new(14.0, 25.0), 2.5), (Point2::new(56.0, 25.0), 1.5)];
    let floors = rough
        .floors()
        .iter()
        .map(|f| {
            let mut remaining = f.remaining.clone();
            for &(at, rad) in &pillars {
                remaining = remaining.union(&disc(at, rad + 0.05)).unwrap();
            }
            StockFloor { remaining, ..f.clone() }
        })
        .collect();
    for &(at, rad) in &pillars {
        sim.stand(at, rad);
    }
    let stock = RestStock::from_verified(floors).unwrap();
    let path = rest_adaptive(&region, &rest_params(), &stock).unwrap();
    let r = read(&path, &region, &mut sim);
    assert_load_held(&r);
    assert!(r.left < 0.05, "{:.3} mm2 of rest material in reach is left standing", r.left);
    // No feed enters the island's own keep-out (the island grown by the
    // tool radius), and both pillars are gone.
    let keep_out = island.offset(R - 0.01).unwrap();
    for m in path.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        assert!(!keep_out.contains(Point2::new(m.to.x, m.to.y)), "a move at ({}, {}) gouges the island", m.to.x, m.to.y);
    }
    for &(at, _) in &pillars {
        assert!(sim.top_under(at, 0.5) <= BOTTOM + 1e-6, "the pillar at ({}, {}) still stands", at.x, at.y);
    }
    // (Issue 2109: 955 mm of air feed for 250 mm in cut before, 485 for 388 now.)
    assert!(r.air <= 1.5 * r.cut, "{:.0} mm of air feed for {:.0} mm in cut", r.air, r.cut);
}

#[test]
fn a_neck_the_earlier_tool_could_not_pass_is_cut_through_within_the_load() {
    // Two bays joined by a 10 mm neck: the 12 mm tool never entered it.
    let region = bays(10.0);
    let mut sim = Sim::new(&region);
    let stock = roughed(&region, &mut sim, BOTTOM);
    assert!(sim.top_under(Point2::new(35.0, 15.0), 0.5) >= TOP - 1e-9, "the fixture's neck is uncut");
    let path = rest_adaptive(&region, &rest_params(), &stock).unwrap();
    let r = read(&path, &region, &mut sim);
    assert_load_held(&r);
    assert!(r.left < 0.05, "{:.3} mm2 of rest material in reach is left standing", r.left);
    for x in [31.0, 35.0, 39.0] {
        for y in [11.0, 15.0, 19.0] {
            assert!(sim.top_under(Point2::new(x, y), 0.3) <= BOTTOM + 1e-6, "the neck still stands at ({x}, {y})");
        }
    }
    // A half loop is reached back along the one before it, and each
    // mouth of the neck is cut through its phases before the tool
    // leaves it (issue 2109: 37 plunges a depth and 3445 mm of air feed
    // for 954 mm in cut before; 10 a depth and 1979 for 1438 now).
    assert!(r.plunges <= 24, "{} plunges", r.plunges);
    assert!(r.air <= 1.6 * r.cut, "{:.0} mm of air feed for {:.0} mm in cut", r.air, r.cut);
}

#[test]
fn a_neck_too_narrow_to_hold_the_load_is_refused_by_name() {
    // A 6 mm neck for a 4 mm tool: every pass through it wraps the
    // cutter. Not slotted - refused, with the place.
    let region = bays(6.0);
    let mut sim = Sim::new(&region);
    let stock = roughed(&region, &mut sim, BOTTOM);
    let err = rest_adaptive(&region, &rest_params(), &stock);
    assert!(err.as_ref().is_err_and(|e| e.location().is_some()), "the refusal names its place");
    let msg = refusal(err);
    assert!(msg.contains("cannot hold the optimal load at (") && msg.contains("choose Waterline or Offset rings"), "{msg}");
}

#[test]
fn a_pocket_corner_too_sharp_to_turn_in_within_the_load_is_refused_by_name() {
    // Square corners: the cutter's center turns on the spot in each, and
    // the last sliver wraps a quarter of it however thin it is.
    let region = rect(0.0, 0.0, 60.0, 40.0);
    let mut sim = Sim::new(&region);
    let stock = roughed(&region, &mut sim, BOTTOM);
    let msg = refusal(rest_adaptive(&region, &rest_params(), &stock));
    assert!(msg.contains("cannot hold the optimal load at ("), "{msg}");
}

#[test]
fn a_roughing_pass_that_stops_above_the_rest_floor_is_refused_by_name() {
    let region = filleted(&rect(0.0, 0.0, 60.0, 40.0), 3.0);
    let mut sim = Sim::new(&region);
    // Roughed to Z-2 only; the rest pass is asked down to Z-4.
    let stock = roughed(&region, &mut sim, FIRST);
    assert_eq!(stock.deepest(), FIRST);
    let msg = refusal(rest_adaptive(&region, &rest_params(), &stock));
    assert!(msg.contains("not proved at Z-4.000") && msg.contains("proved down to Z-2.000 only"), "{msg}");
    // Down to the proved floor it plans, and holds every law there.
    let shallow = PocketParams { bottom_z: FIRST, ..rest_params() };
    let path = rest_adaptive(&region, &shallow, &stock).unwrap();
    assert!(path.moves.iter().all(|m| m.to.z >= FIRST - 1e-9));
    assert_eq!(check(&path, &region), Vec::new());
}

#[test]
fn stock_that_is_not_proved_is_refused_by_name() {
    let region = filleted(&rect(0.0, 0.0, 60.0, 40.0), 3.0);
    let p = rest_params();
    // No floor at all.
    match RestStock::from_verified(Vec::new()) {
        Err(GenError::Invalid(s)) => assert!(s.contains("no proved floor was given"), "{s}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
    let floor = |z: f64, remaining: Region, uncertain: Region| StockFloor { z, remaining, uncertain };
    // A floor at no depth.
    assert!(RestStock::from_verified(vec![floor(f64::NAN, Region::empty(), Region::empty())]).is_err());
    // A film that is no length.
    let corner = rect(0.0, 0.0, 5.0, 5.0).intersect(&region).unwrap();
    let ok = RestStock::from_verified(vec![floor(BOTTOM, corner.clone(), Region::empty())]).unwrap();
    assert_eq!(ok.film(), REST_STOCK_FILM);
    assert!(ok.clone().with_film(-0.1).is_err() && ok.clone().with_film(f64::INFINITY).is_err());
    // Uncertain everywhere: no position is proved clear, so no entry.
    let doubt = RestStock::from_verified(vec![floor(BOTTOM, corner, region.clone())]).unwrap();
    let msg = refusal(rest_adaptive(&region, &p, &doubt));
    assert!(msg.contains("no entry with proved clearance"), "{msg}");
    // Nothing left: said, not posted as an empty path.
    let clean = RestStock::from_verified(vec![floor(BOTTOM, Region::empty(), Region::empty())]).unwrap();
    match rest_adaptive(&region, &p, &clean) {
        Err(GenError::Empty(s)) => assert!(s.contains("no rest material"), "{s}"),
        other => panic!("expected Empty, got {other:?}"),
    }
}

#[test]
fn degenerate_input_errors_and_never_panics() {
    let p = rest_params();
    let some = |region: &Region| {
        RestStock::from_verified(vec![StockFloor { z: BOTTOM, remaining: region.clone(), uncertain: Region::empty() }]).unwrap()
    };
    // Narrower than the tool.
    let sliver = rect(0.0, 0.0, 20.0, 3.0);
    assert!(refusal(rest_adaptive(&sliver, &p, &some(&sliver))).contains("narrower than the tool"));
    // An empty pocket, a bad parameter.
    assert!(rest_adaptive(&Region::empty(), &p, &some(&sliver)).is_err());
    let region = filleted(&rect(0.0, 0.0, 30.0, 20.0), 3.0);
    for bad in [
        PocketParams { optimal_load: 0.0, ..p },
        PocketParams { tool_diameter: f64::NAN, ..p },
        PocketParams { stepdown: 0.0, ..p },
        PocketParams { feed: -1.0, ..p },
    ] {
        assert!(rest_adaptive(&region, &bad, &some(&region)).is_err(), "{bad:?}");
    }
    // Sub-micron and barely-fitting shapes, stock that lies outside the
    // pocket: any answer but a panic or a hang.
    for (w, h) in [(1e-4, 1e-4), (4.0 + 1e-7, 30.0), (4.2, 4.2)] {
        let tiny = rect(0.0, 0.0, w, h);
        let _ = rest_adaptive(&tiny, &p, &some(&tiny));
    }
    let _ = rest_adaptive(&region, &p, &some(&rect(100.0, 100.0, 110.0, 110.0)));
}

/// The load as the default measuring stock reads it (D/80, `quality`'s
/// own grid): in a concave corner its rim, a cell and a half outside the
/// cutter, takes in more wall than the planner's D/120 reading, so the
/// same paths read higher. They are held to a ceiling of their own there
/// (`LOAD_CEILING_D80`), so an over-engagement the coarse grid would hide
/// behind its yardstick still fails.
#[test]
fn on_the_default_measuring_grid_the_load_stays_under_its_documented_ceiling() {
    for (cr, leave, climb) in [(3.0, 0.0, true), (3.5, 0.3, false)] {
        let region = filleted(&rect(0.0, 0.0, 60.0, 40.0), cr);
        let mut sim = Sim::on_grid(&region, TOOL_D / 80.0);
        let stock = roughed(&region, &mut sim, BOTTOM);
        let p = PocketParams { climb, stock_to_leave: leave, ..rest_params() };
        let path = rest_adaptive(&region, &p, &stock).unwrap();
        let r = read_leaving(&path, &region, &mut sim, leave);
        assert_load_within(&r, LOAD_CEILING_D80);
        assert!(r.left < 0.05, "{:.3} mm2 of rest material in reach is left standing", r.left);
    }
}

/// On a fine grid (D/240; a pocket small enough for the measuring
/// stock's 4096 cells a side) the rim stands only a sixtieth of a
/// millimetre past the cutter, and the corners - the worst place, cut
/// the conventional way with stock left on the walls - read within the
/// planner's own margin: the coarse grid's excess is the yardstick's,
/// not the path's.
#[test]
fn on_a_fine_grid_the_corners_read_within_the_planning_margin() {
    let region = filleted(&rect(0.0, 0.0, 60.0, 40.0), 3.5);
    let mut sim = Sim::on_grid(&region, TOOL_D / 240.0);
    let columns = sim.dex.columns().unwrap();
    assert_eq!((columns.nx(), columns.ny()), (3720, 2520), "D/240 stays below the 4096-cell limit");
    assert_eq!(columns.cell(), TOOL_D / 240.0, "the independent measuring grid stays at D/240");
    let stock = roughed(&region, &mut sim, BOTTOM);
    let p = PocketParams { climb: false, stock_to_leave: 0.3, ..rest_params() };
    let path = rest_adaptive(&region, &p, &stock).unwrap();
    // Only corners: every feed within a corner's reach of two walls.
    for m in path.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        let (dx, dy) = (m.to.x.min(60.0 - m.to.x), m.to.y.min(40.0 - m.to.y));
        assert!(dx < 11.0 && dy < 11.0, "a feed at ({}, {}) is outside the corners", m.to.x, m.to.y);
    }
    let r = read_leaving(&path, &region, &mut sim, 0.3);
    assert_load_within(&r, LOAD_SLACK);
    assert!(r.left < 0.05, "{:.3} mm2 of rest material in reach is left standing", r.left);
}

fn from_ops() -> RestParams {
    RestParams {
        source: RestSource::PreviousOperations,
        previous_tool_diameter: 0.0,
        previous_corner_radius: 0.0,
        previous_stock_to_leave: 0.0,
        allowance: 0.0,
    }
}

#[test]
fn the_column_stock_of_the_previous_operations_plans_the_rest_adaptive_pass() {
    // Issue 1446: the stock is read from the simulated columns (not
    // from the roughing path), and the path is held to every law.
    let region = filleted(&rect(0.0, 0.0, 60.0, 40.0), 3.0);
    let mut sim = Sim::new(&region);
    let _ = roughed(&region, &mut sim, BOTTOM);
    let before = rest_area(&sim.dex, &region, BOTTOM).unwrap();
    let path = {
        let cols = sim.dex.columns().unwrap();
        rest_adaptive_previous(&region, &rest_params(), &from_ops(), Some(&cols)).unwrap()
    };
    assert!(path.notes.iter().any(|n| n.contains("reads the stock that the previous operations leave")), "{:?}", path.notes);
    let r = read(&path, &region, &mut sim);
    assert_load_held(&r);
    assert!(before > 20.0 && r.left < 0.05, "{:.3} of {before:.1} mm2 of rest material is left standing", r.left);
    for m in path.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        let (dx, dy) = (m.to.x.min(60.0 - m.to.x), m.to.y.min(40.0 - m.to.y));
        assert!(dx < 14.0 && dy < 14.0, "a feed at ({}, {}) is outside the corners", m.to.x, m.to.y);
    }
}

#[test]
fn the_previous_operations_planner_refuses_what_it_cannot_prove() {
    let region = filleted(&rect(0.0, 0.0, 60.0, 40.0), 3.0);
    let p = rest_params();
    // An entered previous tool proves nothing.
    let entered = RestParams { source: RestSource::EnteredTool, previous_tool_diameter: 12.0, ..from_ops() };
    let mut sim = Sim::new(&region);
    let _ = roughed(&region, &mut sim, FIRST);
    let cols = sim.dex.columns().unwrap();
    assert!(refusal(rest_adaptive_previous(&region, &p, &entered, Some(&cols))).contains(REST_ADAPTIVE_ENTERED));
    // No stock of the previous operations.
    assert!(refusal(rest_adaptive_previous(&region, &p, &from_ops(), None)).contains(REST_ADAPTIVE_NO_STOCK));
    // Roughed to Z-2 only: the floor at Z-4 stands whole, and no plunge
    // is proved clear there.
    let msg = refusal(rest_adaptive_previous(&region, &p, &from_ops(), Some(&cols)));
    assert!(msg.contains("no proved-clear") || msg.contains("entry"), "{msg}");
}

#[test]
fn rest_levels_and_previous_ops_honour_equal_steps() {
    let region = filleted(&rect(0.0, 0.0, 60.0, 40.0), 3.0);
    let mut sim = Sim::new(&region);
    let _ = roughed(&region, &mut sim, BOTTOM);
    let mut p = rest_params();
    p.top_z = TOP;
    p.bottom_z = BOTTOM;
    p.stepdown = 1.5;
    p.links = Links { approach: Approach { equal_steps: true, ..Links::PLANAR.approach }, ..Links::PLANAR };

    let zs = rest_levels(&p).unwrap();
    let expected = [0.0, -4.0 / 3.0, -8.0 / 3.0, -4.0];
    assert_eq!(zs.len(), expected.len(), "expected 4 equal-step levels, got {zs:?}");
    for (&got, &want) in zs.iter().zip(&expected) {
        assert!((got - want).abs() < 1e-9, "got {zs:?}, expected {expected:?}");
    }

    let cols = sim.dex.columns().unwrap();
    let prev_path = rest_adaptive_previous(&region, &p, &from_ops(), Some(&cols)).unwrap();
    let stock = rest_stock_from_columns(&cols, &region, &zs).unwrap().with_film(cols.cell() * (1.0 + std::f64::consts::SQRT_2)).unwrap();
    let rest_path = rest_adaptive(&region, &p, &stock).unwrap();
    for (label, path) in [("rest_adaptive_previous", &prev_path), ("rest_adaptive", &rest_path)] {
        let mut cut_zs: Vec<f64> = path
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < 1e-9)
            .map(|m| m.to.z)
            .collect();
        cut_zs.sort_by(f64::total_cmp);
        cut_zs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        for z in [-4.0 / 3.0, -8.0 / 3.0, -4.0] {
            assert!(cut_zs.iter().any(|&c| (c - z).abs() < 1e-6), "{label}: missing cut at Z{z}: {cut_zs:?}");
        }
        assert!(
            cut_zs.iter().all(|&c| zs.iter().any(|&want| (c - want).abs() < 1e-6)),
            "{label}: unexpected cut depth in {cut_zs:?}"
        );
        assert_eq!(check(path, &region), Vec::new(), "{label}: verifier warnings");
    }
}
