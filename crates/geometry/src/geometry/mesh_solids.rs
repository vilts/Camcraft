//! The closed solids of one imported mesh (#917): a STEP file with several
//! solids keeps one facet range per solid, so the slicer can read two
//! solids that share a wall line as the union they stand for instead of
//! one non-manifold shell.
//! Provenance: our own design; the section of a union of solids is the
//! union of their sections (Requicha, "Representations for Rigid Solids",
//! ACM Computing Surveys 12(4), 1980). No HSMWorks material consulted.

use std::ops::Range;

use super::{Mesh, Triangle};
use crate::geo2d::GeoError;

impl Mesh {
    /// `facets` with one range per closed solid. The ranges must be in
    /// order, not empty, and cover `facets` exactly, end to end; anything
    /// else is an input error, never a silent partial body. Fewer than two
    /// ranges store none: a single solid is the plain soup.
    pub fn from_solids(facets: Vec<Triangle>, solids: Vec<Range<usize>>) -> Result<Self, GeoError> {
        let mut at = 0_usize;
        for r in &solids {
            if r.start != at || r.end <= r.start {
                return Err(GeoError::Invalid(format!(
                    "solid range {}..{} does not continue the ranges at facet {at}",
                    r.start, r.end
                )));
            }
            at = r.end;
        }
        if at != facets.len() {
            return Err(GeoError::Invalid(format!(
                "the solid ranges end at facet {at}, the mesh has {} facets",
                facets.len()
            )));
        }
        let mut mesh = Mesh::from_facets(facets);
        if solids.len() > 1 {
            mesh.solids = solids.into();
        }
        Ok(mesh)
    }

    /// The facet range of each closed solid; empty when the mesh is one
    /// soup or the solids are not known.
    pub fn solid_ranges(&self) -> &[Range<usize>] {
        &self.solids
    }
}
