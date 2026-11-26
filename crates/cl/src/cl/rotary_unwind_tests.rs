//! Unit pins of `unwind_c` (issue #33 slice 2, #2828 the radial
//! link): the whole-turn rewind at clear links - axial, and on the
//! retract cylinder the flow kernel retracts to by feed - the
//! stretch wider than the travel, the broken chain, the non-finite
//! point, all refused by name. Our own fixtures; no vendor material.

use super::*;
use crate::cl::stamp_c;

/// `rings` full rings at r 10, each a stretch of 12 feeds, joined
/// by a link that rises to z 5, steps down a ring, and comes back.
fn rings(rings: usize) -> Toolpath {
    let at = |k: usize, z: f64| {
        let th = (k as f64 * 30.0).to_radians();
        Point3::new(10.0 * th.cos(), 10.0 * th.sin(), z)
    };
    let radial = |p: Point3| [p.x / 10.0, p.y / 10.0, 0.0];
    let mut moves: Vec<Move> = Vec::new();
    for ring in 0..rings {
        let z = -(ring as f64);
        if let Some(end) = moves.last().map(|m| m.to) {
            let top = at(0, 5.0);
            moves.push(Move::new(MoveKind::Rapid, end, top, None).with_axis([1.0, 0.0, 0.0]));
            // The contour kernel's shape: up by rapid, down by plunge.
            moves.push(Move::new(MoveKind::Plunge, top, at(0, z), Some(100.0)).with_axis([1.0, 0.0, 0.0]));
        }
        for k in 0..12 {
            let to = if k == 11 { at(0, z) } else { at(k + 1, z) };
            moves.push(Move::new(MoveKind::Feed, at(k, z), to, Some(400.0)).with_axis(radial(to)));
        }
    }
    stamp_c(&Toolpath::new(moves), [0.0, 0.0], 0.01).unwrap_or_else(|e| panic!("stamps: {e}"))
}

/// Three rings at r 10 (z 0, -1, -2) joined by RADIAL links: out
/// along +X to `out_r`, down a ring, back in - no rapid rises
/// above the cut's own Z, so only the radius can be a clearance.
fn radial_rings(out_r: f64) -> Toolpath {
    let at = |k: usize, z: f64| {
        let th = (k as f64 * 30.0).to_radians();
        Point3::new(10.0 * th.cos(), 10.0 * th.sin(), z)
    };
    let radial = |p: Point3| {
        let r = p.x.hypot(p.y);
        [p.x / r, p.y / r, 0.0]
    };
    let mut moves: Vec<Move> = Vec::new();
    for ring in 0..3 {
        let z = -(ring as f64);
        if let Some(end) = moves.last().map(|m| m.to) {
            let (a, b) = (Point3::new(out_r, 0.0, end.z), Point3::new(out_r, 0.0, z));
            moves.push(Move::new(MoveKind::Rapid, end, a, None).with_axis(radial(a)));
            moves.push(Move::new(MoveKind::Rapid, a, b, None).with_axis(radial(b)));
            moves.push(Move::new(MoveKind::Rapid, b, at(0, z), None).with_axis(radial(at(0, z))));
        }
        for k in 0..12 {
            let to = if k == 11 { at(0, z) } else { at(k + 1, z) };
            moves.push(Move::new(MoveKind::Feed, at(k, z), to, Some(400.0)).with_axis(radial(to)));
        }
    }
    stamp_c(&Toolpath::new(moves), [0.0, 0.0], 0.01).unwrap_or_else(|e| panic!("stamps: {e}"))
}

#[test]
fn a_radial_link_beyond_the_cut_unwinds_at_its_radius_when_admitted() {
    let raw = radial_rings(20.0);
    assert!(c_range(&raw).0 < -1000.0, "three rings read -1080: {:?}", c_range(&raw));
    let t = unwind_c(&raw, [0.0, 0.0], (-360.0, 360.0), 0.01, true).unwrap_or_else(|e| panic!("{e}"));
    let (l, h) = c_range(&t);
    assert!(l >= -360.0 - 1e-9 && h <= 360.0 + 1e-9, "every C inside the travel: {l}..{h}");
    for w in t.moves.windows(2) {
        assert_eq!(w[0].to, w[1].from, "the chain holds");
    }
    // The rewind: rapid chords on the r 20 cylinder, at the link's Z.
    let added = t.moves.len() - raw.moves.len();
    let orbit: Vec<&Move> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid && m.from.z == m.to.z && m.from.x.hypot(m.from.y) > 15.0 && m.to.x.hypot(m.to.y) > 15.0)
        .collect();
    assert!(added > 0 && orbit.len() == added, "every added move is an orbit chord: {added} vs {}", orbit.len());
    for m in &orbit {
        for p in [m.from, m.to] {
            assert!((p.x.hypot(p.y) - 20.0).abs() < 1e-9, "on the clearance radius: {p:?}");
        }
    }
}

/// #2828: the flow kernel's own link shape - a FEED retract along
/// the radial to the `out_r` cylinder, rapid index chords on it,
/// a plunge down. The retract's end is the arrival at rest, not a
/// cut point (the mirror of the plunge's start), so the cylinder
/// stays a clearance the rewind orbits on - where the rapid-link
/// shape of `radial_rings` proved the same cylinder before.
fn feed_retract_rings(out_r: f64) -> Toolpath {
    let at = |k: usize, z: f64| {
        let th = (k as f64 * 30.0).to_radians();
        Point3::new(10.0 * th.cos(), 10.0 * th.sin(), z)
    };
    let radial = |p: Point3| {
        let r = p.x.hypot(p.y);
        [p.x / r, p.y / r, 0.0]
    };
    let mut moves: Vec<Move> = Vec::new();
    for ring in 0..3 {
        let z = -(ring as f64);
        if let Some(end) = moves.last().map(|m| m.to) {
            let (a, b) = (Point3::new(out_r, 0.0, end.z), Point3::new(out_r, 0.0, z));
            moves.push(Move::new(MoveKind::Feed, end, a, Some(400.0)).with_axis(radial(a)));
            moves.push(Move::new(MoveKind::Rapid, a, b, None).with_axis(radial(b)));
            moves.push(Move::new(MoveKind::Rapid, b, at(0, z), None).with_axis(radial(at(0, z))));
        }
        for k in 0..12 {
            let to = if k == 11 { at(0, z) } else { at(k + 1, z) };
            moves.push(Move::new(MoveKind::Feed, at(k, z), to, Some(400.0)).with_axis(radial(to)));
        }
    }
    stamp_c(&Toolpath::new(moves), [0.0, 0.0], 0.01).unwrap_or_else(|e| panic!("stamps: {e}"))
}

#[test]
fn a_feed_retracts_arrival_at_rest_stays_a_clearance() {
    let raw = feed_retract_rings(20.0);
    assert!(c_range(&raw).0 < -1000.0, "three rings read -1080: {:?}", c_range(&raw));
    let t = unwind_c(&raw, [0.0, 0.0], (-360.0, 360.0), 0.01, true).unwrap_or_else(|e| panic!("{e}"));
    let (l, h) = c_range(&t);
    assert!(l >= -360.0 - 1e-9 && h <= 360.0 + 1e-9, "every C inside the travel: {l}..{h}");
    for w in t.moves.windows(2) {
        assert_eq!(w[0].to, w[1].from, "the chain holds");
    }
    // The rewind orbits sit on the retract cylinder the feed
    // arrived at, at the link's own Z.
    let orbit: Vec<&Move> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid && m.from.z == m.to.z && m.from.x.hypot(m.from.y) > 15.0 && m.to.x.hypot(m.to.y) > 15.0)
        .collect();
    assert!(!orbit.is_empty(), "the rewinds are orbit chords on the cylinder");
    for m in &orbit {
        for p in [m.from, m.to] {
            assert!((p.x.hypot(p.y) - 20.0).abs() < 1e-9, "on the clearance radius: {p:?}");
        }
    }
    // The same shape with the radial links not taken: refused by
    // name, as the contour's own law answers it.
    let e = unwind_c(&raw, [0.0, 0.0], (-360.0, 360.0), 0.01, false).err().unwrap_or_default();
    assert!(e.contains("no clear link stands inside it") && e.contains("radial links are not taken for this kind"), "{e}");
}

#[test]
fn a_radial_link_at_or_inside_the_cut_radius_is_no_link() {
    // At the largest cut radius: not strictly beyond it.
    let e = unwind_c(&radial_rings(10.0), [0.0, 0.0], (-360.0, 360.0), 0.01, true).err().unwrap_or_default();
    assert!(e.contains("no clear link stands inside it"), "{e}");
    // Inside it.
    let e = unwind_c(&radial_rings(5.0), [0.0, 0.0], (-360.0, 360.0), 0.01, true).err().unwrap_or_default();
    assert!(e.contains("no clear link stands inside it"), "{e}");
}

#[test]
fn a_radial_only_link_is_refused_by_name_where_radial_links_are_not_admitted() {
    // Rotary Contour's own call: axial links only.
    let e = unwind_c(&radial_rings(20.0), [0.0, 0.0], (-360.0, 360.0), 0.01, false).err().unwrap_or_default();
    assert!(e.contains("no clear link stands inside it") && e.contains("radial links are not taken for this kind"), "{e}");
}

fn c_range(t: &Toolpath) -> (f64, f64) {
    t.moves
        .iter()
        .filter_map(|m| m.rotary)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), c| (l.min(c), h.max(c)))
}

#[test]
fn three_rings_unwind_at_their_links_and_stay_a_chain() {
    let raw = rings(3);
    assert!(c_range(&raw).0 < -1000.0, "three rings read past -1000: {:?}", c_range(&raw));
    let t = unwind_c(&raw, [0.0, 0.0], (-360.0, 360.0), 0.01, false).unwrap_or_else(|e| panic!("{e}"));
    let (l, h) = c_range(&t);
    assert!(l >= -360.0 - 1e-9 && h <= 360.0 + 1e-9, "{l}..{h}");
    for w in t.moves.windows(2) {
        assert_eq!(w[0].to, w[1].from, "the chain holds");
    }
    // The rewinds are real moves: rapid chords at the clearance
    // height, two of them (one per link), and C stays continuous.
    let orbit = t.moves.iter().filter(|m| m.from.z == 5.0 && m.to.z == 5.0).count();
    assert_eq!(t.moves.len(), raw.moves.len() + orbit, "only orbit chords were added");
    assert!(orbit >= 2 * 360 / 6, "two whole-turn orbits split to tol: {orbit}");
    let worst = t.moves.windows(2).filter_map(|w| Some((w[1].rotary? - w[0].rotary?).abs())).fold(0.0, f64::max);
    assert!(worst < 31.0, "C stays continuous: worst step {worst}");
}

#[test]
fn a_path_inside_its_travel_comes_back_unchanged() {
    let raw = rings(1);
    let t = unwind_c(&raw, [0.0, 0.0], (-360.0, 360.0), 0.01, false).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(t.moves, raw.moves);
}

#[test]
fn a_ring_wider_than_the_travel_refuses_by_name() {
    let e = unwind_c(&rings(2), [0.0, 0.0], (-90.0, 90.0), 0.01, false).err().unwrap_or_default();
    assert!(e.contains("no clear link stands inside it"), "{e}");
    let e = unwind_c(&rings(1), [0.0, 0.0], (10.0, -10.0), 0.01, false).err().unwrap_or_default();
    assert!(e.contains("is not a range"), "{e}");
}

#[test]
fn a_non_finite_point_refuses_by_name() {
    let mut t = rings(2);
    if let Some(m) = t.moves.get_mut(3) {
        m.to.y = f64::NAN;
    }
    let e = unwind_c(&t, [0.0, 0.0], (-360.0, 360.0), 0.01, false).err().unwrap_or_default();
    assert!(e.contains("move 3 has a non-finite point"), "{e}");
}

#[test]
fn a_broken_chain_refuses_by_name() {
    let mut t = rings(2);
    if let Some(m) = t.moves.get_mut(5) {
        m.from.z += 0.5;
    }
    let e = unwind_c(&t, [0.0, 0.0], (-360.0, 360.0), 0.01, false).err().unwrap_or_default();
    assert!(e.contains("move 5 does not start where move 4 ended"), "{e}");
}
