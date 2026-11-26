//! #3908: the flute rule of the 3D tool preview - the count, the helix,
//! the plain kinds, the facet bound, and that every fluted point stays
//! inside the conservative revolved envelope the laws read.
//! Provenance: our own tests over the shipped catalog tools.

use super::*;
use camcraft_app::tools::catalog;

fn shipped(name: &str) -> Tool {
    match catalog().into_iter().find(|(n, _)| *n == name) {
        Some((_, t)) => t,
        None => panic!("{name} is a shipped tool"),
    }
}

/// `name` with `n` flutes and the helix `helix` degrees.
fn with(name: &str, n: Option<u32>, helix: Option<f64>) -> Tool {
    let mut t = shipped(name);
    t.flutes = n;
    t.catalog.helix_deg = helix;
    t
}

/// The steel meridian as the preview revolves it: the profile to the
/// shank top, closed on the axis.
fn steel(t: &Tool) -> Vec<(f64, f64)> {
    let mut p = Tool { holder_len: 0.0, ..t.clone() }.profile();
    let top = t.flute_len + t.shank_len;
    p.push((top, 0.0));
    p
}

/// The vertices `(angle, radius)` of the ring nearest height `z`.
fn ring(mesh: &[Triangle], z: f64) -> Vec<(f64, f64)> {
    let pts: Vec<Point3> = mesh.iter().flat_map(|t| [t.a, t.b, t.c]).collect();
    let near = pts.iter().map(|p| p.z).min_by(|a, b| (a - z).abs().total_cmp(&(b - z).abs())).unwrap_or(z);
    let mut v: Vec<(f64, f64)> = pts
        .iter()
        .filter(|p| (p.z - near).abs() < 1e-9)
        .map(|p| (p.y.atan2(p.x).rem_euclid(TAU), p.x.hypot(p.y)))
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);
    v
}

/// The grooves around a ring: the runs below the land radius `r`.
fn grooves(ring: &[(f64, f64)], r: f64) -> usize {
    let deep: Vec<bool> = ring.iter().map(|&(_, q)| q < r * 0.99).collect();
    (0..deep.len()).filter(|&i| deep[i] && !deep[(i + deep.len() - 1) % deep.len()]).count()
}

/// The angle of the deepest point of a ring.
fn floor_at(ring: &[(f64, f64)]) -> f64 {
    ring.iter().copied().min_by(|a, b| a.1.total_cmp(&b.1)).map_or(f64::NAN, |p| p.0)
}

#[test]
fn an_end_mill_shows_its_flute_count_on_the_fluted_length() {
    for n in [2, 3, 4, 6] {
        let t = with("EM-1000-4FL", Some(n), Some(30.0));
        let Some(m) = fluted(&t, &steel(&t)) else { panic!("{n} flutes are drawn") };
        let mid = ring(&m, t.flute_len * 0.5);
        assert_eq!(grooves(&mid, t.diameter * 0.5), n as usize, "{n} grooves at mid flute");
        let floor = mid.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        assert!((floor - CORE * t.diameter * 0.5).abs() < 0.1, "the groove floor is the core: {floor}");
        let shank = ring(&m, t.flute_len + t.shank_len * 0.5);
        assert_eq!(grooves(&shank, t.shank_d * 0.5), 0, "the shank is plain");
    }
}

#[test]
fn helix_zero_cuts_straight_grooves_and_a_helix_twists_them() {
    let t = with("EM-1000-4FL", Some(4), Some(0.0));
    let m = fluted(&t, &steel(&t)).unwrap_or_default();
    let (z0, z1) = (t.flute_len * 0.2, t.flute_len * 0.7);
    let (a0, a1) = (floor_at(&ring(&m, z0)), floor_at(&ring(&m, z1)));
    assert!((a0 - a1).abs() < 1e-9, "a straight groove keeps its angle: {a0} {a1}");
    let unstated = with("EM-1000-4FL", Some(4), None);
    assert_eq!(fluted(&unstated, &steel(&unstated)), Some(m), "an unknown helix draws straight flutes");
    let h = with("EM-1000-4FL", Some(4), Some(30.0));
    let g = Grooves::of(&h).unwrap_or_else(|| panic!("the helix tool has grooves"));
    assert!((g.twist - 30f64.to_radians().tan() / 5.0).abs() < 1e-12, "the twist is tan(helix) / R");
    // A right-hand helix: the groove floor turns by the twist as it rises.
    let (b0, b1) = (g.scale(0.0, z0), g.scale(g.twist * (z1 - z0), z1));
    assert!((b0 - b1).abs() < 1e-9, "the groove follows the helix: {b0} {b1}");
}

#[test]
fn the_fluted_steel_stays_inside_the_revolved_envelope() {
    for name in ["EM-1000-4FL", "BB-1000-R1", "CM-0600-90", "TN-1600-0R8", "EM-0200-2FL"] {
        let t = with(name, Some(3), Some(40.0));
        let prof = steel(&t);
        let Some(m) = fluted(&t, &prof) else { continue };
        let env = |z: f64| {
            prof.windows(2)
                .filter(|w| (w[0].0.min(w[1].0) - 1e-9..=w[0].0.max(w[1].0) + 1e-9).contains(&z))
                .map(|w| w[0].1.max(w[1].1))
                .fold(0.0, f64::max)
        };
        for p in m.iter().flat_map(|q| [q.a, q.b, q.c]) {
            assert!(p.x.hypot(p.y) <= env(p.z) + 1e-9, "{name}: {p:?} leaves the envelope");
        }
    }
}

#[test]
fn tools_without_flutes_draw_the_plain_model() {
    for name in ["TD-1732-60", "TM-0600-P125", "FG-0600-04", "TS-2000-0600", "DT-2000-60"] {
        let t = with(name, Some(4), Some(30.0));
        assert_eq!(fluted(&t, &steel(&t)), None, "{name} ({:?}) draws no flutes", t.kind);
    }
    let em = |n| with("EM-1000-4FL", n, Some(30.0));
    for n in [None, Some(0), Some(MAX_FLUTES + 1)] {
        assert_eq!(fluted(&em(n), &steel(&em(n))), None, "{n:?} flutes draw none");
    }
}

#[test]
fn the_fluted_steel_keeps_to_its_facet_bound() {
    for name in ["EM-1000-4FL", "BB-1000-R1", "CM-0600-90", "EM-0200-2FL"] {
        let t = with(name, Some(MAX_FLUTES), Some(80.0));
        let n = fluted(&t, &steel(&t)).map_or(0, |m| m.len());
        assert!(n > 0 && n <= MAX_TRIS, "{name}: {n} facets, bound {MAX_TRIS}");
    }
}
