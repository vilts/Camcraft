//! Shared adaptive trochoid fixtures from the project tests for issues #1509 and #2990.
#![allow(dead_code)] // Each target uses a different subset of the fixtures.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::Point3;
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{PocketParams, PocketStrategy, adaptive_trochoidal, pocket, ARC_FIT_TOL, DEFAULT_MAX_ENGAGEMENT};
use camcraft_motion::quality::{QualitySpec, measure, rest_area, stock_for};

const D: f64 = 6.0;

pub fn poly(pts: &[(f64, f64)]) -> Loop {
    let Ok(l) = Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()) else { panic!("loop") };
    l
}

pub fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let Ok(r) = Region::new(vec![poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])]) else { panic!("region") };
    r
}

/// A pocket face, 34 x 16 about the origin: a slim twin of the
/// maintainer's 50 x 25 (#1503; that one is the app suite's), its band
/// still wider than the tool.
pub fn face() -> Region {
    rect(-17.0, -8.0, 17.0, 8.0)
}

pub fn params() -> PocketParams {
    PocketParams {
        blend_radius: None,
        tool_diameter: D,
        stepover: 1.0,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown: 3.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Adaptive,
        retract_z: Some(2.0),
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links: Links::PLANAR,
    }
}

fn cut(region: &Region, p: &PocketParams) -> Toolpath {
    match adaptive_trochoidal(region, p) {
        Ok(t) => t,
        Err(e) => panic!("refused: {e}"),
    }
}

/// Every cutting move's cutter centre, sampled.
fn cut_points(t: &Toolpath) -> Vec<Point3> {
    t.moves.iter().filter(|m| m.kind != MoveKind::Rapid).flat_map(|m| m.segments3(0.05)).flat_map(|(a, b)| [a, b]).collect()
}

pub fn check_cleared(name: &str, region: &Region, p: &PocketParams) {
    let reach = if p.stock_to_leave > 0.0 {
        region.offset(-p.stock_to_leave).unwrap_or_else(|_| panic!("{name}: reach offset"))
    } else {
        region.clone()
    };
    let center = region.offset(-(0.5 * p.tool_diameter + p.stock_to_leave)).unwrap_or_else(|_| panic!("{name}: center offset"));
    let guard = center.offset(2e-3).unwrap_or_else(|_| panic!("{name}: guard offset"));
    let t = cut(region, p);
    assert!(t.all_finite(), "{name}: non-finite move");
    assert!(t.has_arcs(), "{name}: expected arc moves");
    for q in cut_points(&t) {
        assert!(guard.contains(Point2::new(q.x, q.y)), "{name}: point ({:.3}, {:.3}) outside center", q.x, q.y);
        assert!(q.z >= p.bottom_z - 1e-9 && q.z <= p.clearance_z + 1e-9, "{name}: z={:.3} out of range", q.z);
    }
    let crossed = camcraft_motion::verify::crossed_pairs(&t, ARC_FIT_TOL);
    assert!(crossed.is_empty(), "{name}: {} crossings: {:?}", crossed.len(), &crossed[..crossed.len().min(3)]);

    let mut spec = QualitySpec::new(p.tool_diameter);
    // Match the D/60 debug grid used by the other HSM quality fixtures.
    // Release checks keep the default D/80 measurement grid.
    if cfg!(debug_assertions) {
        spec.cell = p.tool_diameter / 60.0;
    }
    let Ok(mut dex) = stock_for(region, p.top_z, p.bottom_z, p.tool_diameter, &spec) else { panic!("{name}: stock") };
    let Ok(q) = measure(&t, &mut dex, &spec) else { panic!("{name}: measure") };
    assert!(q.max_deg <= DEFAULT_MAX_ENGAGEMENT + 0.5, "{name}: engagement {:.1} deg at {:?}", q.max_deg, q.max_at);
    assert!(q.slot_len < 1e-6, "{name}: slotting {:.3} mm", q.slot_len);
    let Ok(troch_rest) = rest_area(&dex, &reach, p.bottom_z) else { panic!("{name}: troch rest") };

    let Ok(hsm) = pocket(region, p) else { panic!("{name}: hsm ref") };
    let Ok(mut hdex) = stock_for(region, p.top_z, p.bottom_z, p.tool_diameter, &spec) else { panic!("{name}: hsm stock") };
    let Ok(_) = measure(&hsm, &mut hdex, &spec) else { panic!("{name}: hsm measure") };
    let Ok(hsm_rest) = rest_area(&hdex, &reach, p.bottom_z) else { panic!("{name}: hsm rest") };
    let tol = reach.area() * 0.005 + 2.0;
    assert!(troch_rest <= hsm_rest + tol, "{name}: troch rest {troch_rest:.2} mm2 exceeds hsm rest {hsm_rest:.2} mm2 (+{tol:.2})");
}
