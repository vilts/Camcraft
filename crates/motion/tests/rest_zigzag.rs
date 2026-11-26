//! Provenance: analytic rounded-rectangle residue and this project's rest2d fixtures (#1490).
//! Gate map (#1490): covers camcraft_motion::machining2d (rest2d, pocket, pocket_strategy).

use camcraft_motion::cl::MoveKind;
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{rest_pocket, PocketParams, PocketStrategy, RestParams};
use camcraft_motion::plunge_law::{first_solid_plunge, Solid};
use camcraft_motion::verify::{verify, Mode, ToolGeom, VerifyContext};

const W: f64 = 30.0;
const H: f64 = 20.0;

fn rounded() -> Region {
    let mut points = Vec::new();
    for (x, y, start) in [(W - 3.0, 3.0, -90.0), (W - 3.0, H - 3.0, 0.0), (3.0, H - 3.0, 90.0), (3.0, 3.0, 180.0)] {
        for i in 0..=64 {
            let a = (start + 90.0 * f64::from(i) / 64.0).to_radians();
            points.push(Point2::new(x + 3.0 * a.cos(), y + 3.0 * a.sin()));
        }
    }
    Region::new(vec![Loop::new(points).unwrap()]).unwrap()
}

fn depth_in(p: Point2, radius: f64) -> f64 {
    let qx = (p.x - W * 0.5).abs() - (W * 0.5 - radius);
    let qy = (p.y - H * 0.5).abs() - (H * 0.5 - radius);
    radius - qx.max(0.0).hypot(qy.max(0.0)) - qx.max(qy).min(0.0)
}

fn distance(q: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let square = dx * dx + dy * dy;
    let t = if square > 0.0 { (((q.x - a.x) * dx + (q.y - a.y) * dy) / square).clamp(0.0, 1.0) } else { 0.0 };
    (q.x - a.x - t * dx).hypot(q.y - a.y - t * dy)
}

#[test]
fn zigzag_rest_covers_corner_residue_without_gouges_or_unsafe_rapids() {
    for climb in [true, false] {
        for stepover in [1.5, 4.0] {
            check_rest(climb, stepover);
        }
    }
}

fn check_rest(climb: bool, stepover: f64) {
    let region = rounded();
    let previous = RestParams {
        source: Default::default(),
        previous_tool_diameter: 12.0,
        previous_corner_radius: 0.0,
        previous_stock_to_leave: 0.0,
        allowance: 0.0,
    };
    let p = PocketParams {
        blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        tool_diameter: 4.0,
        stepover,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -2.0,
        stepdown: 2.0,
        feed: 400.0,
        plunge_feed: 80.0,
        climb,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    };
    let path = rest_pocket(&region, &p, &previous).expect("Zigzag rest machining must cut the corner residue");
    let tool = ToolGeom {
        radius: 2.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 2.0,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    let context = VerifyContext {
        region: &region,
        stock: None,
        model: None,
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 0.0,
        clearance_z: p.clearance_z,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    let warnings = verify(&path, &context);
    assert!(warnings.is_empty(), "climb={climb}, stepover={stepover}: the rest pass must pass verification: {warnings:#?}");
    for m in &path.moves {
        if m.kind == MoveKind::Rapid && (m.from.z < p.top_z || m.to.z < p.top_z) {
            assert!(m.from.x == m.to.x && m.from.y == m.to.y && m.to.z > m.from.z, "rapid enters stock: {m:?}");
        }
    }
    let cuts: Vec<_> = path.moves.iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - p.bottom_z).abs() < 1e-9 && (m.to.z - p.bottom_z).abs() < 1e-9)
        .flat_map(|m| m.chords(0.005))
        .collect();
    assert!(!cuts.is_empty(), "the path must cut at the floor");
    // Closing rings alone can cover these narrow corners. Require a row
    // strictly inside the planned center area to pin the chosen strategy.
    let plan = camcraft_motion::machining2d::rest_plan(&region, p.tool_diameter, p.stock_to_leave, &previous).unwrap();
    assert!(cuts.iter().any(|&(a, b)| {
        (a.y - b.y).abs() < 1e-9 && (a.x - b.x).abs() > 0.02
            && plan.center.loops().iter().all(|ring| {
                let points = ring.points();
                (0..points.len()).all(|i| distance(a, points[i], points[(i + 1) % points.len()]) > 0.005)
            })
    }), "Zigzag rest must retain interior rows before shared emission");
    let mut measured = 0;
    let mut missed = 0;
    for i in 0..=300 {
        for j in 0..=200 {
            let q = Point2::new(f64::from(i) * 0.1, f64::from(j) * 0.1);
            // Both analytic boundaries have a 30 um margin, independent
            // of the implementation's rest-region offsets.
            if depth_in(q, 3.0) > 0.03 && depth_in(q, 6.0) < -0.03 {
                measured += 1;
                if !cuts.iter().any(|&(a, b)| distance(q, a, b) <= 2.02) {
                    missed += 1;
                }
            }
        }
    }
    assert!(measured > 1500, "fixture must measure residue in its four corners");
    assert_eq!(missed, 0, "climb={climb}, stepover={stepover}: Zigzag must not leave wall cusps in the corner residue");
}

#[test]
fn tiny_rest_pocket_keeps_interior_rows_when_previous_tool_fits_nowhere() {
    let region = Region::new(vec![Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(0.02, 0.0),
        Point2::new(0.02, 0.015),
        Point2::new(0.0, 0.015),
    ]).unwrap()]).unwrap();
    let previous = RestParams {
        source: Default::default(),
        previous_tool_diameter: 0.03,
        previous_corner_radius: 0.0,
        previous_stock_to_leave: 0.0,
        allowance: 0.0,
    };
    let p = PocketParams {
        blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        tool_diameter: 0.001,
        stepover: 0.0005,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -0.001,
        stepdown: 0.001,
        feed: 40.0,
        plunge_feed: 8.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 0.0001,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    };
    let mut non_center_cutting = p;
    non_center_cutting.links.approach.center_cutting = false;
    let path = rest_pocket(&region, &non_center_cutting, &previous)
        .expect("tiny rest pocket may ramp in with a non-center-cutting tool");
    assert_eq!(
        first_solid_plunge(&path, p.tool_diameter * 0.5, &Solid { top: p.top_z, footprint: Some(&region) }),
        None,
        "the non-center-cutting tool must never plunge into solid"
    );
    let cuts: Vec<_> = path.moves.iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - p.bottom_z).abs() < 1e-9 && (m.to.z - p.bottom_z).abs() < 1e-9)
        .flat_map(|m| m.chords(0.00001))
        .collect();
    let middle = Point2::new(0.01, 0.0075);
    assert!(cuts.iter().any(|&(a, b)| distance(middle, a, b) <= p.tool_diameter * 0.5), "the closing ring alone leaves the pocket center uncut");
}
