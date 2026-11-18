//! Unit tests of the revolved envelope (#368): conservative, merged,
//! refusing by name.

use super::*;
use crate::tools::catalog;

/// A closed `n`-gon prism of circumradius `r` about the axis over `[z0, z1]`.
fn prism(r: f64, z0: f64, z1: f64, n: usize) -> Vec<Tri> {
    let p = |k: usize, z: f64| {
        let a = std::f64::consts::TAU * k as f64 / n as f64;
        [r * a.cos(), r * a.sin(), z]
    };
    let mut v = Vec::new();
    for k in 0..n {
        let (a0, a1, b0, b1) = (p(k, z0), p(k + 1, z0), p(k, z1), p(k + 1, z1));
        v.push([a0, a1, b1]);
        v.push([a0, b1, b0]);
        v.push([[0.0, 0.0, z0], a1, a0]);
        v.push([[0.0, 0.0, z1], b0, b1]);
    }
    v
}

fn band_at(p: &HolderProfile, z: f64) -> f64 {
    p.reach_over(z, z)
}

#[test]
fn facet_crossing_a_band_with_no_vertex_in_it_still_counts() {
    // A small spindle round the axis over 0..10, and one long sliver at
    // radius 8 whose only vertices sit at z = 0 and z = 10: every band
    // between holds none of its vertices, and must still reach 8.
    let mut tris = prism(1.0, 0.0, 10.0, 8);
    tris.push([[8.0, 0.0, 0.0], [8.0, 0.5, 0.0], [8.0, 0.0, 10.0]]);
    let p = HolderProfile::envelope(&tris).expect("a sane body");
    for k in 0..=100 {
        let z = f64::from(k) * 0.1;
        assert!(band_at(&p, z) >= 8.0, "z {z}: reach {}", band_at(&p, z));
    }
}

#[test]
fn random_facet_points_never_stand_outside_their_band() {
    // Every point of every facet lies within the band holding its height.
    let mut s = 0x2545_f491_4f6c_dd1d_u64;
    let mut rnd = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut tris = prism(2.0, 0.0, 40.0, 6);
    for _ in 0..200 {
        let mut t = [[0.0; 3]; 3];
        for v in &mut t {
            *v = [rnd() * 40.0 - 20.0, rnd() * 40.0 - 20.0, rnd() * 40.0];
        }
        tris.push(t);
    }
    let p = HolderProfile::envelope(&tris).expect("a sane body");
    assert!(p.bands().len() <= MAX_BANDS);
    for t in &tris {
        for _ in 0..20 {
            let (mut a, mut b) = (rnd(), rnd());
            if a + b > 1.0 {
                (a, b) = (1.0 - a, 1.0 - b);
            }
            let c = 1.0 - a - b;
            let q: Vec<f64> = (0..3).map(|k| a * t[0][k] + b * t[1][k] + c * t[2][k]).collect();
            let r = q[0].hypot(q[1]);
            // Barycentric float dust can step a hair past the body's ends.
            let z = q[2].clamp(p.bottom(), p.top());
            assert!(band_at(&p, z) >= r - 1e-9, "point {q:?} r {r} outside {}", band_at(&p, z));
        }
    }
}

#[test]
fn a_prism_reads_as_one_band() {
    let p = HolderProfile::envelope(&prism(20.0, 50.0, 80.0, 24)).expect("a sane body");
    assert_eq!(p.bands().len(), 1, "{:?}", p.bands());
    assert!((p.bottom() - 50.0).abs() < 1e-12 && (p.top() - 80.0).abs() < 1e-12);
    let r = p.bands()[0].r;
    assert!((20.0..20.0 + 1e-5).contains(&r), "{r}");
}

#[test]
fn a_stepped_body_keeps_its_steps() {
    // A nose of radius 10 over 50..70 under a flange of radius 30 over 70..80.
    let mut tris = prism(10.0, 50.0, 70.0, 16);
    tris.extend(prism(30.0, 70.0, 80.0, 16));
    let p = HolderProfile::envelope(&tris).expect("a sane body");
    assert!(band_at(&p, 60.0) < 10.5, "nose {}", band_at(&p, 60.0));
    assert!(band_at(&p, 75.0) >= 30.0, "flange {}", band_at(&p, 75.0));
}

#[test]
fn degenerate_bodies_refuse_by_name() {
    let ok = prism(5.0, 10.0, 20.0, 8);
    assert_eq!(HolderProfile::envelope(&[]), Err(EnvelopeError::Empty));
    let mut nan = ok.clone();
    nan[0][1][2] = f64::NAN;
    assert_eq!(HolderProfile::envelope(&nan), Err(EnvelopeError::NonFinite));
    let line = vec![[[-1.0, -1.0, 0.0], [0.0, 0.0, 5.0], [1.0, 1.0, 10.0]]];
    assert_eq!(HolderProfile::envelope(&line), Err(EnvelopeError::Degenerate));
    let flat = vec![[[-1.0, -1.0, 3.0], [1.0, -1.0, 3.0], [0.0, 1.0, 3.0]]];
    assert_eq!(HolderProfile::envelope(&flat), Err(EnvelopeError::Flat));
    assert_eq!(HolderProfile::envelope(&prism(5.0, -1.0, 20.0, 8)), Err(EnvelopeError::BelowTip));
    let side: Vec<Tri> = ok.iter().map(|t| t.map(|v| [v[0] + 50.0, v[1], v[2]])).collect();
    assert_eq!(HolderProfile::envelope(&side), Err(EnvelopeError::OffAxis));
    assert_eq!(HolderProfile::envelope(&prism(600.0, 0.0, 20.0, 8)), Err(EnvelopeError::TooLarge));
    for e in [EnvelopeError::Empty, EnvelopeError::OffAxis, EnvelopeError::TooLarge] {
        assert!(e.to_string().starts_with("the tool body"), "{e}");
    }
}

#[test]
fn the_enclosing_cylinder_never_shrinks_the_body() {
    let (_, t) = catalog().into_iter().next().expect("a catalog tool");
    let shank_top = t.flute_len + t.shank_len;
    // A shrink-fit nose reaching 10 mm down beside the shank, wide on top.
    let mut tris = prism(9.0, shank_top - 10.0, shank_top + 20.0, 16);
    tris.extend(prism(25.0, shank_top + 20.0, shank_top + 40.0, 16));
    let p = HolderProfile::envelope(&tris).expect("a sane body");
    let e = p.enclose(&t).expect("the body clears the flutes");
    assert!(e.shank_d >= 18.0 && e.shank_d >= t.shank_d);
    assert!(e.holder_d >= 50.0);
    assert!((e.holder_len - 40.0).abs() < 1e-9);
    assert_eq!((e.diameter, e.flute_len, e.shank_len), (t.diameter, t.flute_len, t.shank_len));
    // Over the flutes: no cylinder stack can hold it.
    let low = HolderProfile::envelope(&prism(9.0, t.flute_len - 1.0, shank_top, 8)).expect("sane");
    assert!(low.enclose(&t).is_none());
}

#[test]
fn the_stack_fills_the_gap_to_a_raised_body_and_the_cylinder_keeps_its_sums() {
    let (_, t) = catalog().into_iter().next().expect("a catalog tool");
    let g = t.geom();
    let shank_top = g.flute_len + g.shank_len;
    let p = HolderProfile::envelope(&prism(20.0, shank_top + 5.0, shank_top + 30.0, 12)).expect("sane");
    let s = p.stack(&g);
    assert_eq!(s.bands[0], HolderBand { above: shank_top, len: p.bottom() - shank_top, r: g.shank_radius });
    assert_eq!(s.top, p.top());
    let c = HolderStack::cylinder(&g);
    assert_eq!(c.bands, vec![HolderBand { above: g.flute_len + g.shank_len, len: g.holder_len, r: g.holder_radius }]);
    assert_eq!(c.top.to_bits(), (g.flute_len + g.shank_len + g.holder_len).to_bits());
}

// ---- #828: an assumed (machine-default) holder is floored ------------

/// The library holder `entry` drawn for `t` at its default gauge, as the
/// machine-default door and the explicit pick both land it.
fn library_profile(entry: &str, t: &crate::tools::Tool) -> HolderProfile {
    let e = crate::holder_library::find(entry).expect("a library holder");
    let pts = e.points(t.flute_len + t.shank_len, e.gauge_len).expect("drawn");
    crate::spun::SpunProfile::new(pts).expect("a sane outline").holder_profile()
}

fn em4() -> crate::tools::Tool {
    catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry")
}

/// Farthest the stack the fixture laws sweep reaches over `[lo, hi]`.
fn stack_reach(s: &HolderStack, lo: f64, hi: f64) -> f64 {
    s.bands.iter().filter(|b| b.above < hi && b.above + b.len > lo).fold(0.0, |a, b| a.max(b.r))
}

/// The review's band: a wall 20 mm off the tool axis, 55-65 mm above
/// the tip - inside the bare tool's 44 mm stand-in cylinder, outside the
/// ER20 nut (35 mm) and the TTS collar (38.1 mm). On a BT30 and a TTS
/// machine the assumed default convicts at least where the bare tool
/// does; the same holder picked explicitly is judged by its real shape.
#[test]
fn an_assumed_holder_convicts_the_band_above_the_shank_where_the_bare_tool_does() {
    let t = em4();
    let g = t.geom();
    let (lo, hi, wall) = (55.0, 65.0, 20.0);
    let bare = HolderStack::cylinder(&g);
    assert!(stack_reach(&bare, lo, hi) > wall, "the bare tool's stand-in cylinder reaches the wall");
    for entry in ["BT30 ER20", "TTS ER20"] {
        let real = library_profile(entry, &t);
        assert!(stack_reach(&real.stack(&g), lo, hi) < wall, "{entry}: the explicit pick is its real, narrower nut");
        let assumed = real.floored(&g);
        assert!(stack_reach(&assumed.stack(&g), lo, hi) >= stack_reach(&bare, lo, hi), "{entry}: the default convicts the band");
        // Never smaller than either, anywhere: the union.
        let top = real.top().max(bare.top);
        for k in 0..=400 {
            let z = g.flute_len + g.shank_len + (top - g.flute_len - g.shank_len) * f64::from(k) / 400.0;
            let r = assumed.reach_over(z, z);
            assert!(r >= real.reach_over(z, z) - 1e-9, "{entry} z {z}: {r} under the body");
            if z <= bare.top {
                assert!(r >= g.holder_radius - 1e-9, "{entry} z {z}: {r} under the stand-in cylinder");
            }
        }
        // The single-cylinder laws read it no smaller than the bare tool.
        let e = assumed.enclose(&t).expect("clears the flutes");
        assert!(e.holder_d >= t.holder_d && e.holder_len >= t.holder_len, "{entry}: {} x {}", e.holder_d, e.holder_len);
    }
}

#[test]
fn a_floor_with_no_cylinder_leaves_the_body_as_drawn() {
    let t = em4();
    let real = library_profile("BT30 ER20", &t);
    let g = crate::tools::Tool { holder_len: 0.0, ..t.clone() }.geom();
    assert_eq!(real.floored(&g), real);
    let g = ToolGeom { holder_radius: f64::NAN, ..t.geom() };
    assert_eq!(real.floored(&g), real);
}

#[test]
fn a_band_touching_only_the_shank_top_does_not_widen_the_judged_shank() {
    let t = em4();
    let shank_top = t.flute_len + t.shank_len;
    let pts = vec![
        crate::spun::SpunPoint::line(shank_top, 0.0),
        crate::spun::SpunPoint::line(shank_top, 10.0),
        crate::spun::SpunPoint::line(shank_top + 20.0, 30.0),
        crate::spun::SpunPoint::line(shank_top + 20.0, 0.0),
    ];
    let p = crate::spun::SpunProfile::new(pts).expect("sane cone").holder_profile();
    let e = p.enclose(&t).expect("clears the flutes");
    assert_eq!(e.shank_d, t.shank_d, "band starts at shank_top, so shank_d stays {}", t.shank_d);
    assert!(e.holder_d >= 60.0 - 1e-9, "holder_d encloses the r30 top: {}", e.holder_d);
}
