//! Swallowtail healing tests for 4-axis wire cuts (#2693): tight inside
//! corners, tight fillets, shallow slots, uncut-rest notes, door checks,
//! and strict refusal when `heal_tolerance` is zero or exceeded.
//!
//! Provenance: synthetic profiles and rolling-circle kerf geometry over
//! #532/#537/#1170; no vendor material consulted.

use super::ring::pt_ring;
use super::ruled_tests::{cut, params, rect, refusal, round, ruled, H, K};
use super::*;

fn slotted_rect(hx: f64, hy: f64, slot_half_w: f64, depth: f64) -> Loop {
    Loop::new(vec![
        Point2::new(-hx, -hy),
        Point2::new(hx, -hy),
        Point2::new(hx, hy),
        Point2::new(slot_half_w, hy),
        Point2::new(slot_half_w, hy - depth),
        Point2::new(-slot_half_w, hy - depth),
        Point2::new(-slot_half_w, hy),
        Point2::new(-hx, hy),
    ])
    .expect("slotted rectangle")
}

/// An L-shaped punch with one inside corner at `(0, 0)` filleted to `r`.
fn filleted_l_punch(r: f64) -> Loop {
    let mut pts = vec![
        Point2::new(-20.0, -10.0),
        Point2::new(20.0, -10.0),
        Point2::new(20.0, 0.0),
    ];
    for i in 0..=16 {
        // Quarter-circle fillet from `(r, 0)` around center `(r, r)` to `(0, r)`.
        let a = -std::f64::consts::FRAC_PI_2 - (f64::from(i) / 16.0) * std::f64::consts::FRAC_PI_2;
        pts.push(Point2::new(r + r * a.cos(), r + r * a.sin()));
    }
    pts.extend([Point2::new(0.0, 10.0), Point2::new(-20.0, 10.0)]);
    Loop::new(pts).expect("filleted L punch")
}

fn twisted_die_flank() -> (Loop, Loop, WirePairing) {
    let lo = rect(20.0, 10.0);
    let up = Loop::new(vec![
        Point2::new(-20.0, -10.0),
        Point2::new(20.0, -10.0),
        Point2::new(10.0, 10.0),
        Point2::new(-20.0, 10.0),
    ])
    .expect("twisted die");
    let pairing = super::ruled_tests::sync(&[
        ((-20.0, -10.0), (-20.0, -10.0)),
        ((20.0, -10.0), (20.0, -10.0)),
        ((10.0, 10.0), (20.0, 10.0)),
        ((-20.0, 10.0), (-20.0, 10.0)),
    ]);
    (up, lo, pairing)
}

fn assert_kerf_held(path: &Toolpath, lower: &Loop, upper: &Loop) {
    laws::kerf(path, lower.points(), true, K, path.moves[0].from.z).expect("lower plane obeys laws::kerf");
    let upper_pts = path.moves[..path.moves.len() - 1].iter().map(|m| {
        let a = m.axis.expect("posed");
        Point2::new(m.to.x + a[0] / a[2] * H, m.to.y + a[1] / a[2] * H)
    });
    let start = Point2::new(path.moves[0].from.x, path.moves[0].from.y);
    laws::kerf_ring_with_start(upper_pts, start, upper.points(), K, H).expect("upper plane obeys laws::kerf");
    for m in &path.moves[..path.moves.len() - 1] {
        let q = Point2::new(m.to.x, m.to.y);
        assert!(pt_ring(q, lower.points()) >= K - PATH_TOL);
    }
}

#[test]
fn tight_inside_corners_heal_within_heal_tolerance() {
    let (up, lo) = (rect(18.0, 9.0), rect(20.0, 10.0));
    let p = WireParams { side: WireSide::Die, start: Some(Point2::new(0.0, 0.0)), ..params() };
    for widening in [WireWidening::PerRuling, WireWidening::Uniform] {
        let r = WireRuled { heal_tolerance: 0.08, widening, ..ruled(WirePairing::ArcLength) };
        let path = wire_contour_ruled(
            WireProfile { loops: std::slice::from_ref(&up), chains: &[] },
            WireProfile { loops: std::slice::from_ref(&lo), chains: &[] },
            &p,
            &r,
        )
        .expect("sharp inside corners within heal_tolerance generate");
        assert_kerf_held(&path, &lo, &up);
        let said = path.notes.join("\n");
        assert!(
            said.contains("worst uncut rest") && said.contains("heal tolerance 0.0800 mm") && said.contains("near X"),
            "{widening:?}: {said}"
        );
    }
}

#[test]
fn tight_inside_fillet_heals_within_heal_tolerance() {
    // R = 0.12 < K = 0.165 leaves ~0.0186 mm uncut rest at the fillet midpoint.
    let lo = filleted_l_punch(0.12);
    let up = filleted_l_punch(0.12);
    let r = WireRuled { heal_tolerance: 0.04, ..ruled(WirePairing::ArcLength) };
    let path = cut(&up, &lo, &r).expect("tight inside fillet heals within heal_tolerance");
    assert_kerf_held(&path, &lo, &up);
    let said = path.notes.join("\n");
    assert!(said.contains("worst uncut rest") && said.contains("heal tolerance 0.0400 mm"), "{said}");
    let tight = refusal(&up, &lo, &WireRuled { heal_tolerance: 0.01, ..r });
    assert!(tight.contains("heal tolerance 0.0100 mm") && tight.contains("standing there"), "{tight}");
}

#[test]
fn shallow_narrow_slot_heals_within_heal_tolerance() {
    // Upper slot is deeper (0.065 mm) than lower (0.055 mm), so the upper
    // spot is the worst uncut rest across both planes.
    let (up, lo) = (slotted_rect(19.0, 9.0, 0.1, 0.065), slotted_rect(20.0, 10.0, 0.1, 0.055));
    let p = params();
    for widening in [WireWidening::PerRuling, WireWidening::Uniform] {
        let r = WireRuled { heal_tolerance: 0.08, widening, ..ruled(WirePairing::ArcLength) };
        let path = wire_contour_ruled(
            WireProfile { loops: std::slice::from_ref(&up), chains: &[] },
            WireProfile { loops: std::slice::from_ref(&lo), chains: &[] },
            &p,
            &r,
        )
        .expect("shallow narrow slot within heal_tolerance generates");
        assert_kerf_held(&path, &lo, &up);
        let said = path.notes.join("\n");
        assert!(
            said.contains("worst uncut rest 0.0650 mm")
                && said.contains("heal tolerance 0.0800 mm")
                && said.contains("Y8.935 Z12.000"),
            "{widening:?}: {said}"
        );
    }
}

#[test]
fn excesses_above_heal_tolerance_refuse_with_spot_depth_and_bound() {
    let (up, lo) = (rect(18.0, 9.0), rect(20.0, 10.0));
    let p = WireParams { side: WireSide::Die, start: Some(Point2::new(0.0, 0.0)), ..params() };
    let r = WireRuled { heal_tolerance: 0.05, ..ruled(WirePairing::ArcLength) };
    let err = wire_contour_ruled(
        WireProfile { loops: std::slice::from_ref(&up), chains: &[] },
        WireProfile { loops: std::slice::from_ref(&lo), chains: &[] },
        &p,
        &r,
    )
    .expect_err("corner rest ~0.0683 exceeds 0.05");
    let at = err.location().expect("refusal names a spot");
    assert!((at.x.abs() - 18.0).abs() < 1e-6 && (at.y.abs() - 9.0).abs() < 1e-6, "{at:?}");
    let why = err.to_string();
    assert!(
        why.contains("inside corner or slot")
            && why.contains("0.0683 mm standing there")
            && why.contains("heal tolerance 0.0500 mm"),
        "{why}"
    );
    let (deep_u, deep_l) = (slotted_rect(19.0, 9.0, 0.1, 5.0), slotted_rect(20.0, 10.0, 0.1, 5.0));
    let slot_err = refusal(&deep_u, &deep_l, &WireRuled { heal_tolerance: 0.08, ..ruled(WirePairing::ArcLength) });
    assert!(
        slot_err.contains("5.0000 mm standing there") && slot_err.contains("heal tolerance 0.0800 mm"),
        "{slot_err}"
    );
}

#[test]
fn zero_heal_tolerance_preserves_existing_refusals_and_door_rejects_bad_values() {
    let (up, lo) = (slotted_rect(19.0, 9.0, 0.1, 0.06), slotted_rect(20.0, 10.0, 0.1, 0.06));
    let mut path_params = params();
    path_params.plane_z = H;
    let direct_refusal = laws::cover(up.points(), &ring::wire_ring(&Region::new(vec![up.clone()]).unwrap(), WireSide::Punch, K).unwrap(), &path_params)
        .expect_err("strict cover refusal")
        .to_string();
    let ruled_refusal = refusal(&up, &lo, &WireRuled { heal_tolerance: 0.0, ..ruled(WirePairing::ArcLength) });
    assert_eq!(ruled_refusal, direct_refusal, "heal_tolerance == 0.0 preserves strict cover refusal byte for byte");

    let (ok_u, ok_l) = (round(8.0, 64), rect(10.0, 10.0));
    for bad in [-0.001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let why = refusal(&ok_u, &ok_l, &WireRuled { heal_tolerance: bad, ..ruled(WirePairing::ArcLength) });
        assert!(why.contains("heal tolerance") && why.contains("zero or more"), "{bad}: {why}");
    }
    // Positive heal_tolerance emits no uncut note when COVER_TOL already passes.
    let clean = cut(&ok_u, &ok_l, &WireRuled { heal_tolerance: 0.05, ..ruled(WirePairing::ArcLength) })
        .expect("clean cut generates");
    assert!(!clean.notes.iter().any(|n| n.contains("uncut rest")), "{:?}", clean.notes);
}

#[test]
fn heal_tolerance_stays_distinct_from_twist_tolerance() {
    // Twisted Die flank: needs >0.02 mm twist widening and ~0.1488 mm acute-corner healing.
    let (up, lo, pairing) = twisted_die_flank();
    let p = WireParams { side: WireSide::Die, start: Some(Point2::new(0.0, 0.0)), ..params() };
    for widening in [WireWidening::PerRuling, WireWidening::Uniform] {
        let base = WireRuled { max_deg: Some(45.0), widening, ..ruled(pairing.clone()) };
        let ok = wire_contour_ruled(
            WireProfile { loops: std::slice::from_ref(&up), chains: &[] },
            WireProfile { loops: std::slice::from_ref(&lo), chains: &[] },
            &p,
            &WireRuled { tolerance: 0.05, heal_tolerance: 0.20, ..base.clone() },
        )
        .expect("both twist tolerance and heal tolerance satisfied");
        assert_kerf_held(&ok, &lo, &up);
        let twist_refused = wire_contour_ruled(
            WireProfile { loops: std::slice::from_ref(&up), chains: &[] },
            WireProfile { loops: std::slice::from_ref(&lo), chains: &[] },
            &p,
            &WireRuled { tolerance: 0.0, heal_tolerance: 0.20, ..base },
        )
        .expect_err("heal_tolerance does not relax twist tolerance")
        .to_string();
        assert!(twist_refused.contains("twists the surface too far"), "{widening:?}: {twist_refused}");
    }
}
