//! Shared helpers for the app integration tests.
//!
//! Provenance: the CL text format is ours - a plain-text dump of a
//! generated [`Toolpath`], one line per move, coordinates as integers at
//! [`CL_SCALE`] resolution (the same fixed-point idea as the geo2d
//! bridge). Goldens under `tests/golden/` are committed generated
//! artifacts per AGENTS.md; `write_golden` makes a stale golden a test
//! failure, never a silent pass.

#![allow(dead_code)] // each test binary uses part of the surface

pub mod bore;
pub mod cl_read;
pub mod engine_consumers;
pub mod engine_fixtures;
pub mod rotary_bar;
pub mod solid_part;

use std::path::PathBuf;

use flecs_ecs::prelude::World;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{
    AddTool, CommandError, CreateOperation, DeleteTool, GenerateOperation, History,
};
use camcraft_app::model;
use camcraft_app::operation::{OpParams, SrcRef};

/// M10.1: land an operation on a tool its family cannot cut with, for the
/// tests that pin the GENERATORS' shape refusals (the backstop the tool
/// law keeps). The command door refuses the direct creation - asserted
/// here first, so every caller also pins the gate - and the only road
/// left is M10.1 C's after-state: the op is created against a fitting
/// stand-in under the tool's name, then the tool is deleted and re-added
/// with its own (mismatched) definition out from under it.
pub fn create_past_the_tool_law(
    world: &World,
    history: &mut History,
    name: &str,
    tool: &str,
    src: SrcRef,
    params: OpParams,
) {
    let refused = CreateOperation::new(world, name, tool, src.clone(), params.clone());
    assert!(refused.is_err(), "the tool law must refuse '{tool}' for {name} at the door");
    let entity = model::find_by_name(world, tool).expect("the tool is on the rack");
    let own = model::tool_of(world, &entity).expect("a tool definition");
    let info = params.kind().info();
    let stand_in = camcraft_app::tools::catalog()
        .into_iter()
        .map(|(_, t)| t)
        .find(|t| info.accepts(t.kind))
        .expect("the catalog holds a fitting tool for every kind");
    history.execute(world, DeleteTool::new(world, tool).expect("delete"));
    history.execute(world, AddTool::new(world, tool, stand_in).expect("stand-in"));
    history.execute(world, CreateOperation::new(world, name, tool, src, params).expect("create"));
    history.execute(world, DeleteTool::new(world, tool).expect("delete stand-in"));
    history.execute(world, AddTool::new(world, tool, own).expect("restore"));
}

/// M10.1 C: generate an op [`create_past_the_tool_law`] landed. The
/// command refuses first - `GenerateOperation` now blocks a mismatched
/// tool by the op's name before any generator runs, asserted here so
/// every caller pins that gate too - and the generator itself is then
/// asked directly through `operation::generate` (the pure read the
/// command wraps), so the GENERATORS' shape doors, the backstop the
/// law keeps, stay under test. The error is the command's own shape
/// (`Machining("<op>: <generator words>")`), so callers read the same
/// sentence the command used to carry.
pub fn generate_past_the_tool_law(world: &World, name: &str) -> Result<(), CommandError> {
    let Err(gate) = GenerateOperation::new(world, name) else {
        panic!("the tool law must block generating {name} before its generator runs");
    };
    let gate = gate.to_string();
    assert!(
        gate.contains(&format!("{name}'s tool")) && gate.contains("cuts with"),
        "the block names the op and the kind it cuts with: {gate}"
    );
    let entity = model::find_by_name(world, name).expect("the op exists");
    let op = model::operation_of(world, &entity).expect("an operation");
    camcraft_app::operation::generate(world, &op, name)
        .map(|_| ())
        .map_err(|e| CommandError::Machining(format!("{name}: {e}")))
}

/// Golden coordinate resolution in mm: 0.001 = 1 um.
pub const CL_SCALE: f64 = 0.001;

/// Millimeters to golden units (microns), rounded. Generator output is
/// already validated finite, so the cast is safe here.
fn q(mm: f64) -> i64 {
    (mm / CL_SCALE).round() as i64
}

/// Deterministic text form of a toolpath: a header naming the format and
/// move count, the start point, then one line per move - kind
/// (Rapid/Feed/Plunge, or `Arc` when the move carries a circular
/// segment), destination coordinates, feed in mm/min when the move has
/// one, and for an arc the center `I`/`J` plus sense `S` (+1 ccw, -1
/// cw). v2 (the `Arc` token) appears only when the path carries arcs;
/// straight paths dump exactly as in v1. v3 adds `; CYCLE` comment
/// lines - one per canned cycle, in hole order - so drilling goldens
/// diff the POSTING INTENT (what M1.7's post must emit as G73/G81/G83/
/// G84 blocks) alongside the expanded move chain that verify and the
/// simulator read. Lengths are microns like coordinates; dwell is
/// milliseconds. `AT` is the cycle's first move index: the anchor of
/// the agreement law that pins chain and record to each other. Nothing
/// in the dump depends on the run: no timestamps, no hash-map iteration
/// order, full-precision-integer coordinates. v4 (M5.1 stage A) adds
/// the keyed `A`/`B`/`C` millidegree words a move whose tool leans -
/// the same grammar the `.cnc` reader takes for a pose, and never
/// silently dropped: a tilted path that dumped without its letters
/// would make the golden lie about what the path is. A prismatic path
/// dumps byte-for-byte as v1-v3 always did. M11.2 A: an arc's center
/// words name its plane - `I`/`J` for XY (every arc dumped before,
/// unchanged), `I`/`K` for XZ, `J`/`K` for YZ - the `.cnc` stream's
/// own law; no version bump, since an XY arc states nothing new.
pub fn gen_to_cltext(t: &Toolpath) -> String {
    let version = if !t.is_prismatic() {
        4
    } else if t.plasma.is_some() {
        // #670: the plasma records - `; PLASMA` after START, `; TORCH
        // ON` before each span's first lit move, `; TORCH OFF` after
        // its last (docs/CNC-INTERMEDIATE.md, the `.cl` plasma records).
        5
    } else if !t.cycles.is_empty() {
        3
    } else if t.has_arcs() {
        2
    } else {
        1
    };
    let mut s = format!("; HSMW-CL v{version} scale={CL_SCALE}mm moves={}\n", t.len());
    match t.start() {
        Some(p) => s.push_str(&format!("START {} {} {}\n", q(p.x), q(p.y), q(p.z))),
        None => s.push_str("START\n"),
    }
    if let Some(p) = &t.plasma {
        s.push_str(&p.header_line());
        s.push('\n');
    }
    let torches: &[camcraft_app::cl::TorchSpan] = t.plasma.as_ref().map_or(&[], |p| &p.torches);
    for (i, m) in t.moves.iter().enumerate() {
        for sp in torches.iter().filter(|sp| sp.on == i) {
            s.push_str(&sp.on_line());
            s.push('\n');
        }
        let mut k = match m.kind {
            MoveKind::Rapid => "Rapid",
            MoveKind::Feed => "Feed",
            MoveKind::Plunge => "Plunge",
        };
        let mut arc = String::new();
        if let Some(a) = m.arc {
            // An arc is always a planar feed (`push_arc` refuses
            // anything else), so the token loses nothing but the
            // redundancy.
            k = "Arc";
            let (l0, l1) = a.plane.center_letters();
            arc = format!(
                " {l0}{} {l1}{} S{}",
                q(a.center.x),
                q(a.center.y),
                if a.ccw { 1 } else { -1 }
            );
        }
        s.push_str(&format!("{k} {} {} {}", q(m.to.x), q(m.to.y), q(m.to.z)));
        if let Some(f) = m.feed {
            s.push_str(&format!(" F{}", (f * 1000.0).round() as i64));
        }
        s.push_str(&arc);
        // A stated pose posts its letters; the exact vertical is the
        // absence of a pose (is_vertical's law), so it dumps nothing.
        if let Some(a) = m.axis.filter(|a| !camcraft_app::cl::is_vertical(a)) {
            let [da, db, dc] = camcraft_app::cl::axis_to_angles(a);
            s.push_str(&format!(
                " A{} B{} C{}",
                (da * 1000.0).round() as i64,
                (db * 1000.0).round() as i64,
                (dc * 1000.0).round() as i64
            ));
        }
        // The stated spindle C (issue #33): absent on every golden but
        // a C-axis live-tool cut, so the dumps stay byte-identical.
        if let Some(c) = m.rotary {
            s.push_str(&format!(" SC{}", (c * 1000.0).round() as i64));
        }
        s.push('\n');
        for sp in torches.iter().filter(|sp| sp.off == i) {
            s.push_str(&sp.off_line());
            s.push('\n');
        }
    }
    for c in &t.cycles {
        let kind = match c.kind {
            camcraft_app::cl::CycleKind::Spot => "Spot",
            camcraft_app::cl::CycleKind::Drill => "Drill",
            camcraft_app::cl::CycleKind::Peck => "Peck",
            camcraft_app::cl::CycleKind::Deep => "Deep",
            camcraft_app::cl::CycleKind::Tap => "Tap",
            camcraft_app::cl::CycleKind::FloatingTap => "FloatingTap",
            camcraft_app::cl::CycleKind::Bore => "Bore",
            camcraft_app::cl::CycleKind::BackBore => "BackBore",
        };
        s.push_str(&format!(
            "; CYCLE {kind} X{} Y{} RZ{} BZ{} ENTRY{} PECK{} DWELL{} AT{}\n",
            q(c.xy.x),
            q(c.xy.y),
            q(c.r_z),
            q(c.bottom_z),
            q(c.entry_z),
            q(c.peck),
            (c.dwell * 1000.0).round() as i64,
            c.first_move,
        ));
    }
    s
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("golden")
}

/// Compare `text` against `tests/golden/<name>`; with WRITE_GOLDEN set in
/// the environment, rewrite the file instead (regeneration workflow:
/// `WRITE_GOLDEN=1 cargo test -p camcraft-app`, then read the diff and
/// commit only intended changes). A missing golden fails with a hint
/// rather than quietly generating a fresh one.
pub fn write_golden(name: &str, text: &str) {
    let dir = golden_dir();
    let path = dir.join(name);
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        // Dialect goldens nest (`nc/<dialect>/`): create the file's own
        // parent, not just the root.
        std::fs::create_dir_all(path.parent().unwrap_or(&dir)).expect("cannot create tests/golden");
        std::fs::write(&path, text).expect("cannot write golden file");
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("cannot read golden {}: {e}; create it with WRITE_GOLDEN=1 cargo test", path.display())
    });
    // A working copy checked out under git core.autocrlf carries CRLF where
    // the generator writes LF. The toolpath is compared byte-exactly; the
    // newline convention is not part of a toolpath, so the file is read to
    // the generator's convention before comparing. Content regressions still
    // fail identically - only `\r\n` -> `\n` is absorbed.
    let want = want.replace("\r\n", "\n");
    assert_eq!(want, text, "golden {} is stale; regenerate with WRITE_GOLDEN=1 cargo test", path.display());
}
