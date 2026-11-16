//! STL/OBJ shells have no solid/cavity membership. Never turn a possible
//! cavity into material: overlapping component bounds make that ambiguous.
use super::{ImportError, step_outward};
use crate::geometry::{Aabb, Mesh, Point3};
use crate::pick::winding::{BodyWinding, winding_of};

/// Normalize isolated inward solids, refusing possible nested shells.
/// Bounds are deliberately conservative: intersecting bounds need not mean
/// intersecting surfaces, but cannot establish independent material either.
pub(super) fn turn_outward(mesh: &mut Mesh) -> Result<Option<String>, ImportError> {
    let w = winding_of(&mesh.facets);
    if !w.bodies.contains(&BodyWinding::InsideOut) {
        return Ok(None);
    }
    let mut bounds: Vec<Option<Aabb>> = vec![None; w.bodies.len()];
    for (t, body) in mesh.facets.iter().zip(&w.facet_body) {
        let Some(body) = body else { continue };
        let slot = &mut bounds[*body as usize];
        for p in [t.a, t.b, t.c] {
            *slot = Some(match *slot {
                None => Aabb { min: p, max: p },
                Some(b) => Aabb {
                    min: Point3::new(b.min.x.min(p.x), b.min.y.min(p.y), b.min.z.min(p.z)),
                    max: Point3::new(b.max.x.max(p.x), b.max.y.max(p.y), b.max.z.max(p.z)),
                },
            });
        }
    }
    for (i, a) in bounds.iter().enumerate() {
        if w.bodies[i] != BodyWinding::InsideOut { continue; }
        let Some(a) = a else { continue };
        for (j, b) in bounds.iter().enumerate() {
            let Some(b) = b else { continue };
            if i != j
                && a.min.x <= b.max.x && b.min.x <= a.max.x
                && a.min.y <= b.max.y && b.min.y <= a.max.y
                && a.min.z <= b.max.z && b.min.z <= a.max.z
            {
                return Err(ImportError::AmbiguousShells);
            }
        }
    }
    Ok(step_outward::turn_outward(mesh))
}
