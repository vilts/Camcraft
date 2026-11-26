//! Provenance: synthetic rounded-profile regressions for input preimages of trimmed polygon offsets (#2694).
//! Gate map (M4.4): covers camcraft_motion::wire_edm.

use super::pairing::{pair, Ring};
use super::ring::{dist, wire_ring};
use super::ruled::arcs;
use super::ruled_laws::{clearance, crossing, folds, lean_law};
use super::ruled_tests::{params, ruled, rulings};
use super::*;

/// Four 90-degree fillets in 3-degree facets, on a 40 x 20 rectangle.
pub(super) fn rounded(radius: f64) -> Loop {
    let (hx, hy) = (20.0 - radius, 10.0 - radius);
    let mut points = Vec::new();
    for (cx, cy, start) in [
        (hx, hy, 0.0_f64),
        (-hx, hy, 90.0),
        (-hx, -hy, 180.0),
        (hx, -hy, 270.0),
    ] {
        for step in 0..=30 {
            let angle = (start + 3.0 * f64::from(step)).to_radians();
            points.push(Point2::new(
                cx + radius * angle.cos(),
                cy + radius * angle.sin(),
            ));
        }
    }
    Loop::new(points).expect("a simple rounded rectangle")
}

fn asymmetric_trimmed_fillet(widening: WireWidening, pairing: WirePairing) {
    asymmetric_profiles(rounded(0.3), rounded(0.164), widening, pairing);
}

fn asymmetric_profiles(upper: Loop, lower: Loop, widening: WireWidening, pairing: WirePairing) {
    // The lower radius is just smaller than the kerf offset. Its corner
    // arc is trimmed, within the existing 0.002 mm cover tolerance.
    // The upper corner survives. Pairing the two offset lengths directly
    // moves the lower end along a flank while the upper rounds its corner.
    // A unique nearest side keeps the stated seam independent of vertex order.
    let p = WireParams {
        side: WireSide::Die,
        start: Some(Point2::new(-5.0, -1.0)),
        ..params()
    };
    let r = WireRuled {
        widening,
        ..ruled(pairing)
    };
    let lower_region = Region::new(vec![lower.clone()]).expect("lower region");
    let lower_wire =
        wire_ring(&lower_region, p.side, p.kerf_offset()).expect("one healed lower ring");
    assert!(
        lower_wire.len() < lower.points().len() / 2,
        "the lower fillets really lose their original spans: {lower_wire:?}"
    );
    let path = wire_contour_ruled(
        WireProfile {
            loops: std::slice::from_ref(&upper),
            chains: &[],
        },
        WireProfile {
            loops: std::slice::from_ref(&lower),
            chains: &[],
        },
        &p,
        &r,
    )
    .expect("the shallow asymmetric trim keeps all ruled-surface laws");
    let wire = rulings(&path);
    let fans = wire
        .windows(2)
        .filter(|w| dist(w[0].0, w[1].0) < 1e-9 && dist(w[0].1, w[1].1) > 1e-5)
        .count();
    assert!(fans >= 4, "each excised lower fillet must remain at its trim vertex while the upper walks its span; found {fans} fan steps");
    // Reconstruct the unhealed product independently of the new mapping.
    // The laws read the emitted poses, including every stationary apex.
    let (gu, gl) = (
        Ring::new(upper.points()).expect("upper ring"),
        Ring::new(lower.points()).expect("lower ring"),
    );
    let start = p.start.expect("fixture states its start");
    let anchors = match &r.pairing {
        WirePairing::ArcLength => vec![(
            gu.at(gu.nearest(start, false).0),
            gl.at(gl.nearest(start, false).0),
            0,
        )],
        WirePairing::SyncPoints(points) => points
            .iter()
            .enumerate()
            .map(|(i, s)| {
                (
                    s.upper.expect("upper sync"),
                    s.lower.expect("lower sync"),
                    i + 1,
                )
            })
            .collect(),
    };
    let product =
        pair(&gu, &gl, &arcs(&anchors, &gu, &gl, false), None).expect("stated unhealed product");
    let h = r.top_z - p.plane_z;
    folds(&wire, p.plane_z, h, "regression", "keep aligned").expect("emitted fan never folds");
    crossing(&wire, p.plane_z, h).expect("emitted fan never crosses");
    lean_law(&wire, p.plane_z, h, 30.0, "regression", "M")
        .expect("emitted fan respects the machine lean");
    let read = clearance(&product, &wire, start, p.kerf_offset(), p.plane_z, h);
    assert!(
        read.ratio.is_finite() && read.ratio <= 1.0 + super::ruled::SETTLED,
        "all 33 heights clear the unhealed product: {} at {:?}",
        read.ratio,
        read.at
    );
    assert!(
        read.lead.is_none(),
        "the lead also clears the unhealed product"
    );
}

#[test]
fn uniform_arc_length_sews_an_asymmetric_trimmed_fillet_as_a_fan() {
    asymmetric_trimmed_fillet(WireWidening::Uniform, WirePairing::ArcLength);
}

#[test]
fn per_ruling_arc_length_sews_an_asymmetric_trimmed_fillet_as_a_fan() {
    asymmetric_trimmed_fillet(WireWidening::PerRuling, WirePairing::ArcLength);
}

#[test]
fn sync_points_inside_excised_fillets_hold_the_trim_vertices() {
    let mut pairs = Vec::new();
    for (sx, sy, start) in [
        (1.0, 1.0, 0.0_f64),
        (-1.0, 1.0, 90.0),
        (-1.0, -1.0, 180.0),
        (1.0, -1.0, 270.0),
    ] {
        for delta in [15.0, 75.0] {
            let angle = (start + delta).to_radians();
            let point = |radius: f64| {
                Point2::new(
                    sx * (20.0 - radius) + radius * angle.cos(),
                    sy * (10.0 - radius) + radius * angle.sin(),
                )
            };
            pairs.push(WireSync {
                upper: Some(point(0.3)),
                lower: Some(point(0.164)),
            });
        }
    }
    for widening in [WireWidening::Uniform, WireWidening::PerRuling] {
        asymmetric_trimmed_fillet(widening, WirePairing::SyncPoints(pairs.clone()));
    }
}

#[test]
fn trimmed_preimages_do_not_depend_on_the_input_seam_or_winding() {
    for reverse in [false, true] {
        let rotate = |profile: Loop, shift| {
            let mut points = profile.points().to_vec();
            points.rotate_left(shift);
            if reverse {
                points.reverse();
            }
            Loop::new(points).expect("the same closed profile")
        };
        for widening in [WireWidening::Uniform, WireWidening::PerRuling] {
            asymmetric_profiles(
                rotate(rounded(0.3), 17),
                rotate(rounded(0.164), 63),
                widening,
                WirePairing::ArcLength,
            );
        }
    }
}

#[test]
fn a_deep_slot_still_refuses_by_name_before_emitting_a_path() {
    let notch = |half_width| {
        Loop::new(vec![
            Point2::new(-20.0, -10.0),
            Point2::new(20.0, -10.0),
            Point2::new(20.0, 10.0),
            Point2::new(half_width, 10.0),
            Point2::new(half_width, 5.0),
            Point2::new(-half_width, 5.0),
            Point2::new(-half_width, 10.0),
            Point2::new(-20.0, 10.0),
        ])
        .expect("a simple notched rectangle")
    };
    let (upper, lower) = (notch(0.5), notch(0.1));
    let pairing = WirePairing::SyncPoints(
        upper
            .points()
            .iter()
            .zip(lower.points())
            .map(|(u, l)| WireSync {
                upper: Some(*u),
                lower: Some(*l),
            })
            .collect(),
    );
    for widening in [WireWidening::Uniform, WireWidening::PerRuling] {
        let r = WireRuled {
            widening,
            ..ruled(pairing.clone())
        };
        let why = super::ruled_tests::cut(&upper, &lower, &r)
            .expect_err("a deep bridged slot cannot leave a path")
            .to_string();
        assert!(
            why.contains("inside corner or slot") && why.contains("tighter than the wire can cut"),
            "the refusal names the unsafe feature: {why}"
        );
    }
}

#[test]
fn clearance_detects_a_midheight_gouge_even_when_both_plane_rings_clear() {
    let given = vec![
        Point2::new(-20.0, -10.0),
        Point2::new(20.0, -10.0),
        Point2::new(20.0, 10.0),
        Point2::new(1.0, 10.0),
        Point2::new(1.0, 5.0),
        Point2::new(-1.0, 5.0),
        Point2::new(-1.0, 10.0),
        Point2::new(-20.0, 10.0),
    ];
    let profile = Loop::new(given.clone()).expect("a deep open slot");
    let region = Region::new(vec![profile]).expect("slot region");
    let p = params();
    let offset =
        wire_ring(&region, p.side, p.kerf_offset()).expect("the wire fits the slot on both planes");
    // Each endpoint traces the same legal parallel, but an incorrect
    // one-vertex phase shift sews across the deep slot inside the height.
    for i in 0..offset.len() {
        let mid = super::ruled_laws::lerp(offset[i], offset[(i + 1) % offset.len()], 0.5);
        for q in [offset[i], mid] {
            assert!(
                super::ring::pt_ring(q, &given) >= p.kerf_offset() - 1e-6,
                "both plane rings clear before the sewing"
            );
        }
    }
    let product: Vec<_> = given.iter().map(|q| (*q, *q)).collect();
    let wire: Vec<_> = offset
        .iter()
        .enumerate()
        .map(|(i, q)| (*q, offset[(i + 1) % offset.len()]))
        .collect();
    let read = clearance(
        &product,
        &wire,
        p.start.expect("start"),
        p.kerf_offset(),
        0.0,
        12.0,
    );
    assert!(
        read.ratio > 1.01,
        "the 33-height reading must find the bridged-wall gouge: {}",
        read.ratio
    );
    assert!(
        read.at.z > 0.0 && read.at.z < 12.0,
        "the gouge is inside the part height: {:?}",
        read.at
    );
    let r = ruled(WirePairing::ArcLength);
    let why = super::ruled::too_far(&r, read.at, p.kerf_offset() * (read.ratio - 1.0)).to_string();
    assert!(
        why.contains("twists the surface too far") && why.contains("keeping the kerf out"),
        "the gouge has a named refusal: {why}"
    );
}
