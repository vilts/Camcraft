//! Deviation classes on the machined mesh (M13.1 owed, #25): each
//! facet of the extracted stock judged against the model by the eye's
//! own law - the target's signed distance at the facet's centroid,
//! gated by `preview::gate_tol(tol, cell)` at the view's verdict
//! tolerance (`DEV_TOL`, tightened on a reduced-fidelity model, #1056).
//! Provenance: the classes are `color_classes`' vocabulary (0 clean, 1 rest, 2 gouge,
//! 3 unjudged) read through the GPU eye's `DistanceLattice` (M7.2);
//! our own code, nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **One judgement per mesh, never per frame.** The lattice is packed
//!   and every facet read once when the view's mesh is classified
//!   (`stock_export::MeshCache::classify`); the viewer only reads the
//!   cached classes back.
//! - **The eye's thresholds, not new ones.** The same tolerance, the
//!   same landing slack, the same lattice the GPU pass binds: the mesh
//!   and the traced picture cannot disagree by a threshold.
//! - **No verdict, no paint.** No target, a lattice that will not pack,
//!   a non-finite centroid or reading: class 3, never clean.

use crate::geometry::{Point3, Triangle};
use crate::stock_sim::preview::{gate_tol, pack_target, DistanceLattice};
use crate::stock_view::FieldView;

/// Clean: within the gate of the model.
pub const CLEAN: f32 = 0.0;
/// Rest: material stands outside the model.
pub const REST: f32 = 1.0;
/// Gouge: the surface sits under the model's.
pub const GOUGE: f32 = 2.0;
/// Unjudged: no verdict could be made.
pub const UNJUDGED: f32 = 3.0;

/// One signed deviation `v` (positive outside the model) under the
/// gate `tol`: the classes `color_classes` and the eye share. A NaN or
/// infinite reading, or a tolerance that is not a finite number, is
/// unjudged - a verdict the numbers cannot make is never clean.
#[must_use]
pub fn class_of(v: f64, tol: f64) -> f32 {
    if !(v.is_finite() && tol.is_finite()) {
        return UNJUDGED;
    }
    let tol = tol.max(0.0);
    if v > tol {
        REST
    } else if v < -tol {
        GOUGE
    } else {
        CLEAN
    }
}

/// One class per facet of `tris`, read at each centroid on `target`
/// (absent: every facet unjudged).
#[must_use]
pub fn facet_classes(tris: &[Triangle], target: Option<&DistanceLattice>, tol: f64) -> Vec<f32> {
    let Some(t) = target else {
        return vec![UNJUDGED; tris.len()];
    };
    tris.iter()
        .map(|f| {
            let c = Point3::new(
                (f.a.x + f.b.x + f.c.x) / 3.0,
                (f.a.y + f.b.y + f.c.y) / 3.0,
                (f.a.z + f.b.z + f.c.z) / 3.0,
            );
            class_of(t.signed_at(c), tol)
        })
        .collect()
}

/// The classes of `tris`, the mesh extracted from `fv`: the view's own
/// target packed as the eye packs it, gated at the view's cell.
#[must_use]
pub fn mesh_classes(fv: &FieldView, tris: &[Triangle]) -> Vec<f32> {
    let lattice = fv.target.as_ref().and_then(|t| t.field()).and_then(pack_target);
    facet_classes(tris, lattice.as_ref(), gate_tol(fv.tol, fv.cell))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stock_sim::DEV_TOL;
    use crate::geometry::cube;
    use crate::stock_sim::{SdfField, FIELD_BAND};

    #[test]
    fn the_boundaries_belong_to_clean() {
        assert_eq!(class_of(0.0, 0.01), CLEAN);
        assert_eq!(class_of(0.01, 0.01), CLEAN, "exactly the gate is clean");
        assert_eq!(class_of(-0.01, 0.01), CLEAN, "exactly minus the gate is clean");
        assert_eq!(class_of(0.010_001, 0.01), REST);
        assert_eq!(class_of(-0.010_001, 0.01), GOUGE);
        assert_eq!(class_of(5.0, 0.01), REST);
        assert_eq!(class_of(-5.0, 0.01), GOUGE);
    }

    #[test]
    fn no_number_no_verdict() {
        assert_eq!(class_of(f64::NAN, 0.01), UNJUDGED);
        assert_eq!(class_of(f64::INFINITY, 0.01), UNJUDGED);
        assert_eq!(class_of(f64::NEG_INFINITY, 0.01), UNJUDGED);
        assert_eq!(class_of(0.0, f64::NAN), UNJUDGED, "a nonsense gate judges nothing");
        // A negative gate reads as zero: only an exact zero is clean.
        assert_eq!(class_of(0.0, -1.0), CLEAN);
        assert_eq!(class_of(1e-9, -1.0), REST);
    }

    #[test]
    fn no_target_paints_every_facet_unjudged() {
        let tris = cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0)).facets;
        let c = facet_classes(&tris, None, 0.01);
        assert_eq!(c.len(), tris.len());
        assert!(c.iter().all(|&k| k == UNJUDGED));
        assert!(facet_classes(&[], None, 0.01).is_empty());
    }

    #[test]
    fn facets_on_above_and_under_the_model_class_as_such() {
        let model = cube(Point3::ZERO, Point3::new(10.0, 10.0, 10.0)).facets;
        let f = SdfField::from_mesh(&model, 0.5, FIELD_BAND).expect("the box signs");
        let lat = pack_target(&f).expect("the box packs");
        let tri = |z: f64| Triangle::new(Point3::new(4.0, 4.0, z), Point3::new(6.0, 4.0, z), Point3::new(5.0, 6.0, z));
        let degenerate = Triangle::new(Point3::new(f64::NAN, 0.0, 0.0), Point3::ZERO, Point3::ZERO);
        let tol = gate_tol(DEV_TOL, 0.5);
        let c = facet_classes(&[tri(10.0), tri(11.0), tri(9.0), degenerate], Some(&lat), tol);
        assert_eq!(c, vec![CLEAN, REST, GOUGE, UNJUDGED]);
    }
}
