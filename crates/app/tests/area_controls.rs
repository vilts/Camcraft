//! Area control (M11.5), the region laws: the selection says where, the
//! Boundary settings say how far. Face extended past the part edge
//! clears to the stock outline plus the extension; tool containment
//! Inside / On / Outside moves the tool against the boundary on a pocket
//! and a contour; the offset grows and shrinks with round or sharp
//! corners; include / exclude boundaries combine by the region booleans;
//! an empty result refuses by name. (`area_open` carries the open
//! boundary, the legality, the command and the file.)
//!
//! Gate map (M11.5): covers app::operation (area, boundary, boundary_ctx,
//! job, generate), geo2d (offset_sharp), sim::verify.

mod area_support;
mod common;

use area_support::*;

#[test]
fn face_extended_past_the_part_edge_clears_to_the_stock_outline_plus_the_extension() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let world = world_with(Some(ImportModel::new(&path).expect("the user's STEP imports")), -7.0, 27.0);
    let name = model::named_entities(&world)
        .into_iter()
        .map(|(n, _)| n)
        .find(|n| n != "Stock" && n != TOOL)
        .expect("the imported part");
    let src = sel(&name, top_face(&world, &name, 20.0));
    let (lo, hi) = stock_box(&world);
    // Today's law: the facing stays on the picked face.
    let (base, warns) = run(&world, &op(src.clone(), face(20.0), AreaControls::default())).expect("default face");
    assert_eq!(warns, 0);
    let bf = feeds(&base);
    assert!(max_by(&bf, |p| p.y) + R < hi.y - 1.0, "the default stays inside the selection");
    // Extended by 5: off the part's edge, out to the stock and 5 past it.
    let ext = AreaControls { extension: 5.0, ..Default::default() };
    let (t, warns) = run(&world, &op(src, face(20.0), ext)).expect("extended face");
    assert_eq!(warns, 0, "the extended face must verify clean (depth-aware): {:?}", t.notes);
    let f = feeds(&t);
    assert!((max_by(&f, |p| p.y) + R - (hi.y + 5.0)).abs() < EPS, "rows reach the stock edge + 5");
    assert!((max_by(&f, |p| -p.y) + R - (5.0 - lo.y)).abs() < EPS, "both sides");
    assert!(max_by(&f, |p| p.x) - R >= hi.x + 5.0 - EPS, "the X runs leave the stock + 5 in air");
    assert!(max_by(&f, |p| -p.x) - R >= 5.0 - lo.x - EPS);
    assert!(f.iter().all(|p| (p.z - 20.0).abs() < 1e-9));
}

fn pocket_reach(world: &World, c: Option<Containment>, offset: f64) -> Result<f64, String> {
    let a = AreaControls { containment: c, offset, ..Default::default() };
    let (t, warns) = run(world, &op(SrcRef::Stock, pocket(8.0, 4.0), a))?;
    assert_eq!(warns, 0);
    Ok(max_by(&feeds(&t), |p| p.x))
}

#[test]
fn containment_inside_on_outside_moves_the_pocket_tool_against_the_boundary() {
    let world = world_with(None, 0.0, 10.0);
    let (_, hi) = stock_box(&world);
    let default = pocket_reach(&world, None, 0.0).expect("default");
    let inside = pocket_reach(&world, Some(Containment::Inside), 0.0).expect("inside");
    let on = pocket_reach(&world, Some(Containment::On), 0.0).expect("on");
    let outside = pocket_reach(&world, Some(Containment::Outside), 0.0).expect("outside");
    assert!((default - (hi.x - R)).abs() < EPS && (inside - default).abs() < 1e-9, "Inside is the pocket's own law");
    assert!((on - hi.x).abs() < EPS, "On: the centre rides the boundary ({on})");
    assert!((outside - (hi.x + R)).abs() < EPS, "Outside: the tool clears past it ({outside})");
}

#[test]
fn the_offset_grows_and_shrinks_with_round_or_sharp_corners() {
    let world = world_with(None, 0.0, 10.0);
    let (_, hi) = stock_box(&world);
    assert!((pocket_reach(&world, None, 3.0).expect("grow") - (hi.x + 3.0 - R)).abs() < EPS);
    assert!((pocket_reach(&world, None, -3.0).expect("shrink") - (hi.x - 3.0 - R)).abs() < EPS);
    let area_of = |corner: CornerStyle| {
        let a = AreaControls { offset: 3.0, corner, ..Default::default() };
        operation::region_of(&world, &op(SrcRef::Stock, pocket(8.0, 4.0), a)).expect("preview").area().abs()
    };
    let (round, sharp) = (area_of(CornerStyle::Round), area_of(CornerStyle::Sharp));
    assert!((sharp - 76.0 * 76.0).abs() < 0.01, "sharp keeps the square ({sharp})");
    assert!(round < sharp - 1.0, "round rounds the corners ({round})");
}

fn contour_reach(world: &World, c: Option<Containment>) -> (f64, String) {
    let a = AreaControls { containment: c, ..Default::default() };
    let (t, warns) = run(world, &op(SrcRef::Stock, contour(8.0, 4.0, Side::Auto), a)).expect("contour");
    assert_eq!(warns, 0);
    (max_by(&feeds(&t), |p| p.x), common::gen_to_cltext(&t))
}

#[test]
fn containment_inside_on_outside_moves_the_contour_tool_against_the_boundary() {
    let world = world_with(None, 0.0, 10.0);
    let (_, hi) = stock_box(&world);
    let (default, cl0) = contour_reach(&world, None);
    let (_outside, cl1) = contour_reach(&world, Some(Containment::Outside));
    let (on, _) = contour_reach(&world, Some(Containment::On));
    let (inside, _) = contour_reach(&world, Some(Containment::Inside));
    assert!((default - (hi.x + R)).abs() < EPS && cl0 == cl1, "Outside is the outside contour's own law");
    assert!((on - hi.x).abs() < EPS, "On ({on})");
    assert!((inside - (hi.x - R)).abs() < EPS, "Inside ({inside})");
}

#[test]
fn include_and_exclude_boundaries_combine_by_the_region_booleans() {
    let world = block_world();
    let (lo, hi) = stock_box(&world);
    let top = top_face(&world, "part", 10.0);
    // Exclude the part's top face from a stock pocket: the tool body
    // never enters the 40 x 40 square.
    let ex = AreaControls {
        extra: vec![ExtraBoundary { mode: BoundaryMode::Exclude, shape: BoundaryShape::Picks(pick("part", top)) }],
        ..Default::default()
    };
    let (t, warns) = run(&world, &op(SrcRef::Stock, pocket(8.0, 4.0), ex)).expect("excluded pocket");
    assert_eq!(warns, 0);
    let f = feeds(&t);
    assert!(!f.is_empty());
    let off_square = |p: &Point3| ((p.x.abs() - 20.0).max(0.0)).hypot((p.y.abs() - 20.0).max(0.0));
    let worst = f.iter().copied().fold((f64::INFINITY, Point3::new(0.0, 0.0, 0.0)), |b, p| if off_square(&p) < b.0 { (off_square(&p), p) } else { b });
    assert!(worst.0 >= R - 0.01, "the tool stays off the excluded square: {worst:?}");
    // Include the stock outline on a Face of the part's top: the face
    // clears the whole stock.
    let inc = AreaControls {
        extra: vec![ExtraBoundary { mode: BoundaryMode::Include, shape: BoundaryShape::Stock }],
        ..Default::default()
    };
    let (t, warns) = run(&world, &op(sel("part", top), face(10.0), inc)).expect("included face");
    assert_eq!(warns, 0, "{:?}", t.notes);
    let f = feeds(&t);
    assert!((max_by(&f, |p| p.y) + R - hi.y).abs() < EPS && (max_by(&f, |p| -p.y) + R + lo.y).abs() < EPS);
}

#[test]
fn a_boundary_that_leaves_nothing_refuses_by_name() {
    let world = world_with(None, 0.0, 10.0);
    let a = AreaControls { offset: -40.0, ..Default::default() };
    let err = run(&world, &op(SrcRef::Stock, pocket(8.0, 4.0), a)).expect_err("nothing left");
    assert!(err.contains("Boundary settings leave no area") && err.contains("boundary offset"), "{err}");
    let world = block_world();
    let top = top_face(&world, "part", 10.0);
    let a = AreaControls {
        extra: vec![ExtraBoundary { mode: BoundaryMode::Exclude, shape: BoundaryShape::Stock }],
        ..Default::default()
    };
    let err = run(&world, &op(SrcRef::Stock, pocket(8.0, 4.0), a)).expect_err("all excluded");
    assert!(err.contains("exclude boundaries"), "{err}");
    // Face cannot follow a hole: an exclude inside its band refuses.
    let a = AreaControls {
        extra: vec![ExtraBoundary { mode: BoundaryMode::Exclude, shape: BoundaryShape::Picks(pick("part", top)) }],
        ..Default::default()
    };
    let err = run(&world, &op(SrcRef::Stock, face(10.0), a)).expect_err("face over a hole");
    assert!(err.contains("an exclude boundary"), "{err}");
}

// ---- M11.5 owed (0.9.0): boundaries on a 3+2 index, the finish family ----

use camcraft_app::geometry::{Mesh, Triangle};
use camcraft_app::operation::Tilt;

/// The 40 x 30 x 10 block of the tilt suite, and its b = 90 twin by hand
/// ((x, y, z) -> (y, z, x), outside the frame's door on purpose).
fn tilt_block() -> Mesh {
    cube(Point3::new(0.0, 0.0, -20.0), Point3::new(40.0, 30.0, -10.0))
}

fn rotated_by_90(m: Mesh) -> Mesh {
    Mesh::from_facets(
        m.facets
            .iter()
            .map(|t| {
                Triangle::new(
                    Point3::new(t.a.y, t.a.z, t.a.x),
                    Point3::new(t.b.y, t.b.z, t.b.x),
                    Point3::new(t.c.y, t.c.z, t.c.x),
                )
            })
            .collect(),
    )
}

fn tilt_pocket(tilt: Option<Tilt>, top: f64) -> OpParams {
    let mut p = pocket(top, 25.0);
    if let OpParams::Pocket { tilt: t, clearance_z, retract_z, .. } = &mut p {
        *t = tilt;
        *clearance_z = Height::fixed(70.0);
        *retract_z = Some(Height::fixed(60.0));
    }
    p
}

fn tilt_world() -> World {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("part", tilt_block()));
    h.execute(&world, ImportModel::from_mesh("twin", rotated_by_90(tilt_block())));
    // The tilt suite's tower: tall enough that a b = 35 frame sections it.
    h.execute(&world, ImportModel::from_mesh("tower", cube(Point3::new(0.0, 0.0, -10.0), Point3::new(40.0, 30.0, 25.0))));
    world
}

#[test]
fn a_boundary_offset_on_a_tilted_pocket_is_the_flat_offset_pocket_rotated() {
    let world = tilt_world();
    let area = AreaControls { offset: 3.0, ..Default::default() };
    let tilt = Tilt { a_deg: 0.0, b_deg: 90.0, origin: [0.0; 3] };
    let (flat, wf) = run(&world, &op(SrcRef::Entity("twin".into()), tilt_pocket(None, 40.0), area.clone())).expect("flat");
    let (tilted, wt) = run(&world, &op(SrcRef::Entity("part".into()), tilt_pocket(Some(tilt), 40.0), area)).expect("tilted");
    assert_eq!(wf, wt);
    // The offset did something: the flat pocket reaches 3 further than the block's 30.
    assert!((max_by(&feeds(&flat), |p| p.x) - (30.0 + 3.0 - R)).abs() < EPS);
    assert_eq!(flat.moves.len(), tilted.moves.len(), "the door adds and drops no move");
    for (i, (f, t)) in flat.moves.iter().zip(tilted.moves.iter()).enumerate() {
        assert_eq!(f.kind, t.kind, "move {i}");
        for (want, got) in [(f.to, t.to), (f.from, t.from)] {
            let want = Point3::new(want.z, want.x, want.y);
            assert!(
                (want.x - got.x).abs() < 1e-6 && (want.y - got.y).abs() < 1e-6 && (want.z - got.z).abs() < 1e-6,
                "move {i}: {want:?} vs {got:?}"
            );
        }
    }
    // The preview rides the same law (b = 90 stands edge-on to world
    // XY, so it is read at b = 35): the +3 offset grows the shadow's
    // bounds by 3 in the plane, which projects to between 3 cos 35 and
    // 3 on each side in the world.
    let t35 = Tilt { a_deg: 0.0, b_deg: 35.0, origin: [0.0; 3] };
    let bounds = |offset: f64| {
        operation::region_of(&world, &op(SrcRef::Entity("tower".into()), tilt_pocket(Some(t35), 35.0), AreaControls { offset, ..Default::default() }))
            .expect("tilted preview")
            .bounds()
            .expect("bounds")
    };
    let ((l0, h0), (l3, h3)) = (bounds(0.0), bounds(3.0));
    let c = 3.0 * 35f64.to_radians().cos();
    for g in [l0.x - l3.x, l0.y - l3.y, h3.x - h0.x, h3.y - h0.y] {
        assert!(g >= c - 0.01 && g <= 3.01, "the shadow grows by the projected offset ({g})");
    }
}

/// M13.10, the tilted Face band preview: under b = 35 the band is
/// built in the frame (the rows' own overrun of two radii along the
/// frame's local X, `u`) and projected out, so the overrun past the
/// shadow lands where `u` points in the world - not two radii along
/// world X, which a band laid on the world shadow would draw.
#[test]
fn a_tilted_face_band_is_the_local_band_projected_through_the_frame() {
    let world = tilt_world();
    let t35 = Tilt { a_deg: 0.0, b_deg: 35.0, origin: [0.0; 3] };
    let u = t35.frame().expect("frame").u;
    let mut p = face(30.0);
    if let OpParams::Face { tilt, clearance_z, .. } = &mut p {
        *tilt = Some(t35);
        *clearance_z = Height::fixed(70.0);
    }
    let o = op(SrcRef::Entity("tower".into()), p, AreaControls::default());
    let (rl, rh) = operation::region_of(&world, &o).expect("tilted shadow").bounds().expect("bounds");
    let (bl, bh) = operation::machined_footprint(&world, &o).expect("tilted band").bounds().expect("bounds");
    let (wx, wy) = (2.0 * R * u[0].abs(), 2.0 * R * u[1].abs());
    assert!(wx < 2.0 * R - 0.5 || wy > 0.5, "the tilt turns the rows off world X: u = {u:?}");
    for (g, want) in [(rl.x - bl.x, wx), (bh.x - rh.x, wx), (rl.y - bl.y, wy), (bh.y - rh.y, wy)] {
        assert!((g - want).abs() < 0.05, "the overrun projects through the tilt: {g} vs {want} ({bl:?} {bh:?})");
    }
}

#[test]
fn a_tilt_with_a_non_finite_angle_still_refuses_by_name_under_a_boundary() {
    let world = tilt_world();
    let bad = Tilt { a_deg: f64::NAN, b_deg: 0.0, origin: [0.0; 3] };
    let o = op(SrcRef::Entity("part".into()), tilt_pocket(Some(bad), 40.0), AreaControls { offset: 3.0, ..Default::default() });
    let err = run(&world, &o).expect_err("no direction");
    assert!(err.contains("commands no direction"), "{err}");
    let why = operation::area_refusal(&world, &o).expect("the page's refusal");
    assert!(why.contains("commands no direction"), "{why}");
    assert!(operation::region_of(&world, &o).is_none(), "the preview draws nothing it cannot cut");
}

fn finish_parallel() -> OpParams {
    OpParams::Finish3D {
        pattern: camcraft_app::finishing3d::FinishPattern::Parallel,
        stepover: Some(2.0),
        tolerance: 0.05,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(0.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
    }
}

#[test]
fn the_finish_family_clips_its_path_to_the_boundary() {
    let world = block_world();
    let mut h = History::default();
    let ball = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("ball");
    h.execute(&world, AddTool::new(&world, "BN-0600", ball).expect("tool"));
    let r = 3.0;
    let mut o = op(SrcRef::Entity("part".into()), finish_parallel(), AreaControls::default());
    o.tool = "BN-0600".into();
    let (whole, _) = run(&world, &o).expect("the whole-model finish");
    let reach = max_by(&feeds(&whole), |p| p.x.abs().max(p.y.abs()));
    assert!(reach > 20.0 - 1e-6, "natively the ball reaches the block's walls ({reach})");
    // Inside: the tool's edge stays inside the silhouette - its centre a radius in.
    o.area = AreaControls { containment: Some(Containment::Inside), ..Default::default() };
    let (clipped, _) = run(&world, &o).expect("the clipped finish");
    let f = feeds(&clipped);
    assert!(!f.is_empty());
    let worst = max_by(&f, |p| p.x.abs().max(p.y.abs()));
    assert!(worst <= 20.0 - r + 1e-6, "every cut keeps the tool inside the boundary ({worst})");
    assert!(clipped.moves.len() < whole.moves.len() * 3, "the relink is not a move storm");
    // A shrink that leaves nothing refuses by name.
    o.area = AreaControls { offset: -30.0, ..Default::default() };
    let err = run(&world, &o).expect_err("nothing left");
    assert!(err.contains("Boundary settings leave no area"), "{err}");
}

/// M13.10, the clipped-entry ramp: the 40 x 40 x 10 slab in a stock 2
/// taller, so every clipped row re-enters 2 mm into stock at its start.
/// A non-centre-cutting ball ramps each re-entry along the row
/// (the guard's `entry_ramp` retry reaches the clip) - no plunge into
/// solid, no gouge, the note says the entries ramp; a piece too small
/// for the ramp (a 5 degree ramp on an 8 mm centre area) refuses by
/// name, as before.
#[test]
fn a_clipped_finish_with_a_non_centre_cutting_ball_ramps_its_re_entries_or_refuses_by_name() {
    use camcraft_app::tools::Tool;
    use camcraft_app::verify::Warning;
    use camcraft_motion::approach::{CENTER_CUT_REFUSAL, NOT_CENTER_CUTTING};
    use camcraft_motion::plunge_law::{Solid, first_solid_plunge};
    let world = world_with(
        Some(ImportModel::from_mesh("part", cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0)))),
        0.0,
        12.0,
    );
    let mut h = History::default();
    let ball = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("ball");
    h.execute(&world, AddTool::new(&world, "NC", Tool { center_cutting: false, ..ball }).expect("tool"));
    let area = AreaControls { containment: Some(Containment::Inside), ..Default::default() };
    let mut o = op(SrcRef::Entity("part".into()), finish_parallel(), area);
    o.tool = "NC".into();
    let (t, warnings) = operation::generate(&world, &o, "").expect("the clipped finish ramps in");
    let solid = Solid { top: 12.0, footprint: None };
    assert_eq!(first_solid_plunge(&t, 3.0, &solid), None, "no re-entry plunges into the stock: {:?}", t.notes);
    assert!(t.notes.iter().any(|n| n.starts_with(NOT_CENTER_CUTTING)), "{:?}", t.notes);
    let ramps = t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.to.z < m.from.z - 1e-9).count();
    assert!(ramps > 1, "the re-entries ramp: {ramps}");
    assert!(!warnings.iter().any(|w| matches!(w, Warning::Gouge { .. })), "{warnings:?}");
    // Too small a piece for the ramp: the named refusal stands.
    o.area = AreaControls { offset: -19.0, ..Default::default() };
    if let OpParams::Finish3D { ramp_angle, .. } = &mut o.params {
        *ramp_angle = 5.0;
    }
    let err = run(&world, &o).expect_err("no ramp fits");
    assert!(err.contains(CENTER_CUT_REFUSAL) && err.contains("even with ramped entries"), "{err}");
}

#[test]
fn every_kind_without_an_area_refuses_with_its_own_reason() {
    use camcraft_app::operation::{area_applies, area_unsupported};
    for k in [OpType::Slot, OpType::Deburr, OpType::Chamfer2D, OpType::Drill, OpType::Facing, OpType::Swarf] {
        let why = area_unsupported(k).expect("a reason");
        assert!(why.contains(&k.to_string()), "{k}: {why}");
        assert!(!area_applies(k));
    }
    for k in [OpType::Face, OpType::Pocket, OpType::Finish3D, OpType::Pocket3D, OpType::Trace, OpType::Engrave] {
        assert!(area_applies(k) && area_unsupported(k).is_none());
    }
}

fn trace_params() -> OpParams {
    OpParams::Trace {
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(5.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        tilt: None,
    }
}

/// M13.10, the `ToolOn` native: a Trace rides the block's outline with
/// its centre ON it (reach 20); containment Inside / On / Outside moves
/// the ride by a radius in / not at all / a radius out, the offset
/// moves it by its own amount, and an open boundary or an extension
/// refuses by name. Default controls are the native ride, move for move.
#[test]
fn trace_containment_moves_the_ride_about_its_own_chain() {
    let world = block_world();
    let mut o = op(SrcRef::Entity("part".into()), trace_params(), AreaControls::default());
    let (native, _) = run(&world, &o).expect("the native trace");
    assert!((max_by(&feeds(&native), |p| p.x.abs().max(p.y.abs())) - 20.0).abs() < EPS);
    for (c, want) in [(Containment::Inside, 20.0 - R), (Containment::On, 20.0), (Containment::Outside, 20.0 + R)] {
        o.area = AreaControls { containment: Some(c), ..Default::default() };
        let (t, _) = run(&world, &o).expect("the contained trace");
        let reach = max_by(&feeds(&t), |p| p.x.abs().max(p.y.abs()));
        assert!((reach - want).abs() < EPS, "{c:?}: reach {reach}, want {want}");
        if c == Containment::On {
            assert_eq!(t.moves, native.moves, "On is the native ride");
        }
    }
    o.area = AreaControls { offset: 2.0, corner: CornerStyle::Sharp, ..Default::default() };
    let (t, _) = run(&world, &o).expect("the offset trace");
    assert!((max_by(&feeds(&t), |p| p.x.abs().max(p.y.abs())) - 22.0).abs() < EPS);
    o.area = AreaControls { extension: 3.0, ..Default::default() };
    let err = run(&world, &o).expect_err("no edge to extend past");
    assert!(err.contains("no exterior edge to extend past"), "{err}");
}
