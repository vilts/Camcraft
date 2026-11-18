//! Split from `machining3d.rs` by M5.0.1; structure only.

use crate::geo2d::Point2;
use crate::stock_sim::{Columns, Dexel};
use super::*;

#[path = "crossing_hop_tests.rs"]
mod crossing_hop_tests;

#[test]
fn a_chord_over_a_step_reads_half_engagement() {
    // The chord law at its plainest: travel along a wall, half the
    // disc in standing material, half in air - the measurement says
    // 0.5 up to the 17-point quantization (8 hits, 9 misses).
    let mut dex = Dexel::block(0.0, 0.0, 16.0, 16.0, 10.0, 1.0).unwrap();
    for iy in 0..dex.ny {
        for ix in 0..dex.nx {
            dex.set(iy * dex.nx + ix, if ix < 8 { 10.0 } else { 0.0 });
        }
    }
    let ae = engagement_on(&Columns::of_grid(&dex), Point2::new(8.0, 4.0), Point2::new(8.0, 12.0), 5.0, 8.0);
    assert!((ae - 0.5).abs() < 0.05, "half-bite read {ae}");
    // Pure air, and pure solid, at the same place.
    assert_eq!(dex.cells(), dex.nx * dex.ny);
    dex.fill(0.0);
    let ae = engagement_on(&Columns::of_grid(&dex), Point2::new(8.0, 4.0), Point2::new(8.0, 12.0), 5.0, 8.0);
    assert!(ae < 1e-9, "air must read zero, got {ae}");
    for k in 0..dex.cells() {
        dex.set(k, if k < 8 * dex.nx { 10.0 } else { 0.0 });
    }
    let ae = engagement_on(&Columns::of_grid(&dex), Point2::new(8.0, 1.0), Point2::new(8.0, 2.0), 5.0, 0.5);
    assert!((ae - 1.0).abs() < 1e-9, "solid must read full, got {ae}");
}

#[test]
fn a_zero_length_planar_move_reads_as_a_full_bite() {
    // The ambiguity law: no direction, no chord normal to measure -
    // the safe read is the heavy one.
    let dex = Dexel::block(0.0, 0.0, 4.0, 4.0, 10.0, 1.0).unwrap();
    let ae = engagement_on(&Columns::of_grid(&dex), Point2::new(2.0, 2.0), Point2::new(2.0, 2.0), 5.0, 1.0);
    assert!((ae - 1.0).abs() < 1e-12, "got {ae}");
}

#[test]
fn the_sampler_reads_both_stock_engines_alike() {
    // M7.3: `engagement_on` is the same chord law on the seam's
    // column reader - on the column arm it IS the direct `Dexel` read, and the
    // field arm must answer every chord identically (the sampler is
    // quantized, so equal means equal), with a fresh shelf beside the
    // first pass: half, full, and air bites all asked.
    use crate::cl::{Move, MoveKind};
    use crate::geometry::Point3;
    use crate::stock_sim::{Backend, SimOp, Stock, FIELD_BAND};
    let block = |b| Stock::block(b, 0.0, 0.0, 12.0, 12.0, 0.0, 1.0).unwrap();
    let (mut c, mut f) = (block(Backend::Column), block(Backend::Field { band: FIELD_BAND }));
    let pass = [Move {
        rotary: None,
        kind: MoveKind::Feed,
        from: Point3::new(4.0, 2.0, -2.0),
        to: Point3::new(4.0, 10.0, -2.0),
        feed: Some(500.0),
        arc: None,
        axis: None,
    }];
    let ops = [SimOp { moves: &pass, radius: 2.0, ball: false, cone: None, corner: None }];
    c.simulate(&ops).unwrap();
    f.simulate(&ops).unwrap();
    let (cc, cf) = (c.columns().unwrap(), f.columns().unwrap());
    let dex = c.column().unwrap();
    let mut seen = Vec::new();
    for (a, b) in [((6.0, 2.0), (6.0, 10.0)), ((4.0, 3.0), (4.0, 9.0)), ((9.0, 2.0), (9.0, 10.0)), ((2.0, 6.0), (10.0, 6.0))] {
        let (a, b) = (Point2::new(a.0, a.1), Point2::new(b.0, b.1));
        let ec = engagement_on(&cc, a, b, -2.0, 2.0);
        // The law the feed pass read straight off `Dexel` before it
        // moved onto the seam (M7.3 4/6): one answer, bit for bit.
        let direct = sample_engagement(|q| dex.col(q).map(|i| dex.top_at(i)), a, b, -2.0, 2.0);
        assert_eq!(ec.to_bits(), direct.to_bits(), "the column arm is the shipped sampler");
        assert_eq!(ec, engagement_on(&cf, a, b, -2.0, 2.0), "field disagrees on {a:?} -> {b:?}");
        seen.push(ec);
    }
    // Not a vacuous agreement: the slot's own track (only its rim
    // samples stand), a half bite on its wall, a full bite in the
    // fresh shelf were all read.
    assert!(seen[1] < 0.1, "the cut's track is (nearly) air: {seen:?}");
    assert!(seen[0] > 0.3 && seen[0] < 0.7, "the wall pass is a half bite: {seen:?}");
    assert_eq!(seen[2], 1.0, "the fresh shelf is a full bite: {seen:?}");
}

/// Issue #55: the wrap's air split sorts a plane AT the part's leave
/// envelope (to the last bit of float noise) with the material planes;
/// only a plane clearly above it is an air plane.
#[test]
fn a_plane_at_the_leave_envelope_is_a_material_plane() {
    let zs = [7.000000000000001, 6.500000000000001, 6.000000000000001, 5.5];
    assert_eq!(super::entry::air_levels(&zs, 6.5), 1, "6.5 + 1 ulp is the envelope's own plane");
    assert_eq!(super::entry::air_levels(&zs, 6.4), 2, "a plane 0.1 above is air");
    assert_eq!(super::entry::air_levels(&zs, 7.5), 0, "no plane above the envelope");
}
