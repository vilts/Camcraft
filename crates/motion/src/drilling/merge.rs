//! Provenance: original axial-span merging of fitted cylindrical walls, for #2601.

use super::cylinder::{cmp64, hypot};
use super::hole::{CrossHole, Hole};
use super::recognize::{Found, FIT_TOL, MERGE_DXY};
use crate::cl::lattice;
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};

/// Fold only touching spans, then read the floor to choose the mouth.
/// Reverse Z holes use the directed record; they must never become a
/// downward cycle that enters through solid material.
pub(super) fn collect(
    tris: &[Triangle],
    found: [Vec<Found>; 3],
    mesh_min_z: f64,
) -> (Vec<Hole>, Vec<CrossHole>) {
    let mut holes = Vec::new();
    let mut cross = Vec::new();
    let z = found[0]
        .iter()
        .map(|f| Hole {
            xy: Point2::new(lattice(f.cu), lattice(f.cv)),
            radius: lattice(f.rad),
            entry_z: lattice(f.smax),
            floor_z: lattice(f.smin),
            through: f.through,
            stepped: false,
        })
        .collect();
    for mut hole in merge_z(z, mesh_min_z) {
        let f = Found {
            cu: hole.xy.x,
            cv: hole.xy.y,
            rad: hole.radius,
            smin: hole.floor_z,
            smax: hole.entry_z,
            through: hole.through,
        };
        let (reverse, through) = super::mouth::ends(tris, (2, 0, 1), f);
        if reverse {
            cross.push(directed(f, (2, 0, 1), true, through));
        } else {
            hole.through = through;
            holes.push(hole);
        }
    }
    for (ci, frame) in [(1, (0, 1, 2)), (2, (1, 0, 2))] {
        for f in merge_found(found[ci].clone()) {
            let (reverse, through) = super::mouth::ends(tris, frame, f);
            cross.push(directed(f, frame, reverse, through));
        }
    }
    cross.sort_by(|a, b| {
        cmp64(a.origin.x + a.origin.y, b.origin.x + b.origin.y)
            .then(cmp64(a.origin.z, b.origin.z))
            .then(cmp64(a.origin.y, b.origin.y))
            .then(cmp64(a.origin.x, b.origin.x))
            .then(cmp64(a.radius, b.radius))
    });
    (holes, cross)
}

fn directed(
    f: Found,
    (along, u, v): (usize, usize, usize),
    reverse: bool,
    through: bool,
) -> CrossHole {
    let mut origin = Point3::new(0.0, 0.0, 0.0);
    super::recognize::set(&mut origin, u, lattice(f.cu));
    super::recognize::set(&mut origin, v, lattice(f.cv));
    super::recognize::set(
        &mut origin,
        along,
        lattice(if reverse { f.smin } else { f.smax }),
    );
    let mut dir = Point3::new(0.0, 0.0, 0.0);
    super::recognize::set(&mut dir, along, if reverse { 1.0 } else { -1.0 });
    CrossHole {
        origin,
        dir,
        radius: lattice(f.rad),
        entry_d: 0.0,
        floor_d: lattice(f.smin - f.smax),
        through,
    }
}

/// Merge Z-stepped same-axis components (counterbores, shallow
/// chamfers): one hole, narrowest radius, whole z span. The
/// pre-F2 fold, run on the snapped Holes exactly as it ran then.
fn merge_z(holes: Vec<Hole>, mesh_min_z: f64) -> Vec<Hole> {
    let mut merged: Vec<Hole> = Vec::new();
    let mut used = vec![false; holes.len()];
    for i in 0..holes.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut h = holes[i];
        loop {
            let mut grew = false;
            for j in 0..holes.len() {
                if !used[j]
                    && hypot(h.xy.x - holes[j].xy.x, h.xy.y - holes[j].xy.y) <= MERGE_DXY
                    && h.floor_z <= holes[j].entry_z + 1e-6
                    && holes[j].floor_z <= h.entry_z + 1e-6
                {
                    used[j] = true;
                    h = Hole {
                        xy: Point2::new(
                            lattice(h.xy.x.min(holes[j].xy.x)),
                            lattice(h.xy.y.min(holes[j].xy.y)),
                        ),
                        radius: h.radius.min(holes[j].radius),
                        entry_z: h.entry_z.max(holes[j].entry_z),
                        floor_z: h.floor_z.min(holes[j].floor_z),
                        through: false,
                        // The fold keeps the narrow wall and loses the
                        // wide one - if the radii differ by more than
                        // the circle-fit noise, say so (M8.3): below
                        // FIT_TOL the two walls are the same wall as
                        // far as we can tell, above it the ring is a
                        // real feature the record cannot show. The
                        // bore door in cycle.rs reads this flag.
                        stepped: h.stepped || (h.radius - holes[j].radius).abs() > FIT_TOL,
                    };
                    h.through = (h.floor_z - mesh_min_z).abs() <= 1e-6;
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        merged.push(h);
    }
    merged.sort_by(|a, b| {
        cmp64(a.xy.y, b.xy.y)
            .then(cmp64(a.xy.x, b.xy.x))
            .then(cmp64(a.radius, b.radius))
    });
    merged
}

/// The same fold for radial counterbores, at Found level so spans
/// stay absolute: same axis line within MERGE_DXY, mouth at the
/// highest end, floor at the lowest, narrowest radius - and the
/// through verdict follows the DEEPEST member (a through hole
/// counterbored short is still a through hole).
fn merge_found(found: Vec<Found>) -> Vec<Found> {
    let mut merged: Vec<Found> = Vec::new();
    let mut used = vec![false; found.len()];
    for i in 0..found.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut f = found[i];
        loop {
            let mut grew = false;
            for j in 0..found.len() {
                if !used[j]
                    && hypot(f.cu - found[j].cu, f.cv - found[j].cv) <= MERGE_DXY
                    && f.smin <= found[j].smax + 1e-6
                    && found[j].smin <= f.smax + 1e-6
                {
                    used[j] = true;
                    let deeper = found[j].smin < f.smin;
                    let through = if deeper { found[j].through } else { f.through };
                    f = Found {
                        cu: f.cu.min(found[j].cu),
                        cv: f.cv.min(found[j].cv),
                        rad: f.rad.min(found[j].rad),
                        smin: f.smin.min(found[j].smin),
                        smax: f.smax.max(found[j].smax),
                        through,
                    };
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        merged.push(f);
    }
    merged
}
