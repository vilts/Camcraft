//! Explicit square-corner fans over a round upper profile (#1172).
//! Gate map (M4.4): covers camcraft_motion::wire_edm.
//! Provenance: synthetic profiles and elementary ruled-surface geometry.

use camcraft_motion::geo2d::{Loop, Point2};
use camcraft_motion::wire_edm::{
    wire_contour_ruled, WirePairing, WireParams, WireProfile, WireRuled, WireSide, WireSync,
};

fn fan_fixture() -> (Loop, Loop, Vec<WireSync>, WireParams) {
    let lower = Loop::new(vec![
        Point2::new(-10.0, -10.0),
        Point2::new(10.0, -10.0),
        Point2::new(10.0, 10.0),
        Point2::new(-10.0, 10.0),
    ])
    .expect("square");
    let upper = Loop::new(
        (0..64)
            .map(|i| {
                let a = f64::from(i) * std::f64::consts::TAU / 64.0;
                Point2::new(8.0 * a.cos(), 8.0 * a.sin())
            })
            .collect(),
    )
    .expect("round");
    let mut sync = Vec::new();
    for (i, corner) in lower.points().iter().enumerate() {
        let middle = 225.0 + i as f64 * 90.0;
        for angle in [middle - 22.5, middle + 22.5] {
            let a = angle.to_radians();
            sync.push(WireSync {
                lower: Some(*corner),
                upper: Some(Point2::new(8.0 * a.cos(), 8.0 * a.sin())),
            });
        }
    }
    let params = WireParams {
        wire_diameter: 0.25,
        gap: 0.04,
        condition: "synthetic".into(),
        side: WireSide::Punch,
        start: Some(Point2::new(-30.0, 0.0)),
        plane_z: 0.0,
        tabs: None,
        stock: None,
        clearance: 1.0,
    };
    (upper, lower, sync, params)
}

#[test]
fn square_corners_fan_over_stated_round_arcs() {
    let (upper, lower, sync, params) = fan_fixture();
    let ruled = WireRuled {
        top_z: 100.0,
        pairing: WirePairing::SyncPoints(sync),
        tolerance: 0.5,
        max_deg: Some(80.0),
        machine: "synthetic".into(),
    };
    let path = wire_contour_ruled(
        WireProfile {
            loops: std::slice::from_ref(&upper),
            chains: &[],
        },
        WireProfile {
            loops: std::slice::from_ref(&lower),
            chains: &[],
        },
        &params,
        &ruled,
    )
    .expect("each repeated square corner states a fan, not an invalid duplicate");
    assert!(
        path.moves.len() >= 64,
        "the fan follows every stated round vertex"
    );
    assert!(
        path.moves.iter().all(|m| m.axis.is_some()),
        "each fan ruling states its wire pose"
    );
    let fans = path
        .moves
        .windows(2)
        .filter(|m| {
            (m[0].to.x - m[1].to.x).hypot(m[0].to.y - m[1].to.y) < 1e-9 && m[0].axis != m[1].axis
        })
        .count();
    assert!(
        fans >= 4,
        "all four fixed lower corners span changing upper positions: {fans}"
    );
}

#[test]
fn upper_square_corners_can_be_fan_apices() {
    let (round, square, sync, params) = fan_fixture();
    let sync = sync
        .into_iter()
        .map(|s| WireSync {
            upper: s.lower,
            lower: s.upper,
        })
        .collect();
    let ruled = WireRuled {
        top_z: 100.0,
        pairing: WirePairing::SyncPoints(sync),
        tolerance: 0.5,
        max_deg: Some(80.0),
        machine: "synthetic".into(),
    };
    let path = wire_contour_ruled(
        WireProfile {
            loops: std::slice::from_ref(&square),
            chains: &[],
        },
        WireProfile {
            loops: std::slice::from_ref(&round),
            chains: &[],
        },
        &params,
        &ruled,
    )
    .expect("the fan can hold its upper end still");
    assert!(path.moves.len() >= 64);
    let upper = |m: &camcraft_motion::cl::Move| {
        let a = m.axis.expect("posed");
        Point2::new(m.to.x + a[0] / a[2] * 100.0, m.to.y + a[1] / a[2] * 100.0)
    };
    let fans = path
        .moves
        .windows(2)
        .filter(|m| {
            let (a, b) = (upper(&m[0]), upper(&m[1]));
            (a.x - b.x).hypot(a.y - b.y) < 1e-9 && m[0].to != m[1].to
        })
        .count();
    assert!(fans >= 4, "four upper apices: {fans}");
}

#[test]
fn an_identical_ruling_stated_twice_is_refused() {
    let (upper, lower, mut sync, params) = fan_fixture();
    sync.insert(1, sync[0]);
    let ruled = WireRuled {
        top_z: 100.0,
        pairing: WirePairing::SyncPoints(sync),
        tolerance: 0.5,
        max_deg: Some(80.0),
        machine: "synthetic".into(),
    };
    let why = wire_contour_ruled(
        WireProfile {
            loops: std::slice::from_ref(&upper),
            chains: &[],
        },
        WireProfile {
            loops: std::slice::from_ref(&lower),
            chains: &[],
        },
        &params,
        &ruled,
    )
    .expect_err("two identical rulings do not state a fan")
    .to_string();
    assert!(
        why.contains("sync pairs #1 and #2") && why.contains("same"),
        "{why}"
    );
}

#[test]
fn crossing_fan_spans_are_refused() {
    let (upper, lower, mut sync, params) = fan_fixture();
    let (a, b) = (sync[2].upper, sync[4].upper);
    sync[2].upper = b;
    sync[4].upper = a;
    let ruled = WireRuled {
        top_z: 100.0,
        pairing: WirePairing::SyncPoints(sync),
        tolerance: 0.5,
        max_deg: Some(80.0),
        machine: "synthetic".into(),
    };
    let why = wire_contour_ruled(
        WireProfile {
            loops: std::slice::from_ref(&upper),
            chains: &[],
        },
        WireProfile {
            loops: std::slice::from_ref(&lower),
            chains: &[],
        },
        &params,
        &ruled,
    )
    .expect_err("crossed fan spans never generate")
    .to_string();
    assert!(why.contains("cross"), "{why}");
}

fn ambiguous_whole_ring_fan(hold_upper: bool) -> String {
    let (upper, lower, sync, params) = fan_fixture();
    let mut sync = vec![sync[0], sync[3]];
    if hold_upper {
        sync[1].upper = sync[0].upper;
    } else {
        sync[1].lower = sync[0].lower;
    }
    let ruled = WireRuled {
        top_z: 100.0,
        pairing: WirePairing::SyncPoints(sync),
        tolerance: 0.5,
        max_deg: Some(80.0),
        machine: "synthetic".into(),
    };
    wire_contour_ruled(
        WireProfile {
            loops: std::slice::from_ref(&upper),
            chains: &[],
        },
        WireProfile {
            loops: std::slice::from_ref(&lower),
            chains: &[],
        },
        &params,
        &ruled,
    )
    .expect_err("one fixed apex cannot determine the closing whole-ring span")
    .to_string()
}

#[test]
fn a_whole_lower_ring_fan_is_refused_as_ambiguous() {
    let why = ambiguous_whole_ring_fan(false);
    assert!(
        why.contains("same point of the lower profile")
            && why.contains("whole-ring fan is ambiguous"),
        "{why}"
    );
}

#[test]
fn a_whole_upper_ring_fan_is_refused_as_ambiguous() {
    let why = ambiguous_whole_ring_fan(true);
    assert!(
        why.contains("same point of the upper profile")
            && why.contains("whole-ring fan is ambiguous"),
        "{why}"
    );
}

#[test]
fn an_upper_apex_fan_can_wrap_the_lower_ring_seam() {
    let (round, square, sync, params) = fan_fixture();
    let mut points = round.points().to_vec();
    points.rotate_left(40);
    let round = Loop::new(points).expect("round with its seam inside a fan");
    let sync = sync
        .into_iter()
        .map(|s| WireSync {
            upper: s.lower,
            lower: s.upper,
        })
        .collect();
    let ruled = WireRuled {
        top_z: 100.0,
        pairing: WirePairing::SyncPoints(sync),
        tolerance: 0.5,
        max_deg: Some(80.0),
        machine: "synthetic".into(),
    };
    let path = wire_contour_ruled(
        WireProfile {
            loops: std::slice::from_ref(&square),
            chains: &[],
        },
        WireProfile {
            loops: std::slice::from_ref(&round),
            chains: &[],
        },
        &params,
        &ruled,
    )
    .expect("the first and last lower anchors can share an upper apex across the seam");
    assert!(path.moves.len() >= 64);
}
