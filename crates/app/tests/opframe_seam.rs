//! #763 Phase 1: the machining-frame seam's contract, pinned where it
//! reads the world — the plain and zero-tilt paths byte-identical, a
//! tilted face running in its own plane, the 90/180-degree alignments,
//! the alignment x tilt composition against hand math, local heights and
//! boxes read from fully transformed geometry, and the Phase 1 refusal
//! boundary (picks stay refused, by name). The post's tilted-move gate
//! keeps its own pin in `tests/inverted_post.rs`; the four verified
//! kinds keep theirs in the #326 suites (`plane_align`, `axis_align`,
//! `rest_across_setups`), all run by the same gate.
//!
//! Provenance: our own fixtures over plain datum-face practice; no
//! vendor material consulted.
//!
//! Gate map (#763): covers app::operation::frame, app::operation::job
//! (the collect seam), app::setup_align (composed reads).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, History, ImportModel, SetStock,
};
use camcraft_app::geometry::{Point3, cube, transform_point};
use camcraft_app::model;
use camcraft_app::operation::{
    FeatureSel, GeoSel, Height, HeightReference, OpFrame, OpParams, SrcRef, Tilt, generate,
};
use camcraft_app::setup_align;
use camcraft_app::setup_turn::TurnPivot;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TIGHT: f64 = 1e-9;
const EM: &str = "EM-0400-2FL";

/// A 20 x 20 x 10 block at the origin, Setup1, box stock, one end mill.
fn block() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(
        &world,
        ImportModel::from_mesh(
            "block",
            cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0)),
        ),
    );
    h.execute(
        &world,
        CreateSetup::new(&world, None, None).expect("Setup1"),
    );
    h.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 60.0,
                dy: 60.0,
                dz: 20.0,
            },
            Default::default(),
        )
        .expect("box stock"),
    );
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == EM)
        .map(|(_, t)| t)
        .expect("catalog end mill");
    h.execute(
        &world,
        AddTool::new(&world, EM, tool).expect("rack the end mill"),
    );
    (world, h)
}

fn face(cut: Height, clearance: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(clearance),
        cut_z: cut,
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

fn add_face(world: &World, h: &mut History, name: &str, src: SrcRef, params: OpParams) {
    h.execute(
        world,
        CreateOperation::new(world, name, EM, src, params).expect("create the face op"),
    );
}

fn op_of(world: &World, name: &str) -> camcraft_app::schema::Operation {
    let e = model::find_by_name(world, name).expect("the op exists");
    model::operation_of(world, &e).expect("an operation")
}

fn t45x(origin: [f64; 3]) -> Tilt {
    Tilt {
        basis_version: 1,
        a_deg: 45.0,
        b_deg: 0.0,
        origin,
    }
}

#[test]
fn a_plain_face_keeps_its_prismatic_numbers_through_the_seam() {
    let (world, mut h) = block();
    add_face(
        &world,
        &mut h,
        "Face1",
        SrcRef::Entity("block".into()),
        face(Height::fixed(9.0), 25.0),
    );
    let (path, _) =
        generate(&world, &op_of(&world, "Face1"), "Face1").expect("the plain face generates");
    assert!(
        path.moves.len() > 10,
        "a real sweep: {} moves",
        path.moves.len()
    );
    let mut fed_at_cut = false;
    for m in &path.moves {
        assert!(
            m.to.z <= 25.0 + TIGHT,
            "nothing above the clearance: {:?}",
            m.to
        );
        assert!(
            m.to.z >= 9.0 - TIGHT,
            "nothing below the cut plane: {:?}",
            m.to
        );
        if m.feed.is_some() && (m.to.z - 9.0).abs() < TIGHT {
            fed_at_cut = true;
        }
    }
    assert!(fed_at_cut, "the sweep feeds at the cut plane");
}

#[test]
fn a_zero_tilt_index_reproduces_the_prismatic_path_exactly() {
    let (world, mut h) = block();
    add_face(
        &world,
        &mut h,
        "Plain",
        SrcRef::Entity("block".into()),
        face(Height::fixed(9.0), 25.0),
    );
    let mut tilted = face(Height::fixed(9.0), 25.0);
    if let OpParams::Face { tilt, .. } = &mut tilted {
        *tilt = Some(Tilt {
            basis_version: 1,
            a_deg: 0.0,
            b_deg: 0.0,
            origin: [0.0; 3],
        });
    }
    add_face(
        &world,
        &mut h,
        "ZeroTilt",
        SrcRef::Entity("block".into()),
        tilted,
    );
    let (plain, _) = generate(&world, &op_of(&world, "Plain"), "Plain").expect("plain generates");
    let (zero, _) =
        generate(&world, &op_of(&world, "ZeroTilt"), "ZeroTilt").expect("zero tilt generates");
    assert_eq!(
        plain.moves.len(),
        zero.moves.len(),
        "the identity frame adds no move"
    );
    for (a, b) in plain.moves.iter().zip(zero.moves.iter()) {
        assert_eq!(a.kind, b.kind, "kinds match");
        assert_eq!(a.feed, b.feed, "feeds match");
        assert_eq!(a.from, b.from, "from matches: {:?} vs {:?}", a.from, b.from);
        assert_eq!(a.to, b.to, "to matches: {:?} vs {:?}", a.to, b.to);
    }
}

#[test]
fn a_tilted_face_runs_in_its_own_plane() {
    let (world, mut h) = block();
    let mut p = face(Height::fixed(9.0), 25.0);
    if let OpParams::Face { tilt, .. } = &mut p {
        *tilt = Some(t45x([10.0, 10.0, 5.0]));
    }
    add_face(&world, &mut h, "Tilted", SrcRef::Entity("block".into()), p);
    let (path, _) =
        generate(&world, &op_of(&world, "Tilted"), "Tilted").expect("the tilted face generates");
    assert!(!path.moves.is_empty(), "a real path");
    let opframe = OpFrame::new(None, Some(t45x([10.0, 10.0, 5.0])));
    let frame = opframe
        .checked_frame()
        .expect("finite letters")
        .expect("a frame");
    // The L1 fold reads the heights in the frame: cut 9 - origin.z 5 = 4,
    // clearance 25 - 5 = 20. Every endpoint maps back into that band,
    // and the run stays over the block's local silhouette.
    let mut fed_at_cut = false;
    // The sweep covers the local silhouette plus the cutter's radius
    // (a 4 mm end mill: the turnaround at x = -12 is 10 + r) and its
    // clearance hops; the height band is the L1-folded pair exactly.
    for m in &path.moves {
        for p in [m.from, m.to] {
            let l = frame.to_local(p);
            assert!(
                l.z >= 4.0 - TIGHT && l.z <= 20.0 + TIGHT,
                "local height {} out of band at {:?}",
                l.z,
                p
            );
            assert!(
                l.x.abs() <= 12.5,
                "local x {} off the sweep at {:?}",
                l.x,
                p
            );
            assert!(
                l.y.abs() <= 13.1,
                "local y {} off the sweep at {:?}",
                l.y,
                p
            );
        }
        if m.feed.is_some() && (frame.to_local(m.to).z - 4.0).abs() < TIGHT {
            fed_at_cut = true;
        }
    }
    assert!(fed_at_cut, "the sweep feeds at the tilted cut plane");
    // The tool axis the gates read: the commanded 45-degree X letters.
    let axis = opframe.tool_axis();
    assert!(
        (axis[0] - 0.0).abs() < TIGHT
            && (axis[1] + 1.0 / 2.0_f64.sqrt()).abs() < TIGHT
            && (axis[2] - 1.0 / 2.0_f64.sqrt()).abs() < TIGHT
    );
}

#[test]
fn aligned_ninety_turns_the_solid_and_its_heights() {
    let (world, mut h) = block();
    h.execute(
        &world,
        AlignPartToSetup::turn(&world, "Setup1", [90.0, 0.0, 0.0], TurnPivot::PartCentre)
            .expect("turn 90"),
    );
    let f = OpFrame::new(Some("Setup1".into()), None);
    // The 20-deep Y becomes the Z, about the bounds centre (10, 10, 5).
    let b = f.solid_aabb_in_setup(&world).expect("the solid reads");
    assert!((b.max.z - 15.0).abs() < TIGHT, "turned top: {}", b.max.z);
    assert!((b.min.z + 5.0).abs() < TIGHT, "turned bottom: {}", b.min.z);
    // The seam's S resolve reads the turned bounds: TopOfModel - 2 = 13
    // (the unaligned frame would say 8 - the numbers the part gives).
    let mut p = face(
        Height {
            reference: HeightReference::TopOfModel,
            offset: -2.0,
        },
        25.0,
    );
    if let OpParams::Face { tilt, .. } = &mut p {
        *tilt = None;
    }
    add_face(&world, &mut h, "FaceA", SrcRef::Entity("block".into()), p);
    let op = op_of(&world, "FaceA");
    let opframe = OpFrame::of(&op);
    let hs = opframe
        .resolve_heights(&world, Some(&op.src), &op.params)
        .expect("the heights resolve");
    assert!(
        (hs.bottom.expect("the face cut") - 13.0).abs() < TIGHT,
        "cut reads the turned top"
    );
}

#[test]
fn aligned_one_eighty_mirrors_the_facets() {
    let (world, mut h) = block();
    h.execute(
        &world,
        AlignPartToSetup::preset(&world, "Setup1", setup_align::AlignPreset::TurnZ180)
            .expect("preset"),
    );
    let f = OpFrame::new(Some("Setup1".into()), None);
    let e = model::find_by_name(&world, "block").expect("block");
    let turned = f.facets_in_setup(&world, &e).expect("turned facets");
    let doc = model::world_facets(&world, &e).expect("world facets");
    assert_eq!(turned.len(), doc.len(), "the mirror moves no triangle");
    // TurnZ180 about the bounds centre (10, 10, 5): x -> 20 - x, y -> 20 - y.
    for (ts, td) in turned.iter().zip(doc.iter()) {
        for (ps, pd) in [(ts.a, td.a), (ts.b, td.b), (ts.c, td.c)] {
            assert!(
                (ps.x - (20.0 - pd.x)).abs() < TIGHT,
                "x mirrors: {ps:?} vs {pd:?}"
            );
            assert!(
                (ps.y - (20.0 - pd.y)).abs() < TIGHT,
                "y mirrors: {ps:?} vs {pd:?}"
            );
            assert!((ps.z - pd.z).abs() < TIGHT, "z holds: {ps:?} vs {pd:?}");
        }
    }
}

#[test]
fn alignment_times_tilt_composes_at_the_seam() {
    let (world, mut h) = block();
    h.execute(
        &world,
        AlignPartToSetup::turn(&world, "Setup1", [90.0, 0.0, 0.0], TurnPivot::PartCentre)
            .expect("turn 90"),
    );
    let opframe = OpFrame::new(Some("Setup1".into()), Some(t45x([10.0, 10.0, 5.0])));
    let e = model::find_by_name(&world, "block").expect("block");
    let facets_s = opframe
        .facets_in_setup(&world, &e)
        .expect("the aligned read");
    let local = opframe.local_facets(&facets_s);
    // Expected A: the alignment composed with the entity's Location.
    let a = setup_align::alignment(&world, Some("Setup1")).expect("stored alignment");
    let loc = setup_align::compose(a, model::transform_of(&world, &e).unwrap_or_default());
    let doc = model::world_facets(&world, &e).expect("document facets");
    assert_eq!(local.len(), doc.len());
    // Expected T, by hand: axis = (0, -s, c), u = X, v = (0, c, s).
    let (s, c) = ((45.0_f64).to_radians().sin(), (45.0_f64).to_radians().cos());
    let o = [10.0, 10.0, 5.0];
    for (tl, td) in local.iter().zip(doc.iter()) {
        for (pl, pd) in [(tl.a, td.a), (tl.b, td.b), (tl.c, td.c)] {
            let w = transform_point(pd, loc.pos, loc.rot);
            let d = [w.x - o[0], w.y - o[1], w.z - o[2]];
            let want = [d[0], d[1] * c + d[2] * s, -d[1] * s + d[2] * c];
            assert!((pl.x - want[0]).abs() < TIGHT, "x: {pl:?} vs {want:?}");
            assert!((pl.y - want[1]).abs() < TIGHT, "y: {pl:?} vs {want:?}");
            assert!((pl.z - want[2]).abs() < TIGHT, "z: {pl:?} vs {want:?}");
        }
    }
}

#[test]
fn local_heights_and_boxes_read_fully_transformed_geometry() {
    let (world, _h) = block();
    let f = OpFrame::new(None, Some(t45x([0.0, 0.0, 0.0])));
    // The block (0..20, 0..20, 0..10) turned 45 deg about X: local z runs
    // -20s .. 10c (hand), never the S box's 0..10 shifted by -origin.z.
    let b = f.solid_aabb_local(&world).expect("the local box");
    let (s, c) = ((45.0_f64).to_radians().sin(), (45.0_f64).to_radians().cos());
    assert!((b.max.z - 10.0 * c).abs() < TIGHT, "local top: {}", b.max.z);
    assert!(
        (b.min.z + 20.0 * s).abs() < TIGHT,
        "local bottom: {}",
        b.min.z
    );
    assert!(
        (b.max.z - 10.0).abs() > 1.0,
        "the local box is transformed, not z-shifted"
    );
    // The same law resolves heights: TopOfModel is the local top.
    let top = f.height_z_local(
        &world,
        Height {
            reference: HeightReference::TopOfModel,
            offset: 0.0,
        },
    );
    assert!((top.expect("the model top resolves") - 10.0 * c).abs() < TIGHT);
    // Fixed passes through as spoken along the tool axis; a missing WCS
    // is None, never guessed.
    let fixed = f.height_z_local(&world, Height::fixed(7.5));
    assert!((fixed.expect("fixed reads through") - 7.5).abs() < TIGHT);
    assert!(
        f.height_z_local(
            &world,
            Height {
                reference: HeightReference::WorkOffset,
                offset: 0.0
            }
        )
        .is_none(),
        "no WCS placed: unresolved, never guessed"
    );
}

#[test]
fn picks_in_an_aligned_setup_read_through_opframe_phase_two() {
    let (world, mut h) = block();
    h.execute(
        &world,
        AlignPartToSetup::turn(&world, "Setup1", [90.0, 0.0, 0.0], TurnPivot::PartCentre)
            .expect("turn 90"),
    );
    let e = model::find_by_name(&world, "block").expect("block");
    let feats = model::features_of(&world, &e).expect("features");
    // Document +Y face at y = 20 turns 90 deg about X around (10,10,5) to become horizontal TOP (z = 15).
    let id = feats
        .faces
        .iter()
        .find(|fa| fa.normal[1].abs() > 0.9 && (fa.offset - 20.0).abs() < 1e-6)
        .expect("top-after-turn face")
        .id;
    let sel = SrcRef::Selection(vec![GeoSel {
        entity: "block".into(),
        feature: FeatureSel::Face(id),
    }]);
    add_face(&world, &mut h, "FacePick", sel, face(Height::fixed(9.0), 25.0));
    let op = op_of(&world, "FacePick");
    generate(&world, &op, "FacePick").expect("picks generate in an aligned setup");
    let opframe = OpFrame::of(&op);
    let aligned = opframe.facets_in_setup(&world, &e).expect("the aligned door reads");
    let pose = opframe.pose_in_setup(&world, &e).expect("the aligned pose reads");
    assert_eq!(aligned.len(), model::world_facets(&world, &e).expect("world facets").len());
    let a = setup_align::alignment(&world, Some("Setup1")).expect("stored alignment");
    let loc = setup_align::compose(a, model::transform_of(&world, &e).unwrap_or_default());
    assert!((pose.pos.x - loc.pos.x).abs() < TIGHT && (pose.pos.y - loc.pos.y).abs() < TIGHT && (pose.pos.z - loc.pos.z).abs() < TIGHT);
    let doc = model::world_facets(&world, &e).expect("world facets");
    let want = transform_point(doc[0].a, loc.pos, loc.rot);
    assert!(
        (aligned[0].a.x - want.x).abs() < TIGHT
            && (aligned[0].a.y - want.y).abs() < TIGHT
            && (aligned[0].a.z - want.z).abs() < TIGHT,
        "the door reads the composed alignment: {:?} vs {:?}",
        aligned[0].a,
        want
    );
}
