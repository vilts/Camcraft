//! #760: a Z-rotated work offset (yaw, WCS +Z aligned with machine +Z)
//! is carried end-to-end into directional toolpath generation, staleness,
//! posting (Bake by default, G68/ROT when Control is chosen and declared),
//! machine travel checks and setup sheet text.
//!
//! Provenance: our own fixtures over rotated-work-offset milling practice;
//! no vendor material consulted.
//!
//! Gate map (#760): covers app::operation (stamp, generate/run_job, wcs_yaw),
//! app::posting (wcs_rotation, stream_records, post_setup), post::wcs_rotate,
//! app::machine, app::setup_sheet.
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::*;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, LoadMachine, SetStock,
    SetWcsOrigin,
};
use camcraft_app::geometry::{IDENTITY_Q, Point3, quat_from_euler};
use camcraft_app::machine::{AxisLimits, Machine, check_document};
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, setup_sheet};

fn face_params(stepover: f64, cut_z: f64, clear_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover,
        clearance_z: Height::fixed(clear_z),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

fn box_world(dx: f64, dy: f64, dz: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx, dy, dz }, Default::default()).expect("stock"),
    );
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .map(|(_, t)| t)
        .expect("catalog tool");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("add tool"));
    (world, h)
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-760-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

fn parse_xy_moves(program: &str) -> Vec<(Option<f64>, Option<f64>)> {
    let mut cur_x = None;
    let mut cur_y = None;
    let mut g1 = false;
    let mut pts = Vec::new();
    for line in program.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let mut x = None;
        let mut y = None;
        for w in &words {
            if let Some(rest) = w.strip_prefix('G') {
                if rest == "01" || rest == "1" {
                    g1 = true;
                } else if rest == "00" || rest == "0" {
                    g1 = false;
                }
            } else if let Some(rest) = w.strip_prefix('X') {
                if let Ok(v) = rest.parse::<f64>() {
                    x = Some(v);
                }
            } else if let Some(rest) = w.strip_prefix('Y')
                && let Ok(v) = rest.parse::<f64>()
            {
                y = Some(v);
            }
        }
        if let Some(nx) = x {
            cur_x = Some(nx);
        }
        if let Some(ny) = y {
            cur_y = Some(ny);
        }
        if g1 && (x.is_some() || y.is_some()) {
            pts.push((cur_x, cur_y));
        }
    }
    pts
}

#[test]
fn face_passes_and_bake_post_follow_90_deg_z_rotated_wcs() {
    let (world, mut h) = box_world(60.0, 20.0, 20.0);
    let b = model::stock_world_aabb_for(&world, None).expect("stock box");
    let origin = Point3::new(b.min.x, b.min.y, b.max.z);
    let rot = quat_from_euler(0.0, 0.0, 90f64.to_radians());
    h.execute(&world, SetWcsOrigin::new(&world, Location { pos: origin, rot }).expect("place G54"));
    h.execute(
        &world,
        CreateOperation::new(
            &world,
            "Face1",
            "EM-0400-2FL",
            SrcRef::Stock,
            face_params(4.0, b.max.z - 0.5, b.max.z + 5.0),
        )
        .expect("create op"),
    );
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));

    let op_e = model::find_by_name(&world, "Face1").expect("Face1");
    let cache = model::toolpath_of(&world, &op_e).expect("toolpath");
    let long_sweeps: Vec<_> = cache
        .path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.arc.is_none())
        .map(|m| ((m.to.x - m.from.x).abs(), (m.to.y - m.from.y).abs()))
        .filter(|&(dx, dy)| dx.hypot(dy) > 10.0)
        .collect();
    assert!(!long_sweeps.is_empty(), "expected long facing passes");
    for (dx, dy) in &long_sweeps {
        assert!(
            *dy > 10.0 && *dx < 1e-3,
            "90-deg Z-rotated WCS +X aligns with Setup +Y; long sweeps must move along Y, got dx={dx}, dy={dy}"
        );
    }

    let path = tmp_nc("yaw90");
    let posted = posting::post_setup(&world, &path).expect("Z-rotated WCS must post in default Bake mode");
    let _ = std::fs::remove_file(&path);
    assert!(!posted.program.contains("G68"), "default Bake mode must not emit G68:\n{}", posted.program);

    // In default Bake mode (translation-only G54), G-code bakes the 90-deg WCS-aligned
    // passes into Setup/Table X/Y coordinates (sweeping along Y) covering the stock box.
    let pts = parse_xy_moves(&posted.program);
    assert!(!pts.is_empty(), "expected posted XY feed moves:\n{}", posted.program);
    let mut y_spans = 0usize;
    let mut prev: Option<(f64, f64)> = None;
    for (ox, oy) in pts {
        if let (Some(xt), Some(yt)) = (ox, oy) {
            let sx = origin.x + xt;
            let sy = origin.y + yt;
            assert!(
                sx >= b.min.x - 5.0 && sx <= b.max.x + 5.0 && sy >= b.min.y - 5.0 && sy <= b.max.y + 5.0,
                "baked table move ({xt}, {yt}) -> Setup ({sx}, {sy}) outside stock {b:?}"
            );
            if let Some((px, py)) = prev
                && (yt - py).abs() > 10.0
                && (xt - px).abs() < 1e-3
            {
                y_spans += 1;
            }
            prev = Some((xt, yt));
        }
    }
    assert!(y_spans > 0, "baked 90-deg WCS facing sweeps must move along table Y:\n{}", posted.program);
}

#[test]
fn face_passes_and_bake_post_align_to_30_deg_yaw() {
    let (world, mut h) = box_world(60.0, 60.0, 20.0);
    let b = model::stock_world_aabb_for(&world, None).expect("stock box");
    let origin = Point3::new(10.0, 5.0, b.max.z);
    let yaw = 30f64.to_radians();
    h.execute(
        &world,
        SetWcsOrigin::new(&world, Location { pos: origin, rot: quat_from_euler(0.0, 0.0, yaw) }).expect("G54"),
    );
    h.execute(
        &world,
        CreateOperation::new(
            &world,
            "Face1",
            "EM-0400-2FL",
            SrcRef::Stock,
            face_params(4.0, b.max.z - 0.5, b.max.z + 5.0),
        )
        .expect("create"),
    );
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));

    let op_e = model::find_by_name(&world, "Face1").expect("Face1");
    let cache = model::toolpath_of(&world, &op_e).expect("toolpath");
    let long_sweeps: Vec<_> = cache
        .path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.arc.is_none())
        .filter_map(|m| {
            let dx = m.to.x - m.from.x;
            let dy = m.to.y - m.from.y;
            (dx.hypot(dy) > 25.0).then_some((dx, dy))
        })
        .collect();
    assert!(!long_sweeps.is_empty(), "expected long 30-deg sweeps");
    for (dx, dy) in long_sweeps {
        let dir_dot = (dx * yaw.cos() + dy * yaw.sin()).abs() / dx.hypot(dy);
        assert!(
            dir_dot > 0.9999,
            "sweep ({dx}, {dy}) in Setup is not parallel to 30-deg WCS X (dot={dir_dot})"
        );
    }

    let path = tmp_nc("yaw30");
    let posted = posting::post_setup(&world, &path).expect("30-deg Z-rotated WCS posts");
    let _ = std::fs::remove_file(&path);
    // In Bake mode (no G68), table moves sweep along the 30-deg WCS X direction (dy/dx = tan 30).
    let mut prev: Option<(f64, f64)> = None;
    let mut angled_sweeps = 0usize;
    for (ox, oy) in parse_xy_moves(&posted.program) {
        if let (Some(x), Some(y)) = (ox, oy) {
            if let Some((px, py)) = prev {
                let (dx, dy) = (x - px, y - py);
                let len = dx.hypot(dy);
                if len > 25.0 {
                    let dot = (dx * yaw.cos() + dy * yaw.sin()).abs() / len;
                    if dot > 0.9999 {
                        angled_sweeps += 1;
                    }
                }
            }
            prev = Some((x, y));
        }
    }
    assert!(
        angled_sweeps > 0,
        "baked 30-deg WCS program should sweep along 30-deg table diagonal:\n{}",
        posted.program
    );
}

#[test]
fn changing_wcs_yaw_marks_generated_op_stale() {
    let (world, mut h) = box_world(60.0, 20.0, 20.0);
    let b = model::stock_world_aabb_for(&world, None).expect("stock");
    let origin = Point3::new(0.0, 0.0, b.max.z);
    h.execute(
        &world,
        SetWcsOrigin::new(&world, Location { pos: origin, rot: IDENTITY_Q }).expect("square"),
    );
    h.execute(
        &world,
        CreateOperation::new(
            &world,
            "Face1",
            "EM-0400-2FL",
            SrcRef::Stock,
            face_params(4.0, b.max.z - 0.5, b.max.z + 5.0),
        )
        .expect("create"),
    );
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    let op_e = model::find_by_name(&world, "Face1").expect("Face1");
    assert_eq!(operation::status_of(&world, &op_e), OpStatus::Ok);

    h.execute(
        &world,
        SetWcsOrigin::new(
            &world,
            Location { pos: origin, rot: quat_from_euler(0.0, 0.0, 90f64.to_radians()) },
        )
        .expect("rotate yaw"),
    );
    assert_eq!(
        operation::status_of(&world, &op_e),
        OpStatus::Stale,
        "rotating WCS about Z must mark the generated operation Stale"
    );
}

#[test]
fn machine_travel_limits_check_physical_table_coordinates_under_rotated_wcs() {
    let (world, mut h) = box_world(240.0, 40.0, 20.0);
    let b = model::stock_world_aabb_for(&world, None).expect("stock");
    let center = Point3::new((b.min.x + b.max.x) * 0.5, (b.min.y + b.max.y) * 0.5, b.max.z);
    let m = Machine {
        model: "AsymmetricMill".into(),
        vendor: "Test".into(),
        axes: vec![
            AxisLimits { name: "X".into(), travel: Some((-250.0, 250.0)), ..Default::default() },
            AxisLimits { name: "Y".into(), travel: Some((-80.0, 80.0)), ..Default::default() },
            AxisLimits { name: "Z".into(), travel: Some((-200.0, 200.0)), ..Default::default() },
        ],
        spindles: Some(1),
        ..Default::default()
    };
    h.execute(&world, LoadMachine::new(&world, "asym".into(), Some(m.clone())).expect("machine"));
    h.execute(
        &world,
        SetWcsOrigin::new(
            &world,
            Location { pos: center, rot: quat_from_euler(0.0, 0.0, 90f64.to_radians()) },
        )
        .expect("yaw 90"),
    );
    h.execute(
        &world,
        CreateOperation::new(
            &world,
            "Face1",
            "EM-0400-2FL",
            SrcRef::Stock,
            face_params(4.0, b.max.z - 0.5, b.max.z + 5.0),
        )
        .expect("create"),
    );
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("gen"));
    assert!(
        check_document(&world, &m).expect("check").problems.is_empty(),
        "90-deg WCS turns pass orientation without turning table stock; physical X=+-120 Y=+-20 stays inside X=+-250 Y=+-80"
    );

    // Shifting G54 origin -200mm in X pushes table travel rel to G54 past +250mm X.
    h.execute(
        &world,
        SetWcsOrigin::new(
            &world,
            Location {
                pos: Point3::new(center.x - 140.0, center.y, center.z),
                rot: quat_from_euler(0.0, 0.0, 90f64.to_radians()),
            },
        )
        .expect("shift origin"),
    );
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("regen"));
    let chk = check_document(&world, &m).expect("check");
    assert!(
        chk.problems.iter().any(|p| p.contains("in X") && p.contains("stroke")),
        "shifted origin + 240mm bar exceeds X stroke in table coordinates: {chk:?}"
    );
}

#[test]
fn setup_sheet_states_z_rotation_without_refusing_while_tilt_still_refuses() {
    let (world, mut h) = box_world(40.0, 40.0, 20.0);
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    h.execute(
        &world,
        SetWcsOrigin::new(
            &world,
            Location {
                pos: Point3::new(0.0, 0.0, 20.0),
                rot: quat_from_euler(0.0, 0.0, 90f64.to_radians()),
            },
        )
        .expect("yaw 90"),
    );
    let sheet_yaw = setup_sheet::render(&world, Some("Setup1")).expect("sheet");
    assert!(sheet_yaw.contains("90.000 deg about Z"), "{sheet_yaw}");
    assert!(
        !sheet_yaw.contains("post refuses"),
        "Z-rotated milling offset is now supported and must not claim refusal:\n{sheet_yaw}"
    );

    h.execute(
        &world,
        SetWcsOrigin::new(
            &world,
            Location {
                pos: Point3::new(0.0, 0.0, 20.0),
                rot: quat_from_euler(30f64.to_radians(), 0.0, 0.0),
            },
        )
        .expect("tilt 30 X"),
    );
    let sheet_tilt = setup_sheet::render(&world, Some("Setup1")).expect("sheet");
    assert!(
        sheet_tilt.contains("the post refuses"),
        "Z-tilted offset must still warn on the setup sheet:\n{sheet_tilt}"
    );
}
