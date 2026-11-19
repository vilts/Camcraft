//! #366: face-normal alignment picks trust a face's facet winding only on
//! a body that is closed and wound outward (positive signed volume). An
//! inside-out cube refuses the plane lay and the 3-2-1 by name (#554:
//! two edges no longer lay a plane - refused as edges, whatever the
//! winding); the same cube wound outward still works; an open box, a body
//! wound both ways and a non-manifold body refuse by name. The shipped
//! test models (STL, OBJ, STEP) read as outward; the inside-out STL
//! fixture is turned outward at import and says so (#1672, #1752).
//! Provenance: our own fixtures over the divergence-theorem volume sign.
//!
//! Gate map (#366): covers app::pick::winding, app::pick::align
//! (outward_normal), app::pick::plane (face datum, edge refusal).
//! Lane shape (M4.4): all tests here run in seconds.

use std::path::Path;

use flecs_ecs::prelude::World;

use camcraft_app::cl::ArcPlane;
use camcraft_app::commands::{CreateSetup, History, ImportModel};
use camcraft_app::geometry::import;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube, feature};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::winding::{BodyWinding, verdict_for, winding_of};
use camcraft_app::pick::{PlaneLay, align_321, lay_on_plane, plane_side};

const G17: PlaneLay = PlaneLay { plane: ArcPlane::Xy, flip: false, place: false };

fn block() -> Mesh {
    cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0))
}

/// Every facet wound the other way: the classic inside-out STL.
fn inverted(m: &Mesh) -> Mesh {
    Mesh::from_facets(m.facets.iter().map(|t| Triangle::new(t.a, t.c, t.b)).collect())
}

fn world_of(mesh: Mesh) -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("p", mesh));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    world
}

fn features(w: &World) -> std::sync::Arc<feature::Features> {
    let e = model::find_by_name(w, "p").expect("part");
    model::features_of(w, &e).expect("features")
}

/// The face of `p` whose plane holds point `at`.
fn face(w: &World, at: [f64; 3]) -> GeoSel {
    let f = features(w);
    let id = f
        .faces
        .iter()
        .find(|f| (f.normal[0] * at[0] + f.normal[1] * at[1] + f.normal[2] * at[2] - f.offset).abs() < 1e-9)
        .map(|f| f.id)
        .expect("the part has that face");
    GeoSel { entity: "p".into(), feature: FeatureSel::Face(id) }
}

fn edge(w: &World, a: [f64; 3], b: [f64; 3]) -> GeoSel {
    let f = features(w);
    let id = feature::chains(&f)
        .into_iter()
        .find(|c| {
            let p = feature::chain_points(&f, c);
            p.len() == 2 && ((p[0] == a && p[1] == b) || (p[0] == b && p[1] == a))
        })
        .map(|c| c.id)
        .expect("the part has that edge");
    GeoSel { entity: "p".into(), feature: FeatureSel::Chain(id) }
}

const TOP: [f64; 3] = [10.0, 10.0, 10.0];
const FRONT_TOP: ([f64; 3], [f64; 3]) = ([0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
const RIGHT_TOP: ([f64; 3], [f64; 3]) = ([20.0, 0.0, 10.0], [20.0, 20.0, 10.0]);

fn assert_named(e: &str, what: &str) {
    assert!(e.starts_with("Face ") && !e.contains("0x"), "the refusal names the face without hex: {e}");
    assert!(e.contains("of 'p'"), "the refusal names the part: {e}");
    assert!(e.contains(what), "the refusal says why ({what}): {e}");
}

#[test]
fn an_inside_out_cube_refuses_the_face_lay_by_name() {
    let w = world_of(inverted(&block()));
    let top = face(&w, TOP);
    let e = lay_on_plane(&w, std::slice::from_ref(&top), G17).expect_err("refused");
    assert_named(&e, "inside-out");
    assert!(e.starts_with(&camcraft_app::pick::geo_label(&w, &top)), "names the picked face: {e}");
    let e = plane_side(&w, &[top]).expect_err("the side readout refuses too");
    assert_named(&e, "inside-out");
}

#[test]
fn an_inside_out_cube_refuses_two_edges_as_edges() {
    // #554: the two-edge lay is gone; the refusal is the edge's, before
    // any winding is read.
    let w = world_of(inverted(&block()));
    let picks = [edge(&w, FRONT_TOP.0, FRONT_TOP.1), edge(&w, RIGHT_TOP.0, RIGHT_TOP.1)];
    let e = lay_on_plane(&w, &picks, G17).expect_err("refused");
    assert!(e.contains("an edge cannot set a plane"), "{e}");
}

#[test]
fn an_inside_out_cube_refuses_the_321_by_name() {
    let w = world_of(inverted(&block()));
    let picks = [face(&w, TOP), edge(&w, FRONT_TOP.0, FRONT_TOP.1)];
    let e = align_321(&w, &picks).expect_err("refused");
    assert_named(&e, "inside-out");
}

#[test]
fn the_same_cube_wound_outward_still_aligns() {
    let w = world_of(block());
    let a = lay_on_plane(&w, &[face(&w, TOP)], PlaneLay { plane: ArcPlane::Xy, flip: true, place: false })
        .expect("face lay");
    // Flipped onto -Z: the top (local +Z) turns over, a half turn.
    assert!(a.rot[3].abs() < 1e-9, "a half turn: {a:?}");
    let a = align_321(&w, &[face(&w, TOP), edge(&w, FRONT_TOP.0, FRONT_TOP.1)]).expect("3-2-1");
    assert!((a.z[2] - 1.0).abs() < 1e-9, "Z out of the top: {:?}", a.z);
}

#[test]
fn an_open_box_refuses_face_normal_picks_by_name() {
    // The block with its bottom (z = 0) torn off.
    let f: Vec<Triangle> = block()
        .facets
        .iter()
        .copied()
        .filter(|t| !(t.a.z == 0.0 && t.b.z == 0.0 && t.c.z == 0.0))
        .collect();
    let w = world_of(Mesh::from_facets(f));
    let e = lay_on_plane(&w, &[face(&w, TOP)], G17).expect_err("refused");
    assert_named(&e, "open body");
    let picks = [face(&w, TOP), edge(&w, FRONT_TOP.0, FRONT_TOP.1)];
    assert_named(&align_321(&w, &picks).expect_err("refused"), "open body");
}

#[test]
fn mixed_and_non_manifold_bodies_refuse_by_name() {
    // One facet of the bottom flipped: wound both ways.
    let mut f = block().facets.to_vec();
    let i = f.iter().position(|t| t.a.z == 0.0 && t.b.z == 0.0 && t.c.z == 0.0).expect("a bottom facet");
    f[i] = Triangle::new(f[i].a, f[i].c, f[i].b);
    let w = world_of(Mesh::from_facets(f));
    assert_named(&lay_on_plane(&w, &[face(&w, TOP)], G17).expect_err("refused"), "wound both ways");
    // A fin hung off the top's front edge: three facets on one edge.
    let mut f = block().facets.to_vec();
    f.push(Triangle::new(Point3::new(0.0, 0.0, 10.0), Point3::new(20.0, 0.0, 10.0), Point3::new(10.0, -5.0, 15.0)));
    let w = world_of(Mesh::from_facets(f));
    assert_named(&lay_on_plane(&w, &[face(&w, TOP)], G17).expect_err("refused"), "non-manifold");
}

#[test]
fn bodies_are_judged_one_by_one() {
    // An outward block beside an inside-out one, in one mesh: the good
    // body's face still lays, the bad body's face refuses.
    let good = block();
    let bad = inverted(&cube(Point3::new(40.0, 0.0, 0.0), Point3::new(60.0, 20.0, 5.0)));
    let w = world_of(Mesh::from_facets(good.facets.iter().chain(bad.facets.iter()).copied().collect()));
    let wind = winding_of(&model::facets_of(&w, &model::find_by_name(&w, "p").expect("p")).expect("facets"));
    assert_eq!(wind.bodies.len(), 2, "two bodies");
    lay_on_plane(&w, &[face(&w, TOP)], G17).expect("the good block lays");
    let e = lay_on_plane(&w, &[face(&w, [50.0, 10.0, 5.0])], G17).expect_err("the bad block refuses");
    assert_named(&e, "inside-out");
}

#[test]
fn the_shipped_test_models_read_as_outward() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models");
    for name in ["cube.stl", "box.obj", "box.step", "Milling-test.step", "sample_ap214.step"] {
        let m = import::load_model(&dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let w = winding_of(&m.facets);
        let all: Vec<u32> = (0..m.facets.len() as u32).collect();
        assert_eq!(verdict_for(&w, &all), Some(BodyWinding::Outward), "{name}: bodies {:?}", w.bodies);
    }
}

#[test]
fn the_inside_out_stl_fixture_is_turned_outward_at_import_and_lays() {
    // testdata/models/cube_inside_out.stl: a 20 x 20 x 10 block exported
    // inside-out (winding and stored normals both inward) - the GUI repro.
    // #1672 (PR #1686): the importer turns a closed, isolated inward body
    // outward and notes it (#1752: this test still expected the old
    // refusal). The refusal of an inside-out body that reaches the world
    // unturned stays pinned by the in-memory tests above; a nested inward
    // shell, which cannot be turned safely, is refused at import
    // (import_outward.rs).
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/cube_inside_out.stl");
    let g = import::load_model_graded(&path).expect("the fixture imports");
    assert_eq!(g.notes.len(), 1, "one note for the turn: {:?}", g.notes);
    assert!(
        g.notes[0].contains("1 closed solid(s) wound inward") && g.notes[0].contains("12 facet(s) reversed"),
        "the import notes the turn of all 12 facets: {:?}",
        g.notes
    );
    assert_eq!(winding_of(&g.mesh.facets).bodies, vec![BodyWinding::Outward]);
    // The plain loader gives the same turned mesh.
    let m = import::load_model(&path).expect("the fixture imports");
    assert_eq!(m, g.mesh);
    let w = world_of(m);
    let a = lay_on_plane(&w, &[face(&w, TOP)], PlaneLay { plane: ArcPlane::Xy, flip: true, place: false })
        .expect("the turned block lays on its top face");
    assert!(a.rot[3].abs() < 1e-9, "a half turn, as the outward block: {a:?}");
}
