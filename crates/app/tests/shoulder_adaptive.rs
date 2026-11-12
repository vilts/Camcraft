//! The 0.8.0 hand test on 2D Adaptive over the user's "Milling-test"
//! (testdata STEP: a 60 x 50 block, Z -5..20, notched sides, a round
//! recess r 12.5 down to Z 0 in its top). The face picked at Z -5
//! (~2500 mm^2) is the block's underside: its floor is its own Z, and
//! Containment Outside clears the band the tool reaches past that
//! outline - never the recess the band does not touch, and never a hole
//! of the selection.
//!
//! Gate map: covers app::operation (heights_sel, boundary, selection,
//! generate), sim::verify.
//! Provenance: our own regression of the user's report; no vendor
//! material consulted.

mod area_support;
mod common;

use area_support::*;
use camcraft_app::geo2d::Point2;
use camcraft_app::operation::HeightReference;

fn milling_test() -> (World, String) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let world = world_with(Some(ImportModel::new(&path).expect("the user's STEP imports")), -7.0, 27.0);
    let name = model::named_entities(&world)
        .into_iter()
        .map(|(n, _)| n)
        .find(|n| n != "Stock" && n != TOOL)
        .expect("the imported part");
    (world, name)
}

/// The recess in the part's top: (centre, radius) of the top face's hole.
fn recess(world: &World, name: &str) -> ((f64, f64), f64) {
    let FeatureSel::Face(id) = top_face(world, name, 20.0) else { panic!("a face") };
    let e = model::find_by_name(world, name).expect("model");
    let f = model::features_of(world, &e).expect("features");
    let ff = f.faces.iter().find(|x| x.id == id).expect("the face");
    let l = ff.loops.iter().find(|l| l.kind != feature::LoopKind::Outer).expect("the recess is the top's hole");
    let pts: Vec<[f64; 3]> = l.verts.iter().filter_map(|&v| f.verts.get(v as usize).copied()).collect();
    let n = pts.len() as f64;
    let c = (pts.iter().map(|p| p[0]).sum::<f64>() / n, pts.iter().map(|p| p[1]).sum::<f64>() / n);
    let r = pts.iter().map(|p| (p[0] - c.0).hypot(p[1] - c.1)).fold(f64::INFINITY, f64::min);
    (c, r)
}

fn adaptive(bottom: Height) -> OpParams {
    let mut p = operation::retype_params(&pocket(20.0, -5.0), OpType::Adaptive2D.ordinal());
    if let OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. } = &mut p {
        *top_z = Height::fixed(20.0);
        *bottom_z = bottom;
        *stepdown = 25.0; // one level, on the face: the debug lane stays fast
    }
    assert_eq!(p.kind(), OpType::Adaptive2D);
    p
}

const FLOOR: Height = Height { reference: HeightReference::BottomOfSelection, offset: 0.0 };

#[test]
fn a_face_selection_floor_is_the_faces_own_z() {
    let (world, name) = milling_test();
    let src = sel(&name, top_face(&world, &name, -5.0));
    let z = operation::resolve_height(&world, None, Some(&src), FLOOR).expect("resolves");
    assert!((z + 5.0).abs() < 1e-6, "the picked face's floor is Z -5, got {z}");
    let top = sel(&name, top_face(&world, &name, 20.0));
    let zt = operation::resolve_height(&world, None, Some(&top), FLOOR).expect("resolves");
    assert!((zt - 20.0).abs() < 1e-6, "the top face resolves to its own Z, got {zt}");
    let p = adaptive(FLOOR);
    assert_eq!(operation::selection_heights_sane(&world, None, &src, &p), None, "no height error");
    assert!(operation::depth_sane(&world, None, Some(&src), &p));
}

#[test]
fn containment_outside_clears_past_the_edge_never_the_recess() {
    let (world, name) = milling_test();
    let (c, recess_r) = recess(&world, &name);
    let src = sel(&name, top_face(&world, &name, -5.0));
    // Outside alone leaves a band one tool wide around the part - the
    // adaptive names that ("no room for an entry"), never the recess.
    let a = AreaControls { containment: Some(Containment::Outside), ..Default::default() };
    let err = run(&world, &op(src.clone(), adaptive(FLOOR), a.clone())).expect_err("a one-tool band has no entry room");
    assert!(!err.contains("fills the whole selection"), "Outside is not a height error: {err}");
    let preview = operation::region_of(&world, &op(src.clone(), adaptive(FLOOR), a)).expect("the band previews");
    assert!(!preview.contains(Point2::new(c.0, c.1)), "the recess is not in the machined area");
    // With room past the edge the band clears. 2D Adaptive itself: the
    // band is a channel ~270 mm long no single spiral should grow
    // through, so the planner refuses it by name at once (issue #13: it
    // used to grind for minutes) and its Offset rings cut it.
    let a = AreaControls { containment: Some(Containment::Outside), extension: 4.0, ..Default::default() };
    let band = operation::region_of(&world, &op(src.clone(), adaptive(FLOOR), a.clone())).expect("the band previews");
    assert!(!band.contains(Point2::new(c.0, c.1)), "the recess stays out with an extension too");
    let t0 = std::time::Instant::now();
    let (t, warns) = run(&world, &op(src, adaptive(FLOOR), a)).expect("generates");
    assert!(t0.elapsed().as_secs_f64() < 60.0, "the band plans in seconds, not minutes");
    assert_eq!(warns, 0, "the plan verifies clean: {:?}", t.notes);
    assert!(
        t.notes.iter().any(|n| n.contains("channel") && n.contains("Offset rings cut")),
        "the refusal is named: {:?}",
        t.notes
    );
    let f: Vec<&camcraft_app::cl::Move> = t.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    assert!(!f.is_empty());
    let lo = f.iter().map(|m| m.to.z).fold(f64::INFINITY, f64::min);
    assert!((lo + 5.0).abs() < 1e-6, "the last level sits on the picked face, got {lo}");
    let mut worst = f64::INFINITY;
    for m in &f {
        // An arc move is read along its arc (its chords at 0.01 mm, well
        // inside the 0.05 this asserts to), never as the straight from
        // `from` to `to`: since #689 a closed ring fits as whole arcs,
        // whose end-to-end straight crosses the recess the arc rounds.
        for (a, b) in m.chords(0.01) {
            for i in 0..=8 {
                let s = f64::from(i) / 8.0;
                let x = a.x + (b.x - a.x) * s;
                let y = a.y + (b.y - a.y) * s;
                worst = worst.min((x - c.0).hypot(y - c.1));
            }
        }
    }
    assert!(
        worst >= recess_r + R - 0.05,
        "a feed enters the recess (centre {worst:.3} from its axis, recess r {recess_r:.3})"
    );
}

#[test]
fn the_open_run_refusal_says_how_to_close_it() {
    let world = block_world();
    let rim = rim_edge(&world, |q| (q[1] - 20.0).abs() < 1e-9);
    let p = operation::retype_params(&pocket(10.0, 5.0), OpType::Adaptive2D.ordinal());
    let err = run(&world, &op(sel("part", rim), p, AreaControls::default())).expect_err("an open run refuses");
    assert!(
        err.contains("cannot end mid-edge")
            && err.ends_with("- Ctrl+click the remaining edges, or Shift+click to take the whole loop"),
        "{err}"
    );
}
