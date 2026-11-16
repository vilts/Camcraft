//! #108 acceptance: the Probe operation - face / edge / outside corner /
//! bore / boss centre finding with a touch probe. Each touch is a fast
//! seek, an incremental back-off and a slow measuring re-probe; the
//! post writes the touches as G31 skip blocks (custom macro B) on the
//! generic Fanuc-style dialect and refuses the op by name on a dialect that
//! cannot probe; the stylus path is judged as protected moves; the op
//! files and undoes like every other.
//!
//! Provenance: Fanuc's published operator documentation (G31 skip,
//! custom macro B system variables) and touch-probe practice; our own fixtures.
//!
//! Gate map (M4.4): covers app::operation (probe_op, kinds), app::posting,
//! app::project (probe DTO), app::commands (create / edit / undo),
//! motion::probing, post::probe_rec, cl::touch.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, SetStock};
use camcraft_app::geometry::Point3;
use camcraft_app::model;
use camcraft_app::operation::{Height, HeightReference, OpParams, OpType, SrcRef};
use camcraft_app::post::{self, cnc, engine};
use camcraft_app::probing::{probe_cycle, ProbeDir, ProbeMode, ProbeParams};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::probe_tool;
use camcraft_app::world::new_world;
use camcraft_app::{posting, project};

const FANUC: &str = include_str!("../../../assets/posts/fanuc.cps");
const HEIDENHAIN: &str = include_str!("../../../assets/posts/heidenhain.cps");
const TORMACH: &str = include_str!("../../../assets/posts/tormach.cps");

fn cycle(mode: ProbeMode, dir: ProbeDir, dir_y: ProbeDir, target: Point3, radius: f64) -> ProbeParams {
    ProbeParams {
        mode,
        dir,
        dir_y,
        target,
        radius,
        ball_diameter: 4.0,
        approach: 5.0,
        overtravel: 5.0,
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        clearance_z: 50.0,
    }
}

/// The measured trips of a path: (measure ordinal, nominal trip).
fn measures(t: &camcraft_app::cl::Toolpath) -> Vec<(u32, Point3)> {
    t.probes
        .iter()
        .filter_map(|p| p.measure.map(|k| (k, t.moves[p.index].to)))
        .collect()
}

fn close(a: Point3, b: Point3) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9 && (a.z - b.z).abs() < 1e-9
}

#[test]
fn each_mode_lays_down_its_touches() {
    let r = 2.0; // ball radius
    // Face -Z on the top at Z 20: one seek + one measure. The path
    // states the stylus TIP like every ball tool, so the trip of a -Z
    // touch is the contact itself (the ball centre stands r above it).
    let t = probe_cycle(&cycle(ProbeMode::Face, ProbeDir::ZNeg, ProbeDir::YPos, Point3::new(10.0, 5.0, 20.0), 0.0))
        .expect("face");
    assert_eq!(t.probes.len(), 2, "a seek and a measure");
    assert_eq!(measures(&t), vec![(0, Point3::new(10.0, 5.0, 20.0))]);
    assert!(t.probes.iter().all(|p| p.is_sane(&t.moves)));
    assert!(t.is_chained() && t.all_finite());
    // Every probe move is a feed; nothing the stylus does is a rapid
    // toward the part.
    assert!(t.probes.iter().all(|p| t.moves[p.index].kind == MoveKind::Feed));

    // Edge +X at the top edge X 0: the ball centre rides `depth` below
    // it, the tip a ball radius lower.
    let t = probe_cycle(&cycle(ProbeMode::Edge, ProbeDir::XPos, ProbeDir::YPos, Point3::new(0.0, 5.0, 20.0), 0.0))
        .expect("edge");
    assert_eq!(measures(&t), vec![(0, Point3::new(-r, 5.0, 17.0 - r))]);

    // Outside corner at the vertex (0, 0, 20), part toward +X +Y: an X
    // touch inset along Y and a Y touch inset along X.
    let t = probe_cycle(&cycle(ProbeMode::Corner, ProbeDir::XPos, ProbeDir::YPos, Point3::new(0.0, 0.0, 20.0), 0.0))
        .expect("corner");
    let m = measures(&t);
    assert_eq!(m.len(), 2);
    assert!(close(m[0].1, Point3::new(-r, 5.0, 17.0 - r)), "{m:?}");
    assert!(close(m[1].1, Point3::new(5.0, -r, 17.0 - r)), "{m:?}");

    // Bore of radius 10 about (30, 30), rim Z 20: four touches outward.
    let t = probe_cycle(&cycle(ProbeMode::Bore, ProbeDir::ZNeg, ProbeDir::YPos, Point3::new(30.0, 30.0, 20.0), 10.0))
        .expect("bore");
    let m = measures(&t);
    assert_eq!(m.len(), 4);
    let want = [
        Point3::new(38.0, 30.0, 17.0 - r),
        Point3::new(22.0, 30.0, 17.0 - r),
        Point3::new(30.0, 38.0, 17.0 - r),
        Point3::new(30.0, 22.0, 17.0 - r),
    ];
    for (k, w) in want.iter().enumerate() {
        assert!(close(m[k].1, *w), "bore touch {k}: {:?} != {w:?}", m[k].1);
    }
    assert_eq!(t.probes.len(), 8, "each touch seeks then measures");

    // Boss of radius 10 about (30, 30): four touches inward from outside.
    let t = probe_cycle(&cycle(ProbeMode::Boss, ProbeDir::ZNeg, ProbeDir::YPos, Point3::new(30.0, 30.0, 20.0), 10.0))
        .expect("boss");
    let m = measures(&t);
    assert!(close(m[0].1, Point3::new(42.0, 30.0, 17.0 - r)), "{m:?}");
    assert!(close(m[1].1, Point3::new(18.0, 30.0, 17.0 - r)), "{m:?}");
    // The stylus never descends inside the boss's circle.
    for mv in &t.moves {
        let d = ((mv.to.x - 30.0).powi(2) + (mv.to.y - 30.0).powi(2)).sqrt();
        assert!(mv.to.z >= 49.0 || d >= 10.0 + r - 1e-9, "inside the boss at {:?}", mv.to);
    }
}

#[test]
fn degenerate_probe_numbers_are_refused() {
    let mut p = cycle(ProbeMode::Bore, ProbeDir::ZNeg, ProbeDir::YPos, Point3::new(0.0, 0.0, 20.0), 2.5);
    assert!(probe_cycle(&p).is_err(), "a bore too small for the ball is refused");
    p.radius = 10.0;
    p.slow_feed = 900.0;
    assert!(probe_cycle(&p).is_err(), "a measure faster than the seek is refused");
    p.slow_feed = 30.0;
    p.approach = f64::NAN;
    assert!(probe_cycle(&p).is_err(), "a NaN is refused");
    let mut e = cycle(ProbeMode::Edge, ProbeDir::ZNeg, ProbeDir::YPos, Point3::new(0.0, 0.0, 20.0), 0.0);
    assert!(probe_cycle(&e).is_err(), "an edge is not probed along Z");
    e.mode = ProbeMode::Face;
    e.clearance_z = 21.0;
    assert!(probe_cycle(&e).is_err(), "a safe plane under the ball is refused");
}

/// The generic dialect's golden: a bore centre setting G55 through the
/// whole post seam (dump -> stream text -> engine -> fanuc.cps).
#[test]
fn bore_centre_posts_g31_skips_on_the_generic_dialect() {
    let t = probe_cycle(&cycle(ProbeMode::Bore, ProbeDir::ZNeg, ProbeDir::YPos, Point3::new(30.0, 30.0, 20.0), 10.0))
        .expect("bore");
    let d = post::Dump {
        op_name: "Probe1",
        tool_number: 7,
        tool_name: "PROBE-4",
        tool_diameter: 4.0,
        rpm: 0.0,
        clearance_z: 50.0,
        retract_z: 50.0,
        probe_result: Some(post::probe_rec::ProbeResult { set_wcs: Some(1), tol_mm: 0.5 }),
        ..post::Dump::default()
    };
    let recs = post::dump(&t, &d);
    assert!(!recs.iter().any(|r| matches!(r, cnc::Record::Warn { .. })), "a sane probe path carries no WARN");
    let stream = cnc::serialize(&recs);
    // The stream round-trips: what the dump says is what the engine reads.
    assert_eq!(cnc::parse(&stream).expect("parse"), recs, "TOUCH / RETREAT / FOUND round-trip");
    let out = engine::run(FANUC, &serde_json::Value::Null, &stream).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("G31 "), "the touches are skip blocks");
    assert!(!out.program.contains("G38"), "a Fanuc control takes no G38.x");
    assert!(out.program.contains("#3000=2(PROBE NO CONTACT)"), "a touch that never trips alarms");
    assert!(
        !out.program.lines().any(|l| l.split_whitespace().any(|w| w == "M3" || w == "M8")),
        "a touch probe starts no spindle and no coolant"
    );
    assert!(out.program.contains("G10 L2 P2"), "the result sets G55");
    common::write_golden("nc/fanuc/probe_bore_centre.nc", &out.program);
}

fn probe_world(top: Height) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 }, Location::default()).expect("stock"),
    );
    h.execute(&world, AddTool::new(&world, "PROBE-4", probe_tool(4.0, 30.0)).expect("tool"));
    let params = face_params(top);
    h.execute(&world, CreateOperation::new(&world, "Probe1", "PROBE-4", SrcRef::Stock, params).expect("create"));
    (world, h)
}

fn stock_top(world: &World) -> f64 {
    model::stock_world_aabb(world).map(|b| b.max.z).expect("stock box")
}

fn face_params(_top: Height) -> OpParams {
    let mut p = camcraft_app::operation::retype_params(
        &OpParams::Probe {
            clearance_z: Height::of(HeightReference::TopOfStock, 15.0),
            mode: ProbeMode::Face,
            dir: ProbeDir::ZNeg,
            dir_y: ProbeDir::YPos,
            target: [0.0; 3],
            radius: 0.0,
            approach: 5.0,
            overtravel: 5.0,
            backoff: 1.0,
            fast_feed: 300.0,
            slow_feed: 30.0,
            depth: 3.0,
            inset: 5.0,
            set_wcs: Some(0),
            tolerance: 0.5,
        },
        OpType::Probe as usize,
    );
    if let OpParams::Probe { clearance_z, .. } = &mut p {
        *clearance_z = Height::of(HeightReference::TopOfStock, 15.0);
    }
    p
}

fn set_target(world: &World, h: &mut History, z: f64) -> OpParams {
    let e = model::find_by_name(world, "Probe1").expect("op");
    let mut p = model::operation_of(world, &e).expect("operation").params;
    if let OpParams::Probe { target, .. } = &mut p {
        let b = model::stock_world_aabb(world).expect("stock");
        *target = [(b.min.x + b.max.x) * 0.5, (b.min.y + b.max.y) * 0.5, z];
    }
    h.execute(world, EditOperationParams::new(world, "Probe1", p.clone()).expect("edit"));
    p
}

fn generate(world: &World, h: &mut History) -> Vec<camcraft_app::verify::Warning> {
    h.execute(world, GenerateOperation::new(world, "Probe1").expect("generate"));
    let e = model::find_by_name(world, "Probe1").expect("op");
    model::toolpath_of(world, &e).expect("cached").warnings
}

#[test]
fn a_face_touch_on_the_stock_top_generates_clean_and_cuts_nothing() {
    let (world, mut h) = probe_world(Height::of(HeightReference::TopOfStock, 0.0));
    let top = stock_top(&world);
    set_target(&world, &mut h, top);
    let warns = generate(&world, &mut h);
    assert!(warns.is_empty(), "a tangent touch is clean: {warns:?}");
    // The probe op removes no stock: the program plan skips it.
    let report = posting::verification_report(&world);
    assert!(report.problems.is_empty(), "{:?}", report.problems);
}

#[test]
fn a_stylus_driven_through_material_is_a_gouge() {
    // The expected contact 4 mm under the stock top: the seek runs the
    // ball through the block before it - the protected-move law convicts.
    let (world, mut h) = probe_world(Height::of(HeightReference::TopOfStock, 0.0));
    let top = stock_top(&world);
    set_target(&world, &mut h, top - 4.0);
    let warns = generate(&world, &mut h);
    assert!(
        warns.iter().any(|w| matches!(w, camcraft_app::verify::Warning::Gouge { .. })),
        "the stylus through material is refused: {warns:?}"
    );
}

#[test]
fn a_dialect_without_probing_refuses_the_op_by_name() {
    let (world, mut h) = probe_world(Height::of(HeightReference::TopOfStock, 0.0));
    let top = stock_top(&world);
    set_target(&world, &mut h, top);
    generate(&world, &mut h);
    let path = std::env::temp_dir().join(format!("camcraft_probe_refuse_{}.nc", std::process::id()));
    let err = posting::post_script(&world, &path, HEIDENHAIN).expect_err("Heidenhain cannot probe here");
    assert!(err.contains("onProbeMove") && err.contains("Probe1"), "refused by name: {err}");
    assert!(!path.exists(), "nothing is written");
    // The generic dialect posts the same document.
    let ok = posting::post_script(&world, &path, FANUC).expect("fanuc probes");
    assert!(ok.program.contains("G31 "));
    let _ = std::fs::remove_file(&path);
}

/// #362: the Tormach PathPilot post probes too (G38.2), so the same
/// document posts on it instead of being refused; no spindle and no
/// coolant on the default machine's flood entry.
#[test]
fn the_tormach_post_probes_the_op_with_g38_2() {
    let (world, mut h) = probe_world(Height::of(HeightReference::TopOfStock, 0.0));
    let top = stock_top(&world);
    set_target(&world, &mut h, top);
    generate(&world, &mut h);
    let path = std::env::temp_dir().join(format!("camcraft_probe_tormach_{}.nc", std::process::id()));
    let ok = posting::post_script(&world, &path, TORMACH).expect("tormach probes");
    assert!(ok.program.contains("G38.2 "), "{}", ok.program);
    assert!(ok.program.contains("G10 L20 P1 Z[#5422-#190]"), "the result sets G54 Z: {}", ok.program);
    assert!(
        !ok.program.lines().any(|l| l.split_whitespace().any(|w| w == "M3" || w == "M8")),
        "a touch probe starts no spindle and no coolant: {}",
        ok.program
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_probe_round_trips_through_the_project_file() {
    let (world, mut h) = probe_world(Height::of(HeightReference::TopOfStock, 0.0));
    let top = stock_top(&world);
    let params = set_target(&world, &mut h, top);
    generate(&world, &mut h);
    let path = std::env::temp_dir().join(format!("camcraft_probe_rt_{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("\"probe\""), "the params carry their tag");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    let e = model::find_by_name(&world2, "Probe1").expect("op");
    assert_eq!(model::operation_of(&world2, &e).expect("op").params, params);
    let tool = model::find_by_name(&world2, "PROBE-4").and_then(|t| model::tool_of(&world2, &t)).expect("tool");
    assert_eq!(tool, probe_tool(4.0, 30.0), "the probe tool files as a probe");
    // The cached touches survive the file: a touch lost on load would
    // post as a feed.
    let a = model::toolpath_of(&world, &model::find_by_name(&world, "Probe1").expect("op")).expect("cache");
    let b = model::toolpath_of(&world2, &e).expect("cache");
    assert_eq!(a.path.probes, b.path.probes);
    assert!(!b.path.probes.is_empty());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_probe_param_edit_undoes_and_redoes() {
    let (world, mut h) = probe_world(Height::of(HeightReference::TopOfStock, 0.0));
    let e = model::find_by_name(&world, "Probe1").expect("op");
    let before = model::operation_of(&world, &e).expect("op").params;
    let mut edited = before.clone();
    if let OpParams::Probe { tolerance, set_wcs, .. } = &mut edited {
        *tolerance = 0.05;
        *set_wcs = None;
    }
    h.execute(&world, EditOperationParams::new(&world, "Probe1", edited.clone()).expect("edit"));
    assert_eq!(model::operation_of(&world, &e).expect("op").params, edited);
    assert!(h.undo(&world));
    assert_eq!(model::operation_of(&world, &e).expect("op").params, before, "undo restores the old numbers");
    assert!(h.redo(&world));
    assert_eq!(model::operation_of(&world, &e).expect("op").params, edited);
}

#[test]
fn the_probe_row_appends_and_lists_in_drilling() {
    use camcraft_app::operation::{KindInfo, Section};
    // Appended: stored kind ids never move; the menu lists it under
    // Drilling by display order.
    assert_eq!(KindInfo::ALL[38].ty, OpType::Probe);
    // #670 appended the plasma profile behind it - the probe's id held.
    assert_eq!(KindInfo::ALL[39].ty, OpType::PlasmaProfile);
    assert_eq!(KindInfo::ALL[13].ty, OpType::Pocket3D);
    assert_eq!(OpType::Probe.info().section, Section::Drilling);
    assert_eq!(OpType::Probe.info().tool_kinds, &[camcraft_app::tools::ToolKind::Probe]);
}

#[test]
fn every_older_kind_keeps_its_stored_id_and_digest_tag() {
    // The kind order a pre-#108 file was written under. The stamp digest
    // reads only `tag` for the kind, and pins and the type combo read the
    // id: both unchanged means an older project loads with no op going
    // Stale and no pin moving - no format rung.
    use camcraft_app::operation::KindInfo;
    use OpType::*;
    let before = [
        Face, Contour, Pocket, Slot, Trace, Adaptive2D, Offset2D, Engrave, Chamfer2D, Circular, Thread,
        Drill, Bore, Pocket3D, Finish3D, Deburr, Contour5D, Parallel5D, Swarf, AdvancedSwarf,
        RotaryContour, RotaryParallel, RotaryOffset, MultiAxisRough, Flow5D, MultiAxisContour, Facing,
        ProfileRough, ProfileFinish, GrooveRough, GrooveFinish, Part, Threading, Chamfering,
        TaperTurning, MultiPassTurning, FormTurning, BoringTurning,
    ];
    for (i, ty) in before.iter().enumerate() {
        assert_eq!(KindInfo::ALL[i].ty, *ty, "id {i} moved");
        assert_eq!(usize::from(ty.info().tag), i, "{ty:?}'s digest tag moved");
    }
    // No PROJECT_VERSION pin here (#344): the loop above is what proves
    // the probe needed no format rung, and a hard-coded version number
    // went stale on every unrelated rung (#196's v51, #329's v52).
}

/// The posted Z of every G31 for a probe whose length register is
/// gauged to the ball's bottom, like a ball mill's (review of 0beeec4):
/// the program states the TIP. A -Z face touch at Z 20 expects its trip
/// at Z 20 and skips toward Z 15 (overtravel 5); a bore touched with
/// the ball centre 3 mm under the rim (Z 17) runs its tip at Z 15.
#[test]
fn the_posted_g31_z_is_the_tip_at_the_expected_contact() {
    let g31_z = |t: &camcraft_app::cl::Toolpath, result: bool| -> (Vec<String>, String) {
        let d = post::Dump {
            op_name: "Probe1",
            tool_number: 7,
            tool_name: "PROBE-4",
            tool_diameter: 4.0,
            clearance_z: 50.0,
            retract_z: 50.0,
            probe_result: result.then_some(post::probe_rec::ProbeResult { set_wcs: Some(0), tol_mm: 0.5 }),
            ..post::Dump::default()
        };
        let stream = cnc::serialize(&post::dump(t, &d));
        let out = engine::run(FANUC, &serde_json::Value::Null, &stream).expect("post");
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
        // The touches only: a protected positioning move (#174) is a G31
        // too, told apart by its contact alarm on the next line.
        let lines: Vec<&str> = out.program.lines().collect();
        let zs = lines
            .windows(2)
            .filter(|w| w[0].starts_with("G31 ") && w[1].contains("NO CONTACT"))
            .filter_map(|w| w[0].split_whitespace().find(|t| t.starts_with('Z')).map(str::to_string))
            .collect();
        (zs, out.program)
    };
    let face = probe_cycle(&cycle(ProbeMode::Face, ProbeDir::ZNeg, ProbeDir::YPos, Point3::new(10.0, 5.0, 20.0), 0.0))
        .expect("face");
    assert_eq!(measures(&face), vec![(0, Point3::new(10.0, 5.0, 20.0))], "the -Z trip is the face");
    let (zs, program) = g31_z(&face, true);
    assert_eq!(zs, vec!["Z15.", "Z15."], "seek and measure skip toward the face less the overtravel");
    // The Z deviation is the measured trip against the face itself, so a
    // part exactly where the model says sets the offset by zero.
    assert!(program.lines().any(|l| l.starts_with("#19") && l.ends_with("-20")), "{program}");

    let bore = probe_cycle(&cycle(ProbeMode::Bore, ProbeDir::ZNeg, ProbeDir::YPos, Point3::new(30.0, 30.0, 20.0), 10.0))
        .expect("bore");
    let (zs, _) = g31_z(&bore, true);
    assert_eq!(zs.len(), 8, "four touches, each a seek and a measure");
    assert!(zs.iter().all(|z| z == "Z15."), "the tip rides r under the ball centre's Z 17: {zs:?}");
}
