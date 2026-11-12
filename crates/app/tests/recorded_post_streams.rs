//! The staleness guard of the two recorded post inputs motion generates
//! (#926): the bore-centre probe (`motion::probing::probe_cycle`) and the
//! peck-drill cycle with a retract change (`motion::drilling::drill`),
//! each dumped through `post::dump` and compared byte for byte with its
//! file in `crates/post/tests/recorded/streams/`. The post's script suites
//! read those files and never generate; this suite is what fails when the
//! generator moves, so a recorded input cannot go stale silently. It lives
//! in app because app is the one crate that sees both motion and post (no
//! new crate edge, #919 Q4). The cl-built streams are guarded in post
//! (`crates/post/tests/recorded_streams.rs`).
//!
//! Regenerate after an intentional generator change, then re-run the post
//! script suites and read their golden diffs:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test recorded_post_streams`
//!
//! Gate map (M4.4): covers motion::probing, motion::drilling, post::dump,
//! post::cnc.

use camcraft_app::drilling::{drill, CycleChoice, CycleParams, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::Point3;
use camcraft_app::post::{self, cnc, Dump};
use camcraft_app::probing::{probe_cycle, ProbeDir, ProbeMode, ProbeParams};
use camcraft_app::tools::{Tool, ToolKind};

fn recorded(case: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../post/tests/recorded/streams")
        .join(format!("{case}.cnc"))
}

fn guard(case: &str, text: &str) {
    let path = recorded(case);
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(&path, text).expect("write the recorded stream");
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        want.replace("\r\n", "\n"),
        text,
        "recorded stream {} is stale: its generator writes something else now - regenerate with WRITE_GOLDEN=1, then re-read every script golden it feeds",
        path.display()
    );
}

/// The bore centre of tests/probe_dialects.rs: a 20 mm bore at (30, 30),
/// rim Z 20, four touches (seek + measure each); the result sets G55.
fn probe() -> String {
    let p = ProbeParams {
        mode: ProbeMode::Bore,
        dir: ProbeDir::ZNeg,
        dir_y: ProbeDir::YPos,
        target: Point3::new(30.0, 30.0, 20.0),
        radius: 10.0,
        ball_diameter: 4.0,
        approach: 5.0,
        overtravel: 5.0,
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        clearance_z: 50.0,
    };
    let t = probe_cycle(&p).expect("probe cycle");
    let d = Dump {
        op_name: "Probe1",
        tool_number: 7,
        tool_name: "PROBE-4",
        tool_diameter: 4.0,
        rpm: 0.0,
        clearance_z: 50.0,
        retract_z: 50.0,
        wcs: "G54",
        probe_result: Some(post::probe_rec::ProbeResult { set_wcs: Some(1), tol_mm: 0.5 }),
        ..Dump::default()
    };
    let recs = post::dump(&t, &d);
    assert!(!recs.iter().any(|r| matches!(r, cnc::Record::Warn { .. })), "a sane probe path carries no WARN");
    cnc::serialize(&recs)
}

/// Two pecked holes with a retract change: the cycle's R plane is 1 mm
/// over the part, the hop between holes 3 mm, the final retract the
/// 10 mm clearance plane.
fn drill_retract() -> String {
    let tool = Tool {
        kind: ToolKind::Drill,
        diameter: 6.0,
        corner_r: 0.0,
        tip_angle: 118.0,
        pitch: 0.0,
        nose_radius: 0.0,
        width: 0.0,
        flute_len: 40.0,
        shank_d: 6.0,
        shank_len: 30.0,
        holder_d: 20.0,
        holder_len: 40.0,
        material_group: String::from("steel"),
        center_cutting: true,
        rpm: 1500.0,
        feed: 200.0,
        tool_material: None,
        flutes: None,
        orientation: None,
        turning: None,
    };
    let hole = |x: f64, floor_z: f64| Hole {
        xy: Point2::new(x, 10.0),
        radius: 3.0,
        entry_z: 0.0,
        floor_z,
        through: false,
        stepped: false,
    };
    let holes = [hole(10.0, -20.0), hole(30.0, -15.0)];
    let p = CycleParams {
        clearance_z: 10.0,
        r_z: 1.0,
        hop_z: Some(3.0),
        bottom_z: None,
        cycle: CycleChoice::Peck,
        peck: 4.0,
        dwell: 0.0,
        feed: 200.0,
        plunge_feed: 60.0,
        tool: &tool,
        holes: &holes,
    };
    let t = drill(&p).expect("drill cycle");
    assert_eq!(t.cycles.len(), 2, "one cycle record per hole");
    let d = Dump {
        op_name: "Peck",
        tool_number: 3,
        tool_name: "DRILL-6",
        tool_diameter: 6.0,
        rpm: 1500.0,
        clearance_z: 10.0,
        retract_z: 3.0,
        top_z: 0.0,
        wcs: "G54",
        ..Dump::default()
    };
    cnc::serialize(&post::dump(&t, &d))
}

#[test]
fn probe_stream_is_current() {
    guard("probe", &probe());
}

#[test]
fn drill_retract_stream_is_current() {
    guard("drill_retract", &drill_retract());
}
