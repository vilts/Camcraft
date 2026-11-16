//! A closed imported solid wound inward is turned outward at import (#1617).
//! A file can wind every face of a solid inward, consistently, so no face
//! disagrees with a neighbour: the mesh then has a negative signed volume
//! and every facet normal points into the material. Only a body that is
//! closed, manifold and consistently wound (every shared edge used once in
//! each direction) with a clearly negative volume is turned; an open body
//! (a lone face, a shell with a gap), a non-manifold or mixed one, or one
//! with no volume is left as it is - its "outside" has no meaning.
//!
//! Provenance: the body verdict is `pick::winding` (signed volume by the
//! divergence theorem, Zhang & Chen ICIP 2001; the closed, consistently
//! oriented 2-manifold test, Botsch et al. "Polygon Mesh Processing" 2010
//! ch. 1). Reversing a triangle's vertex order reverses its normal by the
//! right-hand rule. Our own code, no vendor material.

use crate::geometry::{Mesh, Triangle};
use crate::pick::winding::{BodyWinding, winding_of};

/// Turn every closed, consistently wound, inward body of `mesh` outward.
/// Returns the import note that says so, `None` when nothing changed.
pub(super) fn turn_outward(mesh: &mut Mesh) -> Option<String> {
    let w = winding_of(&mesh.facets);
    let inward: Vec<bool> = w.bodies.iter().map(|b| *b == BodyWinding::InsideOut).collect();
    let bodies = inward.iter().filter(|&&b| b).count();
    if bodies == 0 {
        return None;
    }
    let mut turned = 0usize;
    let facets: Vec<Triangle> = mesh
        .facets
        .iter()
        .enumerate()
        .map(|(i, t)| match w.facet_body.get(i) {
            Some(Some(b)) if inward.get(*b as usize).copied().unwrap_or(false) => {
                turned = turned.saturating_add(1);
                Triangle::new(t.a, t.c, t.b)
            }
            _ => *t,
        })
        .collect();
    *mesh = Mesh::from_facets(facets);
    let note = format!(
        "{bodies} closed solid(s) wound inward (negative volume) were turned outward: {turned} facet(s) reversed"
    );
    tracing::warn!("import: {note}");
    Some(note)
}

#[cfg(test)]
#[path = "step_outward_tests.rs"]
mod tests;
