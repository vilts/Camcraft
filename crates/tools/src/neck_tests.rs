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

fn spun_profile(pts: &[(f64, f64)]) -> HolderProfile {
    let pts: Vec<SpunPoint> = pts.iter().map(|&(z, r)| p(z, r)).collect();
    SpunProfile::new(pts).expect("valid spun profile").holder_profile()
}

#[test]
fn shank_with_body_splits_only_above_a_holder_foot_beside_the_shank() {
    let t = Tool {
        diameter: 6.35,
        flute_len: 14.478,
        shank_d: 6.35,
        shank_len: 50.8,
        holder_d: 35.0,
        holder_len: 29.722,
        ..em10()
    };
    let stored = t.geom();
    let profile = spun_profile(&[(38.1, 0.0), (38.1, 17.5), (95.0, 17.5), (95.0, 0.0)]);
    let judged = profile.enclose(&t).expect("holder sits above flutes").geom();
    let combined = shank_with_body(&stored, Some(&profile), Vec::new());
    assert_eq!(combined.len(), 2, "{combined:?}");
    assert!((combined[0].above - 14.478).abs() < 1e-9 && (combined[0].len - 23.622).abs() < 1e-9);
    assert_eq!(combined[0].r, 3.175);
    assert!((combined[1].above - 38.1).abs() < 1e-9 && (combined[1].len - 27.178).abs() < 1e-9);
    assert_eq!(combined[1].r, 17.5);
    // Feeding that back through HolderStack::shank_bands(&judged) must not smear 17.5 below 38.1,
    // even when a touching band's start sits 1e-12 mm shy of the split boundary from float rounding.
    let stack = profile.stack(&judged).with_neck(combined.clone());
    assert_eq!(stack.shank_bands(&judged), combined);
    let dusty = HolderStack {
        bands: vec![band(38.1 - 1e-12, 56.9, 17.5)],
        top: 95.0,
        neck: combined.clone(),
    };
    assert_eq!(dusty.shank_bands(&judged), combined);
}

#[test]
fn shank_with_body_keeps_neck_untouched_when_holder_starts_at_shank_top() {
    let t = em10();
    let g = t.geom();
    let neck = neck_bands(&t, &necked());
    let at_top = spun_profile(&[(62.0, 0.0), (62.0, 20.0), (92.0, 20.0), (92.0, 0.0)]);
    assert!(shank_with_body(&g, Some(&at_top), Vec::new()).is_empty());
    assert_eq!(shank_with_body(&g, Some(&at_top), neck.clone()), neck);
}

#[test]
fn shank_with_body_never_shrinks_stated_shank_or_neck() {
    let t = em10();
    let g = t.geom();
    // Holder narrower than Ø10 shank (r = 3 < 5): stated Ø10 shank still stands.
    let narrow = spun_profile(&[(40.0, 0.0), (40.0, 3.0), (80.0, 3.0), (80.0, 0.0)]);
    assert!(shank_with_body(&g, Some(&narrow), Vec::new()).is_empty());

    // Stepped holder crossing both a Ø8 neck (22..30) and Ø10 shank (30..62).
    let stepped = spun_profile(&[(26.0, 0.0), (26.0, 4.5), (40.0, 4.5), (40.0, 20.0), (90.0, 20.0), (90.0, 0.0)]);
    let got = shank_with_body(&g, Some(&stepped), neck_bands(&t, &necked()));
    assert_eq!(
        got,
        vec![band(22.0, 4.0, 4.0), band(26.0, 4.0, 4.5), band(30.0, 10.0, 5.0), band(40.0, 22.0, 20.0)]
    );
}
