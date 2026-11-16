//! The staleness guard of the recorded post inputs motion generates
//! (#926): the bore-centre probe (`motion::probing::probe_cycle`) and the
//! peck-drill cycle with a retract change (`motion::drilling::drill`),
//! each dumped through `post::dump` and compared byte for byte with its
//! file in `crates/post/tests/recorded/streams/`; and (slice 2) every probe
//! stream and probe path the post's probe suites read from
//! `crates/post/tests/recorded/inputs/` (`PROBES`, `PATHS`). The post's suites
//! read those files and never generate; this suite is what fails when the
//! generator moves, so a recorded input cannot go stale silently. It lives
//! in app because app is the one crate that sees both motion and post (no
//! new crate edge, #919 Q4). The cl-built streams are guarded in post
//! (`crates/post/tests/recorded_streams.rs`).
//!
//! Regenerate after an intentional generator change, then re-run the post
//! suites and read their golden diffs:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test recorded_post_streams`
//!
//! Gate map (M4.4): covers motion::probing, motion::drilling, post::dump,
//! post::cnc, cl::text.

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
        wcs: 1,
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
        catalog: camcraft_tools::catalog_fields::ToolCatalog::default(),
        cutting: Vec::new(),
        outline: Vec::new(),
        turning: None,
        pitch_min: 0.0,
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
        wcs: 1,
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

// ------------------------------------------------------------ #926 slice 2
// The probe suites that moved to crates/post/tests (fanuc_post,
// probe_dialects, probe_guard, probe_report, probe_tormach,
// inverted_probe) read these recordings instead of calling probe_cycle.

fn input(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../post/tests/recorded/inputs").join(name)
}

fn guard_input(name: &str, text: &str) -> Option<String> {
    let path = input(name);
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(&path, text).expect("write the recorded input");
        return None;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    (want.replace("\r\n", "\n") != text).then(|| name.to_string())
}

/// One recorded probe stream: the cycle, then the dump every probe suite
/// used - operation Probe1, a 4 mm `PROBE-4` on `tool`, 50 mm clearance
/// and retract, G54 at `origin`, the result setting `set_wcs` (0 = G54)
/// within 0.5 mm.
struct Probe {
    name: &'static str,
    mode: ProbeMode,
    dir: ProbeDir,
    dir_y: ProbeDir,
    target: [f64; 3],
    radius: f64,
    tool: u32,
    set_wcs: Option<u8>,
    origin: [f64; 3],
}

const O: [f64; 3] = [0.0; 3];
/// inverted_probe's work-offset origin in the model frame.
const INV: [f64; 3] = [12.5, -7.25, 3.0];

#[allow(clippy::too_many_arguments)]
const fn pr(
    name: &'static str,
    mode: ProbeMode,
    dir: ProbeDir,
    dir_y: ProbeDir,
    target: [f64; 3],
    radius: f64,
    tool: u32,
    set_wcs: Option<u8>,
    origin: [f64; 3],
) -> Probe {
    Probe { name, mode, dir, dir_y, target, radius, tool, set_wcs, origin }
}

use ProbeDir::{XNeg, XPos, YNeg, YPos, ZNeg};
use ProbeMode::{Boss, Bore, Corner, Edge, Face};

const BORE: [f64; 3] = [30.0, 30.0, 20.0];
const FACE: [f64; 3] = [10.0, 5.0, 20.0];
const EDGE: [f64; 3] = [0.0, 5.0, 20.0];
const CORNER: [f64; 3] = [0.0, 0.0, 20.0];

#[rustfmt::skip]
const PROBES: [Probe; 28] = [
    // probe_dialects: the bore centre setting G55, nothing, G59; the edge.
    pr("probe_t7_bore_g55.cnc", Bore, ZNeg, YPos, BORE, 10.0, 7, Some(1), O),
    pr("probe_t7_bore_none.cnc", Bore, ZNeg, YPos, BORE, 10.0, 7, None, O),
    pr("probe_t7_bore_g59.cnc", Bore, ZNeg, YPos, BORE, 10.0, 7, Some(5), O),
    pr("probe_t7_edge_g55.cnc", Edge, XPos, YPos, EDGE, 0.0, 7, Some(1), O),
    // fanuc_post (#377): negative and zero expected values.
    pr("probe_t7_edge_neg_x_g54.cnc", Edge, XPos, YPos, [-15.0, 5.0, 20.0], 0.0, 7, Some(0), O),
    pr("probe_t7_corner_neg_g54.cnc", Corner, XPos, YPos, [-15.0, -10.0, 20.0], 0.0, 7, Some(0), O),
    pr("probe_t7_edge_zero_g54.cnc", Edge, XPos, YPos, [2.0, 5.0, 20.0], 0.0, 7, Some(0), O),
    pr("probe_t7_edge_hair_g54.cnc", Edge, XPos, YPos, [2.0 - 1e-7, 5.0, 20.0], 0.0, 7, Some(0), O),
    // probe_guard: the five modes setting G54.
    pr("probe_t7_face_g54.cnc", Face, ZNeg, YPos, FACE, 0.0, 7, Some(0), O),
    pr("probe_t7_edge_g54.cnc", Edge, XPos, YPos, EDGE, 0.0, 7, Some(0), O),
    pr("probe_t7_corner_g54.cnc", Corner, XPos, YPos, CORNER, 0.0, 7, Some(0), O),
    pr("probe_t7_bore_g54.cnc", Bore, ZNeg, YPos, BORE, 10.0, 7, Some(0), O),
    pr("probe_t7_boss_g54.cnc", Boss, ZNeg, YPos, BORE, 10.0, 7, Some(0), O),
    // probe_report and probe_tormach: PathPilot's probe tool 99.
    pr("probe_t99_face_g55.cnc", Face, ZNeg, YPos, FACE, 0.0, 99, Some(1), O),
    pr("probe_t99_face_none.cnc", Face, ZNeg, YPos, FACE, 0.0, 99, None, O),
    pr("probe_t99_corner_g54.cnc", Corner, XPos, YPos, CORNER, 0.0, 99, Some(0), O),
    pr("probe_t99_edge_g55.cnc", Edge, XPos, YPos, EDGE, 0.0, 99, Some(1), O),
    pr("probe_t99_corner_g55.cnc", Corner, XPos, YPos, CORNER, 0.0, 99, Some(1), O),
    pr("probe_t99_bore_g55.cnc", Bore, ZNeg, YPos, BORE, 10.0, 99, Some(1), O),
    pr("probe_t99_boss_g55.cnc", Boss, ZNeg, YPos, BORE, 10.0, 99, Some(1), O),
    pr("probe_t99_bore_none.cnc", Bore, ZNeg, YPos, BORE, 10.0, 99, None, O),
    // inverted_probe: every mode on a G54 off the model origin, setting G55.
    pr("inverted_probe_face_zneg.cnc", Face, ZNeg, YPos, FACE, 0.0, 7, Some(1), INV),
    pr("inverted_probe_face_xneg.cnc", Face, XNeg, YPos, [40.0, 5.0, 15.0], 0.0, 7, Some(1), INV),
    pr("inverted_probe_edge_yneg.cnc", Edge, YNeg, YPos, [20.0, 40.0, 20.0], 0.0, 7, Some(1), INV),
    pr("inverted_probe_corner_pos.cnc", Corner, XPos, YPos, CORNER, 0.0, 7, Some(1), INV),
    pr("inverted_probe_corner_neg.cnc", Corner, XNeg, YNeg, [80.0, 60.0, 20.0], 0.0, 7, Some(1), INV),
    pr("inverted_probe_bore.cnc", Bore, ZNeg, YPos, BORE, 10.0, 7, Some(1), INV),
    pr("inverted_probe_boss.cnc", Boss, ZNeg, YPos, BORE, 8.0, 7, Some(1), INV),
];

/// The probe paths themselves (`cl::text`), for probe_guard's law that
/// every GUARD is one of the path's positioning feeds: the five modes.
const PATHS: [(&str, ProbeMode, ProbeDir, [f64; 3], f64); 5] = [
    ("probe_face.cl", Face, ZNeg, FACE, 0.0),
    ("probe_edge.cl", Edge, XPos, EDGE, 0.0),
    ("probe_corner.cl", Corner, XPos, CORNER, 0.0),
    ("probe_bore.cl", Bore, ZNeg, BORE, 10.0),
    ("probe_boss.cl", Boss, ZNeg, BORE, 10.0),
];

/// Recorded inputs other suites generate and guard: motion's
/// vertical_arc_entry (the contour paths) and post's inverted_post (the
/// hand-built inverted-axis stream).
const GUARDED_ELSEWHERE: [&str; 4] =
    ["vertical_arc_x.cl", "vertical_arc_y.cl", "vertical_arc_oblique.cl", "inverted_probe.cnc"];

fn params(mode: ProbeMode, dir: ProbeDir, dir_y: ProbeDir, t: [f64; 3], radius: f64) -> ProbeParams {
    ProbeParams {
        mode,
        dir,
        dir_y,
        target: Point3::new(t[0], t[1], t[2]),
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

fn probe_stream(p: &Probe) -> String {
    let t = probe_cycle(&params(p.mode, p.dir, p.dir_y, p.target, p.radius)).expect("probe cycle");
    let d = Dump {
        op_name: "Probe1",
        tool_number: p.tool,
        tool_name: "PROBE-4",
        tool_diameter: 4.0,
        rpm: 0.0,
        clearance_z: 50.0,
        retract_z: 50.0,
        wcs: 1,
        origin: p.origin,
        probe_result: Some(post::probe_rec::ProbeResult { set_wcs: p.set_wcs, tol_mm: 0.5 }),
        ..Dump::default()
    };
    let recs = post::dump(&t, &d);
    assert!(!recs.iter().any(|r| matches!(r, cnc::Record::Warn { .. })), "{}: a sane probe path carries no WARN", p.name);
    cnc::serialize(&recs)
}

#[test]
fn every_recorded_probe_stream_is_current() {
    let stale: Vec<String> = PROBES.iter().filter_map(|p| guard_input(p.name, &probe_stream(p))).collect();
    assert!(
        stale.is_empty(),
        "recorded probe streams are stale: {stale:?} - their generator writes something else now; regenerate with WRITE_GOLDEN=1, then re-read every post golden they feed"
    );
}

#[test]
fn every_recorded_probe_path_is_current() {
    let stale: Vec<String> = PATHS
        .iter()
        .filter_map(|(name, mode, dir, t, r)| {
            let path = probe_cycle(&params(*mode, *dir, YPos, *t, *r)).expect("probe cycle");
            guard_input(name, &camcraft_app::cl::text::to_text(&path).expect("finite path"))
        })
        .collect();
    assert!(stale.is_empty(), "recorded probe paths are stale: {stale:?} - regenerate with WRITE_GOLDEN=1");
}

/// Every file under recorded/inputs has a generator that guards it: here,
/// or one of `GUARDED_ELSEWHERE`. A file nothing writes cannot sit there.
#[test]
fn every_recorded_input_has_a_generator() {
    let dir = input("");
    let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    on_disk.sort();
    let mut known: Vec<String> = PROBES.iter().map(|p| p.name).chain(PATHS.iter().map(|p| p.0)).chain(GUARDED_ELSEWHERE).map(String::from).collect();
    known.sort();
    assert_eq!(on_disk, known, "recorded/inputs holds exactly the guarded recordings");
}
