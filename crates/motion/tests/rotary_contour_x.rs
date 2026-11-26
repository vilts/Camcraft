//! Provenance: original cylindrical fixtures and radial clearance measurements from elementary geometry.
//! Gate map (M4.4): covers camcraft_motion::multiaxis (Rotary Contour along X).

use std::collections::BTreeMap;

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::finishing3d::{Finish3DParams, FinishPattern, GapLink};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::multiaxis::{rotary_contour, AxisControl};

const RADIUS: f64 = 3.0;
const BALL: f64 = 1.0;

fn cylinder(y: f64, z: f64) -> Vec<Triangle> {
    let point = |x, j: usize, radius| {
        let angle = std::f64::consts::TAU * j as f64 / 32.0;
        Point3::new(x, y + radius * angle.cos(), z + radius * angle.sin())
    };
    let mut facets = Vec::new();
    for j in 0..32 {
        let (a, b, c, d) = (
            point(-8.0, j, RADIUS),
            point(-8.0, j + 1, RADIUS),
            point(8.0, j, RADIUS),
            point(8.0, j + 1, RADIUS),
        );
        facets.push(Triangle::new(a, b, d));
        facets.push(Triangle::new(a, d, c));
        facets.push(Triangle::new(point(-8.0, j, 0.0), b, a));
        facets.push(Triangle::new(point(8.0, j, 0.0), c, d));
    }
    facets
}

fn params() -> Finish3DParams {
    Finish3DParams {
        gap_link: GapLink::Jump,
        pattern: FinishPattern::Waterline,
        stepover: None,
        tool_diameter: BALL * 2.0,
        tolerance: 0.05,
        clearance_z: 8.0,
        retract_z: Some(6.0),
        top_z: 6.0,
        bottom_z: -6.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
        waterlines: true,
    }
}

fn control() -> AxisControl {
    AxisControl {
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: None,
    }
}

fn center(point: Point3, axis: [f64; 3]) -> Point3 {
    Point3::new(
        point.x + BALL * axis[0],
        point.y + BALL * axis[1],
        point.z + BALL * axis[2],
    )
}

fn rho(point: Point3, y: f64, z: f64) -> f64 {
    (point.y - y).hypot(point.z - z)
}

fn measure_rings(path: &Toolpath, y: f64, z: f64) {
    let mut quadrants: BTreeMap<i64, u8> = BTreeMap::new();
    let mut cuts = 0;
    for m in &path.moves {
        for p in [m.from, m.to] {
            assert!(
                p.x.is_finite() && p.y.is_finite() && p.z.is_finite(),
                "non-finite move: {m:?}"
            );
        }
        let a = m.axis.expect("every rotary move states its radial pose");
        assert!(a.iter().all(|v| v.is_finite()), "non-finite pose: {m:?}");
        assert!(a[0].abs() < 1e-9, "pose leans along the bar: {m:?}");
        let (dy, dz) = (m.to.y - m.from.y, m.to.z - m.from.z);
        let length2 = dy * dy + dz * dz;
        let t = if length2 > 0.0 {
            (-((m.from.y - y) * dy + (m.from.z - z) * dz) / length2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let closest = Point3::new(0.0, m.from.y + t * dy, m.from.z + t * dz);
        assert!(
            rho(closest, y, z) >= RADIUS - 0.08,
            "a move crosses the cylinder: {m:?}"
        );
        if m.kind != MoveKind::Feed {
            continue;
        }
        if rho(m.to, y, z) > rho(m.from, y, z) + 0.1 {
            continue; // The radial retract feeds out before the rapid index.
        }
        let c = center(m.to, a);
        assert!(
            c.x >= -6.0 - 1e-6 && c.x <= 6.0 + 1e-6,
            "cut outside station window: {c:?}"
        );
        assert!(
            (m.to.x - m.from.x).abs() < 1e-8,
            "a ring advances along X: {m:?}"
        );
        assert!(
            (rho(c, y, z) - (RADIUS + BALL)).abs() < 0.08,
            "ball left the cylinder wall: {c:?}"
        );
        let radial = rho(c, y, z);
        assert!(
            (a[1] - (c.y - y) / radial).abs() < 1e-8,
            "pose is not radial: {m:?}"
        );
        assert!(
            (a[2] - (c.z - z) / radial).abs() < 1e-8,
            "pose is not radial: {m:?}"
        );
        let quadrant = usize::from(c.y >= y) + 2 * usize::from(c.z >= z);
        *quadrants.entry((c.x * 1e6).round() as i64).or_default() |= 1 << quadrant;
        cuts += 1;
    }
    assert!(
        cuts > 64,
        "expected several complete rings, got {cuts} cut moves"
    );
    assert!(
        quadrants.len() > 3,
        "expected several stations: {quadrants:?}"
    );
    assert!(
        quadrants.values().all(|q| *q == 15),
        "a station lacks a complete ring: {quadrants:?}"
    );
    let first = path.moves.first().expect("a generated path opens");
    let last = path.moves.last().expect("a generated path exits");
    assert!(
        (rho(first.from, y, z) - 8.0).abs() < 1e-7,
        "opening misses radial clearance: {first:?}"
    );
    assert!(
        (rho(last.to, y, z) - 8.0).abs() < 1e-7,
        "exit misses radial clearance: {last:?}"
    );
}

#[test]
fn rotary_contour_x_generates_complete_radial_rings() {
    let path = rotary_contour(&cylinder(0.0, 0.0), &params(), &control())
        .expect("Rotary Contour must generate rings about X");
    measure_rings(&path, 0.0, 0.0);
}

#[test]
fn rotary_contour_x_uses_the_translated_axis_for_clearance() {
    let path = rotary_contour(&cylinder(11.0, -7.0), &params(), &control())
        .expect("station bounds and radial clearances do not depend on world Z");
    measure_rings(&path, 11.0, -7.0);
}

#[test]
fn rotary_contour_x_refuses_clearance_inside_the_work_by_name() {
    let p = Finish3DParams {
        clearance_z: 3.0,
        retract_z: None,
        ..params()
    };
    let error = rotary_contour(&cylinder(0.0, 0.0), &p, &control())
        .expect_err("clearance touches the work");
    let message = error.to_string();
    assert!(
        message.contains("Rotary Contour") && message.contains("clearance"),
        "{message}"
    );
}

#[test]
fn rotary_contour_x_refuses_retract_inside_the_work_by_name() {
    let p = Finish3DParams {
        retract_z: Some(3.0),
        ..params()
    };
    let error =
        rotary_contour(&cylinder(0.0, 0.0), &p, &control()).expect_err("retract touches the work");
    let message = error.to_string();
    assert!(
        message.contains("Rotary Contour") && message.contains("retract"),
        "{message}"
    );
}

#[test]
fn rotary_contour_x_refuses_an_invalid_station_window_by_name() {
    for (top, bottom) in [(-6.0, 6.0), (f64::NAN, -6.0), (6.0, f64::NEG_INFINITY)] {
        let p = Finish3DParams {
            top_z: top,
            bottom_z: bottom,
            ..params()
        };
        let error = rotary_contour(&cylinder(0.0, 0.0), &p, &control())
            .expect_err("invalid station bounds");
        let message = error.to_string();
        assert!(
            message.contains("Rotary Contour") && message.contains("station"),
            "{message}"
        );
    }
}
