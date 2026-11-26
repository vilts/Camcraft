//! Rotary operations and a face selection (#2848): Rotary Parallel
//! keeps its passes over the picked faces of the turned surface - the
//! axis, the gouge laws and the links still read the whole model - and
//! a selection a rotary kind cannot use is refused by name, never cut
//! as the whole model. The fixture is the rotary suite's laid bar
//! (`common::rotary_bar`): wall r6 over x -16..-2, a shoulder cone to
//! x 2, the neck r5 to x 16.
//! Provenance: our own fixtures and engines; the law is the Prime
//! Directive's "correct, or visibly refused".
//!
//! Gate map (M4.4): covers app::operation, motion::multiaxis.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::geometry::feature::{self, Features};
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, SrcRef};
use camcraft_app::schema::Operation;
use common::rotary_bar::{params, world_for, Kind, R};

fn features(world: &World) -> std::sync::Arc<Features> {
    let e = model::find_by_name(world, "bar").expect("bar");
    model::features_of(world, &e).expect("features")
}

/// The station (min, max) of one face's facets along the bar (local x
/// on the laid bar, local z on the standing one).
fn span(world: &World, f: &feature::Face, lying: bool) -> (f64, f64) {
    let e = model::find_by_name(world, "bar").expect("bar");
    let tris = model::facets_of(world, &e).expect("facets");
    let mut s = (f64::INFINITY, f64::NEG_INFINITY);
    for &i in &f.facets {
        let t = tris[i as usize];
        for p in [t.a, t.b, t.c] {
            let v = if lying { p.x } else { p.z };
            s = (s.0.min(v), s.1.max(v));
        }
    }
    s
}

/// The side strips of the bar whose stations lie in `lo..=hi`, each as
/// its own face pick.
fn strips(world: &World, lo: f64, hi: f64, lying: bool) -> Vec<GeoSel> {
    let fs = features(world);
    let picks: Vec<GeoSel> = fs
        .faces
        .iter()
        .filter(|f| {
            let (a, b) = span(world, f, lying);
            a >= lo - 1e-6 && b <= hi + 1e-6 && b - a > 1.0
        })
        .map(|f| GeoSel { entity: "bar".into(), feature: FeatureSel::Face(f.id) })
        .collect();
    assert!(!picks.is_empty(), "the bar has side strips in {lo}..{hi}");
    picks
}

/// The flat end cap at station `at`.
fn cap(world: &World, at: f64, lying: bool) -> GeoSel {
    let fs = features(world);
    let f = fs
        .faces
        .iter()
        .find(|f| {
            let (a, b) = span(world, f, lying);
            (a - at).abs() < 1e-6 && (b - at).abs() < 1e-6
        })
        .expect("an end cap");
    GeoSel { entity: "bar".into(), feature: FeatureSel::Face(f.id) }
}

/// The fixture's op with its source set to `picks`.
fn picked(world: &World, picks: Vec<GeoSel>) -> Operation {
    let e = model::find_by_name(world, "OP").expect("op");
    let mut op = model::operation_of(world, &e).expect("operation");
    op.src = SrcRef::Selection(picks);
    op
}

/// The stations (world x) of the ball centre at every feed and plunge
/// end of the laid bar's program.
fn cut_stations(path: &camcraft_app::cl::Toolpath) -> Vec<f64> {
    path.moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .map(|m| m.to.x + R * m.axis.map_or(0.0, |a| a[0]))
        .collect()
}

#[test]
fn rotary_parallel_keeps_its_cut_over_the_picked_faces() {
    let world = world_for(Kind::Parallel);
    // The whole model: the passes run the bar's full length.
    let e = model::find_by_name(&world, "OP").expect("op");
    let whole = model::operation_of(&world, &e).expect("operation");
    let (full, _) = operation::generate(&world, &whole, "").expect("the whole bar");
    let xs = cut_stations(&full);
    assert!(xs.iter().any(|x| *x < -10.0) && xs.iter().any(|x| *x > 10.0), "the whole model is cut end to end");

    // The neck's strips picked: the cut keeps to the neck's stations.
    let op = picked(&world, strips(&world, 2.0, 16.0, true));
    let (path, warns) = operation::generate(&world, &op, "").expect("the picked neck generates");
    let xs = cut_stations(&path);
    assert!(!xs.is_empty(), "the neck is cut");
    for x in &xs {
        assert!((2.0 - 1e-3..=16.0 + 1e-3).contains(x), "a cut at station {x} is off the picked neck (2..16)");
    }
    assert!(xs.iter().any(|x| *x < 3.0) && xs.iter().any(|x| *x > 15.0), "the neck is cut from end to end: {xs:?}");
    // Every move, links included, stays over the picked stations.
    for m in &path.moves {
        let x = m.to.x + R * m.axis.map_or(0.0, |a| a[0]);
        assert!((2.0 - 1e-3..=16.0 + 1e-3).contains(&x), "{:?} at station {x} leaves the picked neck", m.kind);
    }
    let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
    assert!(blocking.is_empty(), "the kept passes verify silent: {blocking:?}");
    // The wall's strips picked: the other end, and only it.
    let op = picked(&world, strips(&world, -16.0, -2.0, true));
    let (path, _) = operation::generate(&world, &op, "").expect("the picked wall generates");
    for x in cut_stations(&path) {
        assert!((-16.0 - 1e-3..=-2.0 + 1e-3).contains(&x), "a cut at station {x} is off the picked wall (-16..-2)");
    }
}

#[test]
fn rotary_parallel_reads_a_welded_wall_pick_as_its_strips() {
    let world = world_for(Kind::Parallel);
    let fs = features(&world);
    let neck = strips(&world, 2.0, 16.0, true);
    let FeatureSel::Face(one) = neck[0].feature else { panic!("a strip is a face") };
    let patch = feature::patches(&fs).into_iter().find(|p| p.faces.contains(&one)).expect("the neck's strips weld into a patch");
    // The weld's own stations: on this bar the wall, the shoulder and
    // the neck are one smooth patch, so the pick is the turned skin and
    // leaves out the two end caps the whole-model raster rolls over.
    let (lo, hi) = patch
        .faces
        .iter()
        .filter_map(|id| fs.faces.iter().find(|f| f.id == *id))
        .map(|f| span(&world, f, true))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), (a, b)| (l.min(a), h.max(b)));
    let op = picked(&world, vec![GeoSel { entity: "bar".into(), feature: FeatureSel::Patch(patch.id) }]);
    let (path, _) = operation::generate(&world, &op, "").expect("the picked patch generates");
    let xs = cut_stations(&path);
    assert!(!xs.is_empty(), "the patch is cut");
    for x in xs {
        assert!((lo - 1e-3..=hi + 1e-3).contains(&x), "a cut at station {x} is off the picked patch ({lo}..{hi})");
    }
}

#[test]
fn rotary_parallel_keeps_to_a_sector_and_still_turns_about_the_whole_models_axis() {
    let world = world_for(Kind::Parallel);
    let e = model::find_by_name(&world, "bar").expect("bar");
    let tris = model::facets_of(&world, &e).expect("facets");
    let fs = features(&world);
    // The neck's strips in the quadrant y >= 0, z >= 0: a quarter of the turn.
    let quarter: Vec<GeoSel> = strips(&world, 2.0, 16.0, true)
        .into_iter()
        .filter(|g| {
            let FeatureSel::Face(id) = g.feature else { return false };
            let f = fs.faces.iter().find(|f| f.id == id).expect("the strip");
            f.facets.iter().all(|&i| {
                let t = tris[i as usize];
                [t.a, t.b, t.c].iter().all(|p| p.y >= -1e-9 && p.z >= -1e-9)
            })
        })
        .collect();
    assert_eq!(quarter.len(), 4, "a 16-sided bar has four strips in a quadrant");
    let (path, warns) = operation::generate(&world, &picked(&world, quarter), "").expect("the picked quarter generates");
    let mut cuts = 0;
    for m in path.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        let a = m.axis.expect("a rotary cut states its pose");
        let c = [m.to.x + R * a[0], m.to.y + R * a[1], m.to.z + R * a[2]];
        assert!((2.0 - 1e-3..=16.0 + 1e-3).contains(&c[0]), "a cut at station {} is off the picked strips", c[0]);
        assert!(c[1] >= -1e-3 && c[2] >= -1e-3, "a cut at ({}, {}) about the axis is off the picked quarter", c[1], c[2]);
        // The pose is the radial of the WHOLE bar's axis (the world x
        // line): four strips alone state no axis of revolution.
        let rho = c[1].hypot(c[2]);
        assert!(a[0].abs() < 1e-6 && (a[1] * c[2] - a[2] * c[1]).abs() < 1e-6 * rho, "the pose {a:?} at {c:?} is not the bar's radial");
        cuts += 1;
    }
    assert!(cuts > 0, "the quarter is cut");
    let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
    assert!(blocking.is_empty(), "the kept passes verify silent: {blocking:?}");
}

fn refusal(world: &World, op: &Operation) -> String {
    match operation::generate(world, op, "") {
        Ok(_) => panic!("the selection must be refused, not cut"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn rotary_parallel_refuses_a_selection_it_cannot_use_by_name() {
    let world = world_for(Kind::Parallel);
    // Two groups of faces with the shoulder between them.
    let mut two = strips(&world, 2.0, 16.0, true);
    two.extend(strips(&world, -16.0, -2.0, true));
    let e = refusal(&world, &picked(&world, two));
    assert!(e.contains("Rotary Parallel") && e.contains("not connected"), "{e}");
    // An end cap: the radial tool sees it edge-on.
    let e = refusal(&world, &picked(&world, vec![cap(&world, 16.0, true)]));
    assert!(e.contains("Rotary Parallel") && e.contains("does not face away from the work's axis"), "{e}");
    // The same cap picked with the neck is still refused: no pick is dropped.
    let mut mixed = strips(&world, 2.0, 16.0, true);
    mixed.push(cap(&world, 16.0, true));
    let e = refusal(&world, &picked(&world, mixed));
    assert!(e.contains("does not face away from the work's axis"), "{e}");
    // An edge chain is no face.
    let fs = features(&world);
    let chain = feature::chains(&fs).into_iter().next().expect("the bar has an edge chain");
    let e = refusal(&world, &picked(&world, vec![GeoSel { entity: "bar".into(), feature: FeatureSel::Chain(chain.id) }]));
    assert!(e.contains("Rotary Parallel") && e.contains("is not a face"), "{e}");
}

#[test]
fn rotary_contour_and_offset_refuse_a_face_selection_by_name() {
    for (kind, label) in [(Kind::Contour, "Rotary Contour"), (Kind::Offset, "Rotary Offset")] {
        let world = world_for(kind);
        let e = refusal(&world, &picked(&world, strips(&world, 2.0, 16.0, false)));
        assert!(e.contains(label) && e.contains("cannot yet keep to picked faces"), "{label}: {e}");
        // The whole model still generates, as it always did.
        let op = model::find_by_name(&world, "OP").and_then(|e| model::operation_of(&world, &e)).expect("op");
        assert_eq!(op.params, params(kind));
        operation::generate(&world, &op, "").unwrap_or_else(|e| panic!("{label} on the whole model: {e}"));
    }
}
