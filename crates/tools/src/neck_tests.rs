//! Unit tests of the necked shank law (#852): a drawn neck narrower than
//! the shank is read at the neck, never wider than the shank, and a tool
//! with no neck (or one that no longer agrees) reads the stated cylinder
//! exactly.

use super::*;
use crate::spun::SpunPoint;
use crate::tools::catalog;

fn p(z: f64, r: f64) -> SpunPoint {
    SpunPoint::line(z, r)
}

fn em10() -> Tool {
    catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).unwrap()
}

/// EM-1000-4FL (Ø10 x 22 flutes, Ø10 shank x 40) with a Ø8 neck from
/// 22 to 30.
fn necked() -> SpunProfile {
    cutter_profile::check(
        &em10(),
        vec![p(0.0, 0.0), p(0.0, 5.0), p(22.0, 5.0), p(22.0, 4.0), p(30.0, 4.0), p(30.0, 5.0), p(62.0, 5.0), p(62.0, 0.0)],
    )
    .unwrap()
}

fn band(above: f64, len: f64, r: f64) -> HolderBand {
    HolderBand { above, len, r }
}

#[test]
fn a_drawn_neck_is_read_at_the_neck_then_the_shank() {
    assert_eq!(neck_bands(&em10(), &necked()), vec![band(22.0, 8.0, 4.0), band(30.0, 32.0, 5.0)]);
}

#[test]
fn the_tools_own_numbers_carry_no_neck() {
    for (name, t) in catalog() {
        let Some(s) = SpunProfile::cutter_of(&t) else { continue };
        if cutter_profile::agrees(&t, &s).is_ok() {
            assert!(neck_bands(&t, &s).is_empty(), "{name}: the parametric outline narrows nowhere");
        }
    }
}

#[test]
fn a_neck_that_no_longer_agrees_is_dropped() {
    let longer = Tool { shank_len: 50.0, ..em10() };
    assert!(neck_bands(&longer, &necked()).is_empty());
}

#[test]
fn no_neck_reads_the_stated_cylinder_bit_for_bit() {
    let g = em10().geom();
    let s = HolderStack::cylinder(&g);
    assert_eq!(s.shank_bands(&g), vec![band(g.flute_len, g.shank_len, g.shank_radius)]);
}

#[test]
fn a_fitting_neck_is_swept_and_a_misfit_falls_back() {
    let t = em10();
    let g = t.geom();
    let neck = neck_bands(&t, &necked());
    let s = HolderStack::cylinder(&g).with_neck(neck.clone());
    assert_eq!(s.shank_bands(&g), neck);
    let cyl = vec![band(22.0, 40.0, 5.0)];
    // Not contiguous, too wide, non-finite, short of the stickout.
    for bad in [
        vec![band(22.0, 8.0, 4.0), band(31.0, 31.0, 5.0)],
        vec![band(22.0, 8.0, 6.0), band(30.0, 32.0, 5.0)],
        vec![band(22.0, 8.0, f64::NAN), band(30.0, 32.0, 5.0)],
        vec![band(22.0, 8.0, 4.0)],
    ] {
        assert_eq!(HolderStack::cylinder(&g).with_neck(bad).shank_bands(&g), cyl);
    }
}

#[test]
fn a_body_beside_the_neck_widens_it() {
    let t = em10();
    let g = t.geom();
    let body = HolderStack { bands: vec![band(26.0, 60.0, 4.5)], top: 86.0, neck: Vec::new() };
    let s = body.with_neck(neck_bands(&t, &necked()));
    assert_eq!(s.shank_bands(&g), vec![band(22.0, 8.0, 4.5), band(30.0, 32.0, 5.0)]);
}
