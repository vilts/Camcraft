//! The operation owns its break (#303): the program verdict's credit
//! for the edge breaks a chamfer or deburr was asked to cut.
//!
//! Provenance: the bevel is the chamfer mill's cone law
//! ([`crate::cl::EdgeBreak`], machine-tool handbook geometry); the
//! verdict is this codebase's own column deviation (`deviate`). Nothing
//! from vendor binaries or output.
//!
//! The law: the part the program must leave is the model WITH every
//! declared bevel cut. A column the verdict convicted (more than `tol`
//! below the model's top) that lies within a declared break's plan
//! width, under a model top no higher than that break's edge, is judged
//! against the bevel instead: cut down to the bevel (within `tol`) it
//! is the finished part; cut below it, it is a gouge charged at its
//! depth under the bevel. Rest is untouched - a declared break is
//! permission to remove, never a demand that the bevel be cut. Nothing
//! else changes: an edge no operation declared, or a model top standing
//! above the edge (a boss beside the rim), keeps the model's verdict.

use super::deviate::Deviation;
use super::dexel::Dexel;
use super::kernel::part_surface;

use crate::cl::EdgeBreak;
use crate::geometry::{Point3, Triangle};

/// Re-judge `dev`'s gouge columns against the declared `breaks`;
/// returns how many columns the bevels cleared or re-charged. `dex`
/// is the verdict's own lattice (the tops `dev` was read from) and
/// `facets` the part it judged; a lattice that does not match `dev`
/// credits nothing - the verdict then stands as it was, never looser.
pub fn credit_breaks(dev: &mut Deviation, dex: &Dexel, facets: &[Triangle], breaks: &[EdgeBreak]) -> usize {
    let breaks: Vec<&EdgeBreak> = breaks.iter().filter(|b| b.is_sane()).collect();
    if breaks.is_empty() {
        return 0;
    }
    credit_floors(dev, dex, facets, |ix, iy, p, tol| {
        let c = dex.center(ix, iy);
        breaks
            .iter()
            .filter_map(|b| b.floor_at(c.x, c.y))
            .filter(|&(_, edge_z)| p <= edge_z + tol)
            .map(|(f, _)| f)
            .fold(f64::INFINITY, f64::min)
    })
}

/// The one re-judging walk the declared credits share (the breaks here,
/// the marks of [`super::mark`]): every column the verdict convicted on
/// its own top is judged against `floor_of(ix, iy, part top, tol)` - the
/// finished surface some operation declared there, `+inf` for none -
/// instead of the model's top. Cut down to the floor (within `tol`) it
/// is the finished part; below it, a gouge charged at its depth under
/// the floor. A credit never deepens a verdict and never touches rest.
/// Returns how many columns changed; a lattice that does not match
/// `dev` credits nothing.
pub(super) fn credit_floors(
    dev: &mut Deviation,
    dex: &Dexel,
    facets: &[Triangle],
    floor_of: impl Fn(usize, usize, f64, f64) -> f64,
) -> usize {
    if dev.gouge_cells == 0 {
        return 0;
    }
    if dev.cells_x != dex.nx || dev.cells_y != dex.ny || dev.cell != dex.cell || dev.dev.len() != dex.nx * dex.ny {
        return 0;
    }
    let tol = dev.tol;
    let part = part_surface(facets, dex.x0, dex.y0, dex.cell, dex.nx, dex.ny);
    let a2 = dex.cell * dex.cell;
    let (mut changed, mut lost_max) = (0usize, false);
    for iy in 0..dex.ny {
        for ix in 0..dex.nx {
            let k = ix + iy * dex.nx;
            let v = dev.dev[k];
            if v.is_nan() || v >= -tol {
                continue;
            }
            let (sim, p) = (dex.top_at(k), part[k]);
            // Only the top verdict is a declared floor's to answer: a
            // column whose value came from elsewhere (a carried body's
            // floor) keeps it.
            if !(sim.is_finite() && p.is_finite()) || (v - (sim - p)).abs() > 1e-9 {
                continue;
            }
            let floor = floor_of(ix, iy, p, tol);
            if !floor.is_finite() {
                continue;
            }
            let nv = (sim - floor).min(0.0).max(v);
            if nv == v {
                continue;
            }
            changed += 1;
            lost_max |= -v >= dev.max_gouge;
            dev.dev[k] = nv;
            dev.gouge_cells = dev.gouge_cells.saturating_sub(1);
            dev.gouge_volume -= -v * a2;
            if nv < -tol {
                dev.gouge_cells += 1;
                dev.gouge_volume += -nv * a2;
            }
        }
    }
    if lost_max {
        rescan_worst(dev, dex);
    }
    dev.gouge_volume = dev.gouge_volume.max(0.0);
    changed
}

/// The worst gouge after a credit moved the old one: the deepest
/// convicted column, located at its own top.
fn rescan_worst(dev: &mut Deviation, dex: &Dexel) {
    dev.max_gouge = 0.0;
    dev.max_gouge_at = None;
    for iy in 0..dex.ny {
        for ix in 0..dex.nx {
            let k = ix + iy * dex.nx;
            let v = dev.dev[k];
            if v < -dev.tol && -v > dev.max_gouge {
                dev.max_gouge = -v;
                let c = dex.center(ix, iy);
                dev.max_gouge_at = Some(Point3::new(c.x, c.y, dex.top_at(k)));
            }
        }
    }
}
