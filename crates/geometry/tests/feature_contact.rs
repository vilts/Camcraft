//! Provenance: hand-computable boxes and directed manifold edge incidence.
//! Gate map: covers camcraft_geometry::geometry::feature and shared_edge.

use camcraft_geometry::geometry::{Point3, Triangle, cube, feature};

fn contact() -> Vec<Triangle> {
    let mut facets = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 5.0))
        .facets
        .to_vec();
    facets.extend(
        cube(Point3::new(10.0, 0.0, 0.0), Point3::new(20.0, 10.0, 5.0))
            .facets
            .iter()
            .copied(),
    );
    facets
}

#[test]
fn a_whole_contact_face_keeps_each_solids_features() {
    let facets = contact();
    let f = feature::features_of_facets(&facets);
    assert_eq!(f.faces.len(), 12, "six faces per closed solid");
    assert!(
        f.edges
            .iter()
            .all(|e| e.kind != feature::EdgeKind::NonManifold)
    );
    assert!(f.edges.iter().all(|e| e.n_facets == 2));
    let mut ids: Vec<_> = f.faces.iter().map(|face| face.id).collect();
    ids.sort_unstable();
    let reversed: Vec<_> = facets.into_iter().rev().collect();
    let mut reordered: Vec<_> = feature::features_of_facets(&reversed)
        .faces
        .iter()
        .map(|face| face.id)
        .collect();
    reordered.sort_unstable();
    assert_eq!(
        ids, reordered,
        "contact resolution does not depend on facet order"
    );
}

#[test]
fn line_only_contact_remains_a_visible_non_manifold_edge() {
    let mut facets = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 5.0))
        .facets
        .to_vec();
    facets.extend(
        cube(Point3::new(10.0, 10.0, 0.0), Point3::new(20.0, 20.0, 5.0))
            .facets
            .iter()
            .copied(),
    );
    let f = feature::features_of_facets(&facets);
    assert_eq!(
        f.edges
            .iter()
            .filter(|e| e.kind == feature::EdgeKind::NonManifold)
            .count(),
        1
    );
}
