//! Flat-area finishing (M2.5 A): the floors, and only the floors.
//!
//! Provenance: floor-only finishing is textbook CAM - the planar
//! shading of a Z-level model, ridden concentrically at the scallop
//! stepover (the same public Huang-Oliver formula the module laws
//! cite). The construction below is our own: flatness measured as
//! tangent-sheet rise across the facet's own box, the floor set a
//! rewritten `shallow` flag on the support scan, and one shared
//! `shadow_pass` so the choreography can never drift from the
//! contour family it borrows.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{GenError, PassStart};

use super::params::{Finish3DParams, validate};
use super::shadow::{shadow_pass, shadow_region};
use super::grid::SupSet;
use super::support::{Sup, build_sups};

/// Is this facet a FLOOR? Two tests, both scale-honest:
/// - it faces the sky at all - the family's own jurisdiction line
///   (`shallow`: `m_z >= SPLIT_MZ`); a near-vertical wall cannot be
///   a floor no matter how finely it is tessellated;
/// - its tangent-sheet rise across its own XY box is under the
///   tolerance: a facet whose whole height variation over the
///   centers that touch it fits inside the surface deviation the
///   op already spends is INDISTINGUISHABLE from a level plane at
///   the finish. `m_xy · diag <= tol` - so a coarse slope is a
///   slope (the raster's, the rings'), and only ground that truly
///   lies flat claims the floor name.
fn is_floor(s: &Sup, tol: f64) -> bool {
    if !s.shallow {
        return false;
    }
    let (mut x0, mut x1, mut y0, mut y1) =
        (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for v in s.tri {
        x0 = x0.min(v.x);
        x1 = x1.max(v.x);
        y0 = y0.min(v.y);
        y1 = y1.max(v.y);
    }
    let rise = s.m.x.hypot(s.m.y) * (x1 - x0).hypot(y1 - y0);
    rise.is_finite() && rise <= tol
}

/// Flat-area finishing: the model's floors, shrunk one stepover per
/// pass at the lifted Z the tangent answers - the shadow family's
/// choreography on the ground the waterline's rest verdict always
/// refused to claim (a horizontal facet enters no forbidden region:
/// its interior is reachable by no ring). The flanks and slopes ride
/// NOTHING here - the ring ladder stays home, which is the whole
/// point of naming the floors their own operation; a model with no
/// floor inside the programmed depths refuses by name. Heights obey
/// the same floor law as the rest of the shadow family
/// (`support_at`'s `bottom_z` outrank); `steep_stepover` is inert
/// here, as if it had never arrived.
pub fn flat(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    flat_from(facets, p, PassStart::Auto)
}

/// [`flat`] with the start side of its offset passes (#3362): `Inside`
/// cuts the innermost floor pass first. `Auto` is [`flat`] byte for
/// byte.
pub fn flat_from(facets: &[Triangle], p: &Finish3DParams, start: PassStart) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    // The floors as a support set: geometry unchanged (clearance
    // still answers for EVERY facet), jurisdiction rewritten.
    let sups = build_sups(facets, r);
    let fsups = SupSet::new(
        sups.iter()
            .map(|s| Sup { shallow: is_floor(s, p.tolerance), folds: [None; 3], ..*s })
            .collect(),
    );
    let region = shadow_region(&fsups, r)?;
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    // max_rings None: every pass until the floor collapses at its
    // medial axis - the contour family's law, verbatim.
    if !shadow_pass(&mut b, p, r, &fsups, &region, None, "flat", start)? {
        return Err(GenError::Empty(
            "flat-area finishing found no floor flatter than one tolerance inside the programmed depths - slopes and flanks belong to the other strategies".into(),
        ));
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("flat path left the finite horizon".into()));
    }
    Ok(path)
}
