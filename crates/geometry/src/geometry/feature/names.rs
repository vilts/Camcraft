//! Deterministic user-friendly feature naming (#2403): maps every
//! pickable face, welded patch, edge chain/run/segment, and feature vertex
//! of one mesh to a stable, non-hex label (e.g. `"Face 1"`, `"Bore 1"`,
//! `"Cylinder 1"`, `"Curved face 1"`, `"Edge 1"`, `"Vertex 1"`).
//!
//! Provenance: our own deterministic spatial-order naming table over
//! `geometry::feature`; no vendor material consulted.

use std::collections::{HashMap, HashSet};

use super::patch_shape::{PatchShape, classify_patch};
use super::*;

/// Hash-domain discriminator matching `FeatureSel`'s four variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NameKind {
    Face,
    Patch,
    Chain,
    Vertex,
}

/// Pure inputs required to name every pickable item of one solid.
pub struct NameInputs<'a> {
    pub features: &'a Features,
    pub tris: &'a [Triangle],
    pub patches: &'a [Patch],
    pub chains: &'a [Chain],
    pub runs: &'a [Chain],
    /// Indices into `features.edges` that are real pickable feature edges.
    pub pick_edges: &'a [u32],
    /// Pickable feature vertices `(vert_idx, VertexId)`.
    pub pick_verts: &'a [(u32, VertexId)],
}

/// Derived per-mesh lookup table mapping `(NameKind, u64)` -> user-friendly
/// name without hexadecimal digests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FeatureNames {
    entries: HashMap<(NameKind, u64), String>,
}

impl FeatureNames {
    #[must_use]
    pub fn get(&self, kind: NameKind, id: u64) -> Option<&str> {
        self.entries.get(&(kind, id)).map(String::as_str)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Plain fallback noun when an entity or id no longer resolves.
    #[must_use]
    pub const fn fallback(kind: NameKind) -> &'static str {
        match kind {
            NameKind::Face => "Face",
            NameKind::Patch => "Curved face",
            NameKind::Chain => "Edge",
            NameKind::Vertex => "Vertex",
        }
    }

    #[must_use]
    pub fn label_or_fallback(&self, kind: NameKind, id: u64) -> &str {
        self.get(kind, id).unwrap_or_else(|| Self::fallback(kind))
    }
}

/// Build the deterministic friendly name table for one mesh.
#[must_use]
pub fn build_feature_names(inputs: &NameInputs<'_>) -> FeatureNames {
    let f = inputs.features;
    let mut entries = HashMap::new();
    let patch_faces: HashSet<FaceId> = inputs.patches.iter().flat_map(|p| p.faces.iter().copied()).collect();

    // Standalone planar faces get Face 1..M before strip faces of smooth patches.
    let mut faces: Vec<(u8, (i64, i64, i64), FaceId)> = f
        .faces
        .iter()
        .filter(|fa| matches!(fa.kind, FaceKind::Planar))
        .map(|fa| {
            let group = u8::from(patch_faces.contains(&fa.id));
            (group, face_center_key(f, inputs.tris, fa), fa.id)
        })
        .collect();
    faces.sort_unstable();
    for (idx, (_, _, fid)) in faces.into_iter().enumerate() {
        entries.entry((NameKind::Face, fid)).or_insert_with(|| format!("Face {}", idx + 1));
    }

    // Welded smooth patches: classify bore vs cylinder vs curved face, numbered per kind.
    let face_by_id: HashMap<FaceId, &Face> = f.faces.iter().map(|fa| (fa.id, fa)).collect();
    let mut patch_items: Vec<((i64, i64, i64), u64, PatchShape)> = inputs
        .patches
        .iter()
        .map(|p| {
            let tris = patch_triangles(&face_by_id, inputs.tris, p);
            let shape = classify_patch(&tris);
            let center = avg_center_key(tris.iter().flat_map(|t| [t.a, t.b, t.c].map(|q| [q.x, q.y, q.z])));
            (center, p.id, shape)
        })
        .collect();
    patch_items.sort_unstable_by_key(|(c, pid, _)| (*c, *pid));
    let (mut bore_n, mut cyl_n, mut curved_n) = (0usize, 0usize, 0usize);
    for (_, pid, shape) in patch_items {
        let name = match shape {
            PatchShape::Bore => {
                bore_n += 1;
                format!("Bore {bore_n}")
            }
            PatchShape::Cylinder => {
                cyl_n += 1;
                format!("Cylinder {cyl_n}")
            }
            PatchShape::CurvedFace => {
                curved_n += 1;
                format!("Curved face {curved_n}")
            }
        };
        entries.insert((NameKind::Patch, pid), name);
    }

    // Pickable edges: full tangent chains, sub-runs, and remaining single feature segments.
    let pick_edge_set: HashSet<u32> = inputs.pick_edges.iter().copied().collect();
    let mut edge_n = 0usize;
    let assign_chains = |list: &[Chain], entries: &mut HashMap<(NameKind, u64), String>, edge_n: &mut usize| {
        let mut items: Vec<((i64, i64, i64), u64)> = list
            .iter()
            .filter(|c| !c.edges.is_empty() && c.edges.iter().all(|e| pick_edge_set.contains(e)))
            .map(|c| (chain_center_key(f, &c.edges), c.id))
            .collect();
        items.sort_unstable();
        for (_, cid) in items {
            entries.entry((NameKind::Chain, cid)).or_insert_with(|| {
                *edge_n += 1;
                format!("Edge {}", *edge_n)
            });
        }
    };
    assign_chains(inputs.chains, &mut entries, &mut edge_n);
    assign_chains(inputs.runs, &mut entries, &mut edge_n);

    let sentinel = (i64::MIN, i64::MIN, i64::MIN);
    let lattice: Vec<Pos> = f.verts.iter().map(|p| weld_pos(p).unwrap_or(sentinel)).collect();
    let mut segs: Vec<((i64, i64, i64), u32, u64)> = inputs
        .pick_edges
        .iter()
        .filter_map(|&ei| {
            let cid = singleton_chain_id_with_lattice(f, &lattice, ei)?;
            (!entries.contains_key(&(NameKind::Chain, cid))).then(|| (chain_center_key(f, &[ei]), ei, cid))
        })
        .collect();
    segs.sort_unstable();
    for (_, _, cid) in segs {
        entries.entry((NameKind::Chain, cid)).or_insert_with(|| {
            edge_n += 1;
            format!("Edge {edge_n}")
        });
    }

    // Pickable vertices: sorted deterministically in local space.
    let mut verts: Vec<((i64, i64, i64), u64)> = inputs
        .pick_verts
        .iter()
        .filter_map(|&(vi, vid)| f.verts.get(vi as usize).map(|p| (quant(*p), vid)))
        .collect();
    verts.sort_unstable();
    verts.dedup_by_key(|(_, vid)| *vid);
    for (idx, (_, vid)) in verts.into_iter().enumerate() {
        entries.entry((NameKind::Vertex, vid)).or_insert_with(|| format!("Vertex {}", idx + 1));
    }

    FeatureNames { entries }
}

fn patch_triangles<'a>(face_by_id: &HashMap<FaceId, &Face>, tris: &'a [Triangle], p: &Patch) -> Vec<&'a Triangle> {
    let mut out = Vec::new();
    for fid in &p.faces {
        if let Some(fa) = face_by_id.get(fid) {
            for &ti in &fa.facets {
                if let Some(tri) = tris.get(ti as usize) {
                    out.push(tri);
                }
            }
        }
    }
    out
}

fn face_center_key(f: &Features, tris: &[Triangle], fa: &Face) -> (i64, i64, i64) {
    if !fa.facets.is_empty() {
        let key = avg_center_key(
            fa.facets
                .iter()
                .filter_map(|&ti| tris.get(ti as usize))
                .flat_map(|t| [t.a, t.b, t.c].map(|q| [q.x, q.y, q.z])),
        );
        if key != (i64::MAX, i64::MAX, i64::MAX) {
            return key;
        }
    }
    avg_center_key(
        fa.loops
            .iter()
            .flat_map(|lp| lp.verts.iter())
            .filter_map(|&vi| f.verts.get(vi as usize).copied()),
    )
}

fn chain_center_key(f: &Features, edges: &[u32]) -> (i64, i64, i64) {
    avg_center_key(
        edges
            .iter()
            .filter_map(|&ei| f.edges.get(ei as usize))
            .flat_map(|e| [e.a, e.b])
            .filter_map(|vi| f.verts.get(vi as usize).copied()),
    )
}

fn avg_center_key(pts: impl Iterator<Item = [f64; 3]>) -> (i64, i64, i64) {
    let (mut sum, mut count) = ([0.0f64; 3], 0usize);
    for p in pts {
        if p.iter().all(|c| c.is_finite()) {
            sum[0] += p[0];
            sum[1] += p[1];
            sum[2] += p[2];
            count += 1;
        }
    }
    if count == 0 {
        return (i64::MAX, i64::MAX, i64::MAX);
    }
    let inv = 1.0 / count as f64;
    quant([sum[0] * inv, sum[1] * inv, sum[2] * inv])
}

fn quant(p: [f64; 3]) -> (i64, i64, i64) {
    // Round in micrometres ordered (z, y, x) so sorting is insensitive to sub-nm noise.
    let um = |v: f64| if v.is_finite() { (v * 1e3).round().clamp(-1e15, 1e15) as i64 } else { i64::MAX };
    (um(p[2]), um(p[1]), um(p[0]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point3, cube, cylinder};

    fn test_names(mesh: &crate::geometry::Mesh) -> FeatureNames {
        let f = features_of_facets(&mesh.facets);
        let patches = patches(&f);
        let chains = chains(&f);
        let runs = edge_runs(&f);
        let pick_edges: Vec<u32> = (0..f.edges.len() as u32).collect();
        let pick_verts: Vec<(u32, VertexId)> =
            corner_verts(&f).into_iter().filter_map(|vi| Some((vi, vertex_id(&f, vi)?))).collect();
        build_feature_names(&NameInputs {
            features: &f,
            tris: &mesh.facets,
            patches: &patches,
            chains: &chains,
            runs: &runs,
            pick_edges: &pick_edges,
            pick_verts: &pick_verts,
        })
    }

    #[test]
    fn cube_features_get_stable_friendly_names_across_facet_reversal() {
        let m = cube(Point3::ZERO, Point3::new(10.0, 20.0, 5.0));
        let mut rev_facets = m.facets.to_vec();
        rev_facets.reverse();
        let rev = crate::geometry::Mesh::from_facets(rev_facets);
        let n1 = test_names(&m);
        let n2 = test_names(&rev);
        assert_eq!(n1, n2, "names stay identical regardless of triangle soup order");
        let f = features_of_facets(&m.facets);
        for fa in &f.faces {
            let name = n1.get(NameKind::Face, fa.id).expect("face name");
            assert!(name.starts_with("Face ") && !name.contains("0x"), "{name}");
        }
        for c in chains(&f) {
            let name = n1.get(NameKind::Chain, c.id).expect("edge name");
            assert!(name.starts_with("Edge ") && !name.contains("0x"), "{name}");
        }
        for vi in corner_verts(&f) {
            let vid = vertex_id(&f, vi).expect("vid");
            let name = n1.get(NameKind::Vertex, vid).expect("vertex name");
            assert!(name.starts_with("Vertex ") && !name.contains("0x"), "{name}");
        }
    }

    #[test]
    fn cylinder_and_inward_bore_are_named_by_kind() {
        let m = cylinder(20.0, 10.0, 16);
        let f = features_of_facets(&m.facets);
        let ps = patches(&f);
        let names = test_names(&m);
        assert_eq!(ps.len(), 1);
        assert_eq!(names.get(NameKind::Patch, ps[0].id), Some("Cylinder 1"));

        // Reversed winding turns an outward cylinder patch into an inward bore wall.
        let mut bore_facets = m.facets.to_vec();
        for t in &mut bore_facets {
            std::mem::swap(&mut t.b, &mut t.c);
        }
        let bore_mesh = crate::geometry::Mesh::from_facets(bore_facets);
        let bf = features_of_facets(&bore_mesh.facets);
        let bps = patches(&bf);
        let bnames = test_names(&bore_mesh);
        assert_eq!(bps.len(), 1);
        assert_eq!(bnames.get(NameKind::Patch, bps[0].id), Some("Bore 1"));
    }
}
