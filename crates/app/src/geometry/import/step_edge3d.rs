//! STEP edge geometry: an edge on a `SURFACE_CURVE` is read from that
//! curve's 3D curve, whatever representation the file names as the master
//! (#1548).
//!
//! Provenance: ISO 10303-42 `surface_curve` - `curve_3d` and the
//! `associated_geometry` pcurves describe one curve, and
//! `master_representation` only names which the writer prefers. Our own
//! design around truck's published STEP tables (ricosjp, Apache-2.0).
//! Nothing from any CAM vendor.
//!
//! Why: with `.PCURVE_S1.` / `.PCURVE_S2.` truck rebuilds the edge as a 2D
//! curve on the neighbouring surface, in truck's own parametrisation of
//! that surface and between parameters it searches for the two vertices
//! without a hint. On a cylinder (a periodic angle) or a patch with a pole
//! (a free parameter) those are not the file's parameters, and the edge
//! comes out somewhere else: on the #1536 model every planar face lost a
//! corner and the import was silently half its volume. The 3D curve is in
//! the model's own space and needs none of that.
//!
//! How: each `EDGE_CURVE` whose geometry is a surface curve is pointed at
//! that curve's `curve_3d` directly, so truck reads it as it reads a plain
//! curve edge. (Going through the surface curve with the master switched to
//! the 3D curve is not the same: truck then applies `same_sense = .F.`
//! twice and the edge runs backwards.) Surface curves used anywhere else
//! get their master switched to the 3D curve.

use std::collections::HashMap;

use truck_stepio::r#in::ruststep::{ast::Name, tables::PlaceHolder};
use truck_stepio::r#in::{CurveAnyHolder, PreferredSurfaceCurveRepresentation, Table};

/// Read every surface-curve edge from its 3D curve; returns how many edges
/// were pointed at one.
pub(super) fn prefer_3d(table: &mut Table) -> usize {
    let direct: HashMap<u64, PlaceHolder<CurveAnyHolder>> =
        table.surface_curve.iter().map(|(id, curve)| (*id, curve.curve_3d.clone())).collect();
    let mut edges = 0;
    for edge in table.edge_curve.values_mut() {
        let curve_3d = match &edge.edge_geometry {
            PlaceHolder::Ref(Name::Entity(id)) => direct.get(id).cloned(),
            PlaceHolder::Owned(CurveAnyHolder::SurfaceCurve(curve)) => Some(curve.curve_3d.clone()),
            _ => None,
        };
        if let Some(curve_3d) = curve_3d {
            edge.edge_geometry = curve_3d;
            edges += 1;
        }
    }
    for curve in table.surface_curve.values_mut() {
        curve.master_representation = PreferredSurfaceCurveRepresentation::Curve3D;
    }
    edges
}
