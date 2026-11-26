//! Kernel pins for the whole-model 2D Contour note of a ring that has no
//! floor level (#2538): a band with two outer rings, where only one ring has
//! a floor level above the next flat level, must name the uncut Z range of
//! the other ring. Pinned for the first band and for an in-between band.
//! Split from `contour_whole.rs` for the 400-line law.
//!
//! Gate map: covers camcraft_motion::machining2d (contour_whole).
//!
//! Provenance: our own extruded and tapered manifold test meshes; no vendor
//! material.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geometry::{Point3, Triangle, slice};
use camcraft_motion::machining2d::{ContourParams, Lead, Side, contour_whole_bodies};
use camcraft_motion::machining3d::Entry3D;

type Ring = [(f64, f64); 4];

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Ring {
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

fn tri(f: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, up: bool) {
    f.push(if up { Triangle::new(a, b, c) } else { Triangle::new(a, c, b) });
}

/// A flat cap at `z`: the whole of `outer`, or the ring between `outer` and `inner`.
fn cap(f: &mut Vec<Triangle>, outer: &Ring, inner: Option<&Ring>, z: f64, up: bool) {
    let p = |q: (f64, f64)| Point3::new(q.0, q.1, z);
    match inner {
        None => {
            tri(f, p(outer[0]), p(outer[1]), p(outer[2]), up);
            tri(f, p(outer[0]), p(outer[2]), p(outer[3]), up);
        }
        Some(inn) => {
            for k in 0..4 {
                let n = (k + 1) % 4;
                tri(f, p(outer[k]), p(outer[n]), p(inn[n]), up);
                tri(f, p(outer[k]), p(inn[n]), p(inn[k]), up);
            }
        }
    }
}

/// Outward walls from the loop `lo` at `z0` up to the loop `hi` at `z1`
/// (`lo == hi`: a straight wall; a wider `lo`: a taper).
fn walls(f: &mut Vec<Triangle>, lo: &Ring, z0: f64, hi: &Ring, z1: f64) {
    for k in 0..4 {
        let n = (k + 1) % 4;
        let (a, b) = (Point3::new(lo[k].0, lo[k].1, z0), Point3::new(lo[n].0, lo[n].1, z0));
        let (c, d) = (Point3::new(hi[n].0, hi[n].1, z1), Point3::new(hi[k].0, hi[k].1, z1));
        tri(f, a, b, c, true);
        tri(f, a, c, d, true);
    }
}

/// A plate z `[0..10]` that carries one step z `[10..top]`. The step has the
/// foot `foot` and the top outline `head` (a narrower `head` is a taper).
fn plate_with_step(f: &mut Vec<Triangle>, plate: &Ring, foot: &Ring, head: &Ring, top: f64) {
    cap(f, plate, None, 0.0, false);
    walls(f, plate, 0.0, plate, 10.0);
    cap(f, plate, Some(foot), 10.0, true);
    walls(f, foot, 10.0, head, top);
}

fn params(top: f64, stepdown: f64) -> ContourParams {
    ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: top + 10.0,
        retract_z: top + 5.0,
        top_z: top,
        bottom_z: 0.0,
        stepdown,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        picks: Default::default(),
    }
}

/// The straight step (x < 0) has its floor pass at `z`.
fn has_lap_left_at(path: &Toolpath, z: f64) -> bool {
    path.moves.iter().any(|m| {
        m.kind == MoveKind::Feed && (m.from.z - z).abs() < 1e-6 && (m.to.z - z).abs() < 1e-6 && m.to.x < -1.0
    })
}

/// Exactly one note says that a ring has no floor level, and it names the
/// uncut Z range `lo..hi` of that ring.
fn assert_names_uncut_range(path: &Toolpath, lo: &str, hi: &str) {
    let said: Vec<&String> = path.notes.iter().filter(|n| n.contains("has no floor level")).collect();
    assert_eq!(said.len(), 1, "one ring has no floor level: {:?}", path.notes);
    let n = said[0];
    assert!(n.contains(&format!("Z {lo}")) && n.contains(&format!("Z {hi}")), "uncut Z range: {n}");
    assert!(n.contains("not cut"), "{n}");
}

#[test]
fn first_band_names_the_uncut_range_of_the_ring_without_a_floor_level() {
    // Left: a straight island z [10..15] on its plate (floor level Z 10).
    // Right: a tapered island z [10..15] on its plate. The ring of its top
    // outline is not legal at any vertex plane below Top Z: no floor level.
    let mut f = Vec::new();
    let island = rect(-30.0, -10.0, -10.0, 10.0);
    plate_with_step(&mut f, &rect(-40.0, -20.0, 0.0, 20.0), &island, &island, 15.0);
    cap(&mut f, &island, None, 15.0, true);
    let head = rect(35.0, -5.0, 45.0, 5.0);
    plate_with_step(&mut f, &rect(20.0, -20.0, 60.0, 20.0), &rect(30.0, -10.0, 50.0, 10.0), &head, 15.0);
    cap(&mut f, &head, None, 15.0, true);

    let top_reg = slice::slice(&f, 15.0 - 1e-3).expect("top slice");
    assert_eq!(top_reg.loops().len(), 2, "two outer rings in the first band");
    // Flat levels are Z 3 and Z 0: the two rings stop at the first level.
    let path = contour_whole_bodies(&top_reg, &f, &[], &params(15.0, 12.0)).expect("whole contour");
    assert!(path.all_finite() && path.is_chained());
    assert!(has_lap_left_at(&path, 10.0), "the straight island keeps its floor pass at Z 10");
    assert_names_uncut_range(&path, "3.000", "15.000");
}

#[test]
fn in_between_band_names_the_uncut_range_of_the_ring_without_a_floor_level() {
    // Left: plate, straight middle step z [10..20], boss z [20..30].
    // Right: plate and a tapered middle step z [10..20] (no floor level).
    // The band under the boss floor (Z 20) has the two middle steps as its
    // two outer rings and holds no flat level.
    let mut f = Vec::new();
    let mid = rect(-35.0, -15.0, -5.0, 15.0);
    let boss = rect(-25.0, -5.0, -15.0, 5.0);
    plate_with_step(&mut f, &rect(-40.0, -20.0, 0.0, 20.0), &mid, &mid, 20.0);
    cap(&mut f, &mid, Some(&boss), 20.0, true);
    walls(&mut f, &boss, 20.0, &boss, 30.0);
    cap(&mut f, &boss, None, 30.0, true);
    let head = rect(35.0, -5.0, 45.0, 5.0);
    plate_with_step(&mut f, &rect(20.0, -20.0, 60.0, 20.0), &rect(30.0, -10.0, 50.0, 10.0), &head, 20.0);
    cap(&mut f, &head, None, 20.0, true);

    let top_reg = slice::slice(&f, 30.0 - 1e-3).expect("top slice");
    // Flat levels are Z 5 and Z 0. The boss ends at its floor, Z 20.
    let path = contour_whole_bodies(&top_reg, &f, &[], &params(30.0, 25.0)).expect("whole contour");
    assert!(path.all_finite() && path.is_chained());
    assert!(has_lap_left_at(&path, 20.0), "the boss keeps its floor pass at Z 20");
    assert!(has_lap_left_at(&path, 10.0), "the straight middle step keeps its floor pass at Z 10");
    assert_names_uncut_range(&path, "5.000", "20.000");
}
