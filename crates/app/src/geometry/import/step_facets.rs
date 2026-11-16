//! One STEP tessellation pass's facets, face by face (#1573 review): the
//! kernel's per-face meshes, oriented as the face says, fanned into
//! triangles under the shell's facet budget, with the facet range each face
//! owns so the seams between faces can be judged and fixed per face.
//!
//! Provenance: polygon fan triangulation; the per-face orientation flag is
//! ISO 10303-42's `face_surface.same_sense` as truck (ricosjp, Apache-2.0)
//! carries it. Our own code; nothing from any CAM vendor.

use truck_meshalgo::prelude::*;

use super::step_refine::PassError;
use crate::geometry::{Point3, Triangle};

/// Every face's mesh as triangles, and `ends`: face `i` owns facets
/// `ends[i-1]..ends[i]` (a face with no mesh owns none). Zero-area slivers
/// are dropped. `Over` as soon as more than `budget` facets are made.
pub(super) fn facets_of(polys: &[Option<PolygonMesh>], budget: usize) -> Result<(Vec<Triangle>, Vec<usize>), PassError> {
    let mut facets: Vec<Triangle> = Vec::new();
    let mut ends = Vec::with_capacity(polys.len());
    for poly in polys {
        if let Some(poly) = poly {
            let positions = poly.positions();
            for face in poly.face_iter() {
                if face.len() < 3 {
                    continue;
                }
                let at = |i: usize| -> Option<Point3> {
                    let v = face.get(i)?.pos;
                    let p = positions.get(v)?;
                    let pt = Point3::new(p.x, p.y, p.z);
                    pt.is_finite().then_some(pt)
                };
                let Some(a) = at(0) else { continue };
                for i in 1..face.len() - 1 {
                    let (Some(b), Some(c)) = (at(i), at(i + 1)) else {
                        continue;
                    };
                    let tri = Triangle::new(a, b, c);
                    // Zero-area facets are tessellation slivers: drop them,
                    // they carry no geometry and would poison the STL-style
                    // gates.
                    let n = tri.normal();
                    if n.x != 0.0 || n.y != 0.0 || n.z != 0.0 {
                        facets.push(tri);
                        if facets.len() > budget {
                            return Err(PassError::Over);
                        }
                    }
                }
            }
        }
        ends.push(facets.len());
    }
    Ok((facets, ends))
}
