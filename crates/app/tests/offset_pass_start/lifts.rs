//! #3747: the lifts of a 2D Offset path inside one level, and the lifts
//! that its link notes name. A lift leaves a floor plane with a planar cut
//! at that plane still to come (a level change is not a lift). Provenance:
//! our own test meter; no vendor material consulted.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::geo2d::{Point2, Region};
use camcraft_app::geometry::Point3;

/// Is move `i` a planar feed at `z`?
fn floor_at(t: &Toolpath, i: usize, z: f64) -> bool {
    t.moves.get(i).is_some_and(|m| m.kind != MoveKind::Rapid && (m.from.z - z).abs() < 1e-9 && (m.to.z - z).abs() < 1e-9)
}

/// The lifts inside a level: each move that rises off a planar feed at
/// `z` when another planar feed at `z` comes later.
pub fn level_lifts(t: &Toolpath) -> usize {
    let n = t.moves.len();
    (0..n)
        .filter(|&i| {
            let m = &t.moves[i];
            let z = m.from.z;
            m.to.z > z + 1e-6 && i > 0 && floor_at(t, i - 1, z) && (i + 1..n).any(|k| floor_at(t, k, z))
        })
        .count()
}

/// The lifts that the link notes name ("link: ... N time(s) ...").
pub fn named_lifts(t: &Toolpath) -> usize {
    t.notes
        .iter()
        .filter(|n| n.starts_with("link: "))
        .filter_map(|n| n.split(" time(s)").next()?.rsplit(' ').next()?.parse::<usize>().ok())
        .sum()
}

/// The lifts inside a level whose traverse (the first lateral rapid after
/// the lift) passes over `part`, sampled each millimetre: the from and to
/// points of each such traverse.
pub fn lifts_across(t: &Toolpath, part: &Region) -> Vec<(Point3, Point3)> {
    let n = t.moves.len();
    (0..n)
        .filter(|&i| {
            let m = &t.moves[i];
            let z = m.from.z;
            m.to.z > z + 1e-6 && i > 0 && floor_at(t, i - 1, z) && (i + 1..n).any(|k| floor_at(t, k, z))
        })
        .filter_map(|i| t.moves[i..].iter().find(|m| m.kind == MoveKind::Rapid && (m.to.x - m.from.x).hypot(m.to.y - m.from.y) > 1e-6))
        .filter(|m| {
            let k = ((m.to.x - m.from.x).hypot(m.to.y - m.from.y).ceil() as usize).clamp(1, 100_000);
            (0..=k).any(|j| {
                let s = j as f64 / k as f64;
                part.contains(Point2::new(m.from.x + s * (m.to.x - m.from.x), m.from.y + s * (m.to.y - m.from.y)))
            })
        })
        .map(|m| (m.from, m.to))
        .collect()
}

/// #3747 pin: the maintainer's fig8 (#3741), Inside and Outside. Each lift
/// inside a level has a named reason on the path (the link notes).
/// #3824: 6 lifts on Inside and 12 on Outside before; the part wall now
/// reads as one wall, so Inside keeps none and Outside keeps the 3 short
/// hops (one each level) whose in-plane link would cut uncut stock with
/// the full tool width.
#[test]
#[ignore = "heavy lane (#3747 fig8 Inside and Outside): two generations run past the 60 s debug budget - run with --release -- --ignored"]
fn fig8_start_sides_lift_only_with_a_named_reason() {
    for (start, max) in [(super::PassStart::Inside, 0), (super::PassStart::Outside, 3)] {
        let (world, mut h, name, stock) = super::fig8_world();
        let (path, _) = super::offset_op(&world, &mut h, &name, stock.0.z, start);
        let (up, named) = (level_lifts(&path), named_lifts(&path));
        eprintln!("fig8 {start:?}: {up} lift(s) inside a level, {named} named: {:?}", path.notes);
        assert_eq!(up, named, "{start:?}: a lift with no named reason: {:?}", path.notes);
        assert!(up <= max, "{start:?}: {up} lifts inside a level (#3824: at most {max}): {:?}", path.notes);
    }
}

/// #3825 pin: on fig8 Outside, no pass link lifts and traverses over the
/// part (four did, from (47.429, -39.000) to (46.482, 123.140) and to
/// (-46.500, 123.542)), and no pass start is entered by a plunge into
/// material.
#[test]
#[ignore = "heavy lane (#3825 fig8 Outside): one generation runs past the 60 s debug budget - run with --release -- --ignored"]
fn fig8_outside_pass_links_keep_off_the_part_and_enter_on_cut_floor() {
    let (world, mut h, name, stock) = super::fig8_world();
    let (path, _) = super::offset_op(&world, &mut h, &name, stock.0.z, super::PassStart::Outside);
    let e = super::model::find_by_name(&world, &name).expect("part");
    let facets = super::model::world_facets(&world, &e).expect("facets");
    let part = camcraft_app::geometry::slice::project(&facets, 64).expect("part outline");
    let across = lifts_across(&path, &part);
    assert_eq!(across, vec![], "#3825: a pass link lifts over the part: {:?}", path.notes);
    let plunged: Vec<&String> = path.notes.iter().filter(|n| n.contains("plunged into material")).collect();
    assert!(plunged.is_empty(), "#3825: a re-entry plunges into material: {plunged:?}");
}
