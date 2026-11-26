//! Regression fixtures for the G76 remaining-stock clearance proof; our own test geometry.

use super::*;
use crate::cl::{Move, ThreadHand};
use crate::geometry::{Point3, Triangle};
use crate::rest::TurnedAhead;

fn revolve(m: &[(f64, f64)]) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut t = Vec::new();
    for w in m.windows(2) {
        let ((ra, za), (rb, zb)) = (w[0], w[1]);
        for j in 0..16 {
            let (a1, a2, b1, b2) = (
                v(j, ra, za),
                v(j + 1, ra, za),
                v(j, rb, zb),
                v(j + 1, rb, zb),
            );
            if ra <= 1e-12 {
                t.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                t.push(Triangle::new(a1, a2, b2));
            } else {
                t.push(Triangle::new(a1, a2, b2));
                t.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    t
}

fn cycle_at(start_z: f64, end_z: f64, infeed_angle: f64) -> ThreadCycle {
    ThreadCycle {
        start: Point3::new(6.05, 0.0, start_z),
        end: Point3::new(4.8, 0.0, end_z),
        major_diameter: 12.0,
        minor_diameter: 9.6,
        lead: 2.0,
        height: 1.2,
        first_depth: 0.4,
        min_depth: 0.4,
        finish_allowance: 0.4,
        infeed_angle,
        spring_passes: 0,
        pull_out: 0.0,
        alternating: false,
        repeat_each_depth: false,
        chip_break_dips: false,
        first: 0,
        last: 0,
        hand: ThreadHand::Right,
        start_in_air: false,
    }
}

fn eval(
    part: &[Triangle],
    stock: Option<&[Triangle]>,
    ahead: &[TurnedAhead],
    rec: ThreadCycle,
    width: f64,
) -> bool {
    let mut path = Toolpath::new(vec![Move {
        kind: MoveKind::Rapid,
        from: Point3::new(6.05, 0.0, 24.0),
        to: rec.start,
        feed: None,
        arc: None,
        axis: None,
        rotary: None,
    }]);
    path.threads.push(rec);
    let cx = Meridian {
        part,
        stock,
        nose: 0.0,
        margin: 0.02,
        z_tol: 1e-3,
        drills: &[],
        band: width * 0.5,
        body: None,
        ahead,
        outward: Some((30.0_f64).to_radians().tan()),
    };
    let params = OpParams::Threading {
        tolerance: 0.05,
        clearance_z: crate::operation::Height::fixed(24.0),
        top_z: crate::operation::Height::fixed(16.0),
        bottom_z: crate::operation::Height::fixed(8.0),
        pitch: 2.0,
        depth: 1.2,
        infeed: 0.4,
        feed: 120.0,
        plunge_feed: 40.0,
        infeed_plan: Default::default(),
        hand: ThreadHand::Right,
        picks: Default::default(),
    };
    let mut prior = PriorCell::new(None);
    judge(&params, &mut path, &cx, &mut prior);
    path.threads[0].start_in_air
}

#[test]
fn relief_groove_wider_than_insert_passes_when_grooved_and_refuses_when_uncut_or_narrow() {
    // Bar Ø12 from -26..40 with a relief groove [12.5, 15.5] (3 mm wide)
    // down to R=4.5 (<= root 4.8); insert is 1.732 mm wide centered at Z=14.0
    // (band [13.134, 14.866]).
    let wide_groove = revolve(&[
        (0.0, -26.0),
        (6.0, -26.0),
        (6.0, 12.5),
        (4.5, 12.5),
        (4.5, 15.5),
        (6.0, 15.5),
        (6.0, 40.0),
        (0.0, 40.0),
    ]);
    let raw_stock = revolve(&[(0.0, -26.0), (6.0, -26.0), (6.0, 40.0), (0.0, 40.0)]);
    let rec = cycle_at(14.0, 8.0, 0.0);

    // No stock: part relief groove alone is wider than insert and <= root -> clear.
    assert!(
        eval(&wide_groove, None, &[], rec, 1.732),
        "wide relief groove in part is clear"
    );

    // Raw Ø12 stock without prior Groove op -> uncut stock stands in the groove -> refused.
    assert!(
        !eval(&wide_groove, Some(&raw_stock), &[], rec, 1.732),
        "uncut stock in relief groove must refuse"
    );

    // Prior GrooveFinish op with 3.0 mm blade (band 1.5) plunging to R=4.5 at Z=14.0 -> clear.
    let groove_cut = [TurnedAhead {
        nose: 0.0,
        band: 1.5,
        moves: vec![Move {
            kind: MoveKind::Feed,
            from: Point3::new(6.05, 0.0, 14.0),
            to: Point3::new(4.5, 0.0, 14.0),
            feed: Some(60.0),
            arc: None,
            axis: None,
            rotary: None,
        }],
    }];
    assert!(
        eval(&wide_groove, Some(&raw_stock), &groove_cut, rec, 1.732),
        "wide relief groove cleared by prior Groove op must pass"
    );

    // Narrow relief groove [13.5, 14.5] (1 mm wide < 1.732 mm insert) -> refused.
    let narrow_groove = revolve(&[
        (0.0, -26.0),
        (6.0, -26.0),
        (6.0, 13.5),
        (4.5, 13.5),
        (4.5, 14.5),
        (6.0, 14.5),
        (6.0, 40.0),
        (0.0, 40.0),
    ]);
    assert!(
        !eval(&narrow_groove, None, &[], rec, 1.732),
        "relief groove narrower than insert must refuse"
    );

    // Shallow relief groove [12.5, 15.5] at R=5.2 (> root 4.8) -> refused.
    let shallow_groove = revolve(&[
        (0.0, -26.0),
        (6.0, -26.0),
        (6.0, 12.5),
        (5.2, 12.5),
        (5.2, 15.5),
        (6.0, 15.5),
        (6.0, 40.0),
        (0.0, 40.0),
    ]);
    assert!(
        !eval(&shallow_groove, None, &[], rec, 1.732),
        "relief groove shallower than root must refuse"
    );
}

#[test]
fn flank_infeed_refuses_when_deep_pass_overlaps_end_face() {
    // Bar ends at Z=14.693. With 55-degree flank angle, height=1.2 shifts
    // the deepest pass by 1.2 * tan(27.5 deg) = 0.625 mm toward -Z.
    let bar = revolve(&[(0.0, -26.0), (6.0, -26.0), (6.0, 14.693), (0.0, 14.693)]);
    // Start A at Z=15.8 (itself clear of 14.693 with half=0.866 -> 14.934),
    // but deepest pass enters at 15.8 - 0.625 = 15.175 -> band_lo = 14.309 < 14.693!
    let overlaps = cycle_at(15.8, 8.0, 55.0);
    assert!(
        !eval(&bar, Some(&bar), &[], overlaps, 1.732),
        "flank pass overlapping end face must refuse"
    );

    // Start A at Z=16.625 -> deepest pass enters at 16.0 -> band_lo = 15.134 > 14.693!
    let clear = cycle_at(16.625, 8.0, 55.0);
    assert!(
        eval(&bar, Some(&bar), &[], clear, 1.732),
        "flank pass clear of end face must pass"
    );
}

#[test]
fn narrow_stock_rib_between_old_stations_refuses() {
    let part = revolve(&[
        (0.0, -26.0),
        (6.0, -26.0),
        (6.0, 12.5),
        (4.5, 12.5),
        (4.5, 15.5),
        (6.0, 15.5),
        (6.0, 40.0),
        (0.0, 40.0),
    ]);
    let stock = revolve(&[
        (0.0, -26.0),
        (6.0, -26.0),
        (6.0, 12.5),
        (4.5, 12.5),
        (4.5, 13.16),
        (6.0, 13.16),
        (6.0, 13.20),
        (4.5, 13.20),
        (4.5, 15.5),
        (6.0, 15.5),
        (6.0, 40.0),
        (0.0, 40.0),
    ]);
    assert!(
        !eval(&part, Some(&stock), &[], cycle_at(14.0, 8.0, 0.0), 1.732),
        "material between axial samples must refuse"
    );
    assert!(
        eval(&part, Some(&part), &[], cycle_at(14.0, 8.0, 0.0), 1.732),
        "a genuinely clear stock groove must pass"
    );
}
