//! G76 continuous coverage regressions from meridian insert and drill geometry;
//! original fixtures, with no vendor material.
//! Gate map (M4.4): covers camcraft_motion::turning::g76_clearance.

use camcraft_motion::turning::g76_clearance::{rectangle_clear, AheadCut, PilotCut};

const BAND: (f64, f64, f64, f64) = (13.134, 14.866, 4.8, 6.0);
const MARGIN: f64 = 0.02;

fn nose_pass(radius: f64, nose: f64) -> AheadCut {
    AheadCut {
        p: (12.0, radius),
        q: (16.0, radius),
        nose,
        band: 0.0,
    }
}

#[test]
fn radial_gap_between_prior_cut_capsules_refuses() {
    // Leave R5.14..5.16 uncut, between the old radial stations.
    // The old sampled check also inflated each nose by .02, hiding this gap.
    let gap = [nose_pass(4.85, 0.29), nose_pass(5.61, 0.45)];
    assert!(
        !rectangle_clear(BAND, &gap, &[], MARGIN),
        "a radial gap between swept inserts must refuse"
    );
    let overlap = [nose_pass(4.85, 0.4), nose_pass(5.61, 0.5)];
    assert!(
        rectangle_clear(BAND, &overlap, &[], MARGIN),
        "overlapping cuts covering the whole band must pass"
    );
}

#[test]
fn diagonal_blade_does_not_credit_its_deep_tip_across_the_whole_band() {
    let cut = [AheadCut {
        p: (12.5, 6.05),
        q: (15.5, 4.5),
        nose: 0.0,
        band: 0.1,
    }];
    assert!(
        !rectangle_clear(BAND, &cut, &[], MARGIN),
        "a sloped blade sweep leaves material near its shallow end"
    );
    let plunge = [AheadCut {
        p: (14.0, 6.05),
        q: (14.0, 4.5),
        nose: 0.0,
        band: 1.5,
    }];
    assert!(
        rectangle_clear(BAND, &plunge, &[], MARGIN),
        "a wide plunge covers the whole band"
    );
}

#[test]
fn pilot_values_prove_only_the_removed_cone() {
    let pilot = [PilotCut {
        lo: 12.0,
        hi: 16.0,
        r: 6.5,
        cot: Some(0.1),
    }];
    assert!(rectangle_clear(BAND, &[], &pilot, MARGIN));
    let narrower = [PilotCut {
        lo: 12.0,
        hi: 16.0,
        r: 5.0,
        cot: Some(0.2),
    }];
    assert!(
        !rectangle_clear(BAND, &[], &narrower, MARGIN),
        "a pilot cannot clear stock outside its radius"
    );
    assert!(
        !rectangle_clear((12.0, 12.5, 4.8, 6.0), &[], &pilot, MARGIN),
        "a pilot cannot credit its full radius at the cone tip"
    );
}
