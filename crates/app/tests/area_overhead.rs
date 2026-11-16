//! The area controls' blocked region (M11.5) under an overhang: the part
//! material an extension must never flood, a Face must never sweep and
//! the pocket family re-subtracts is everything standing above the cut
//! plane, section AND shadow (`slice::material_overhead`). A ledge hung
//! over the faced plane but not cut by it is part material standing above
//! Cut Z like any wall: the extended face's band crosses it, and that is
//! the named refusal. The plain section saw nothing there and flooded the
//! facing under the ledge, shank through it.
//!
//! Provenance: our own fixture (the shadowed-islands follow-up), specced
//! from the Prime Directive; no vendor material.
//!
//! Gate map (M11.5 shadow): covers app::operation (boundary_ctx, boundary,
//! generate), geometry::slice (material_overhead), sim::verify.

mod area_support;
mod common;

use area_support::*;
use camcraft_app::geometry::Mesh;

/// A block X [-20, 0] Z [0, 10] (its top is faced) and, with `ledge`, a
/// slab X [5, 20] Z [15, 16] hanging over the faced plane's extension.
fn ledge_world(ledge: bool) -> World {
    let mut boxes = vec![((-20.0, 0.0), (0.0, 10.0))];
    if ledge {
        boxes.push(((5.0, 15.0), (20.0, 16.0)));
    }
    let mut f: Vec<camcraft_app::geometry::Triangle> = Vec::new();
    for (lo, hi) in boxes {
        f.extend(cube(Point3::new(lo.0, -20.0, lo.1), Point3::new(hi.0, 20.0, hi.1)).facets.iter().cloned());
    }
    world_with(Some(ImportModel::from_mesh("part", Mesh::from_facets(f))), 0.0, 20.0)
}

fn extended_face(world: &World) -> Result<(Toolpath, usize), String> {
    let src = SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: top_face(world, "part", 10.0) }]);
    run(world, &op(src, face(10.0), AreaControls { extension: 10.0, ..Default::default() }))
}

#[test]
fn an_extended_face_under_a_hanging_ledge_is_refused_by_name() {
    // In the open, the extension clears to the stock and past it, clean.
    let (t, warns) = extended_face(&ledge_world(false)).expect("open face");
    assert_eq!(warns, 0, "{:?}", t.notes);
    assert!(max_by(&feeds(&t), |p| p.x) > 20.0, "the rows run out past the stock");
    // Under the ledge: the plain section at Z 10 is empty, so the rows
    // flooded under it; the ledge's shadow is part material standing
    // above Cut Z, and the band that crosses it is refused by name.
    let err = extended_face(&ledge_world(true)).map(|(t, w)| (t.moves.len(), w)).expect_err("refused");
    assert!(err.contains("part material standing above Cut Z"), "{err}");
}
