//! In-app posting: the live world to NC text, through the CPS
//! architecture of [`crate::post`].
//!
//! Provenance: our own design; the three-file role split (script,
//! machine, intermediate) per `docs/CNC-INTERMEDIATE.md` and
//! ROADMAP M1.7, specced from Autodesk's published post-processor
//! documentation, behavior only, per the Clean-Room Protocol.
//!
//! The law here is the validation gate: a program is written only
//! when every operation is generated, fresh, and warning-free, AND
//! the program-level verification passes - no gouge in the material
//! sim, no limit violated on the mounted machine (M2.3 D; or nothing
//! is written at all - the Prime Directive's blocking rule). Context
//! resolution mirrors generation exactly - the same `resolve_params`,
//! the same stock box, the tool library's own rank for `T..`/`H..` -
//! so the posted numbers are the machined ones.

use crate::model;
use crate::operation::{self, Height, OpStatus, OpParams};
use crate::post::{
    self, cnc, engine,
    options::{self, DeclaredOption, OptionValue},
    Dump,
};
use crate::schema::{EndUnits, PostOptions, PostRef, ThreadingOutput};
use flecs_ecs::prelude::*;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

#[path = "posting_files.rs"]
mod files;
pub use files::{
    builtin_machine_text, builtin_post_path, builtin_post_text, default_mch_text, shipped_machines, shipped_posts, AssetFile, BUILTINS,
    DEFAULT_MCH_FILE, MACHINES,
};
pub use camcraft_post::data_scan::{scan_folder, FolderScan};
pub use files::{machine_scan, post_scan, rescan, scan_reads};
#[path = "posting_default_machine.rs"]
mod default_mch;
pub use default_mch::{default_limits, default_machine, default_machine_checked, parse_default_machine, KeptMachine};

/// The registry entries that speak TURNING (M13.5): X as a diameter
/// on one meridian, G18, G99, the face cycles. Everything else in
/// `BUILTINS` is a milling dialect. The picker filters by the mounted
/// machine's kind through [`post_serves`]; a mill-turn lists both.
pub const LATHE_POSTS: &[&str] = &["Fanuc Lathe", "Tormach PathPilot Lathe"];

/// The registry entries that speak PLASMA (#669): the torch words, no
/// spindle and no Z depth. #671 ships the PathPilot plasma post for the
/// Tormach 1300PL; it serves plasma tables only, and no cutting post
/// serves one (a torch has no spindle for their S/M3).
pub const PLASMA_POSTS: &[&str] = &["Tormach PathPilot Plasma"];

/// The registry entries that speak WIRE EDM (#533): RS-274 motion with
/// the control's own codes from the machine file, and (#617) the Sodick
/// dialect with its manual's words. They serve wire EDMs only, and no
/// cutting post serves one. The first is the default for a wire EDM.
pub const WIRE_POSTS: &[&str] = &["Generic ISO Wire EDM", "Sodick Wire EDM (Mark 21)"];

/// The registry entries that speak SINKER EDM (#542): RS-274 motion
/// with the control's own codes from the machine file, each declaring
/// `serves = ["sinker_edm"]`. They serve sinker EDMs only, and no
/// cutting or wire post serves one.
pub const SINKER_POSTS: &[&str] = &["Sinker EDM (ISO)"];

/// Whether a registry entry can post programs for a machine of this
/// kind: a lathe post serves lathes and mill-turns, a mill post serves
/// mills and mill-turns. The picker's listing law, and since #1232 the
/// post's gate too: every posting path refuses by name a mount whose
/// post does not serve its machine's kind (`posting_serves.rs`).
#[must_use]
pub fn post_serves(name: &str, kind: crate::machine::Kind) -> bool {
    use crate::machine::Kind;
    let lathe_post = LATHE_POSTS.contains(&name);
    let plasma_post = PLASMA_POSTS.contains(&name);
    let wire_post = WIRE_POSTS.contains(&name);
    let sinker_post = SINKER_POSTS.contains(&name);
    match kind {
        Kind::Mill => !lathe_post && !plasma_post && !wire_post && !sinker_post,
        Kind::Lathe => lathe_post,
        Kind::MillTurn => !plasma_post && !wire_post && !sinker_post,
        // #669: a cutting post would write spindle words a torch has not.
        Kind::Plasma => plasma_post,
        // #533: the wire post serves wire EDMs; #542: the sinker post
        // serves sinker EDMs; no milling or turning dialect speaks to an
        // EDM control, and neither EDM post to the other kind.
        Kind::WireEdm => wire_post,
        Kind::SinkerEdm => sinker_post,
        // No waterjet post ships in this data-model slice.
        Kind::Waterjet => false,
    }
}

/// The registry's default post for a machine kind: entry 0 (Fanuc
/// mill) for mills and mill-turns, the first lathe post for lathes.
/// A mount does not re-point the document's post by itself - the
/// post choice is its own command - but this is the name a Setup
/// pane or a script offers first for a freshly mounted lathe.
#[must_use]
pub fn default_post_for(kind: crate::machine::Kind) -> PostRef {
    match kind {
        crate::machine::Kind::Lathe => LATHE_POSTS
            .first()
            .map_or(PostRef::Builtin, |n| PostRef::Named((*n).to_owned())),
        // #669/#671: the first plasma post (PathPilot's, for the 1300PL).
        crate::machine::Kind::Plasma => PLASMA_POSTS
            .first()
            .map_or(PostRef::Builtin, |n| PostRef::Named((*n).to_owned())),
        // #533: the generic wire post serves wire EDMs; #542: the
        // generic sinker post serves sinker EDMs.
        crate::machine::Kind::WireEdm => WIRE_POSTS
            .first()
            .map_or(PostRef::Builtin, |n| PostRef::Named((*n).to_owned())),
        crate::machine::Kind::SinkerEdm => SINKER_POSTS
            .first()
            .map_or(PostRef::Builtin, |n| PostRef::Named((*n).to_owned())),
        crate::machine::Kind::Mill | crate::machine::Kind::MillTurn | crate::machine::Kind::Waterjet => PostRef::Builtin,
    }
}

/// The registry name a post reference resolves to: the first entry
/// for the bare built-in, the name itself for a named one, nothing
/// for a file of the user's own (whose dialect nobody here can read).
#[must_use]
pub fn post_name(post: &PostRef) -> Option<&str> {
    match post {
        PostRef::Builtin => BUILTINS.first().map(|(n, _)| *n),
        PostRef::Named(n) => Some(n.as_str()),
        PostRef::File(_) => None,
    }
}

/// A post the Setup pane may offer instead of the document's (M13.5
/// B): when the mounted machine's kind is not served by the current
/// registry post, this is the word for what the current one serves
/// ("mill" / "lathe") and the registry's default for the mount. The
/// pane offers; the user chooses - a mount never re-points the post
/// by itself, and a `File` post is never second-guessed.
#[must_use]
pub fn post_offer(world: &World) -> Option<(&'static str, PostRef)> {
    let kind = crate::machine::kind_of(world);
    let current = post_of(world);
    let name = post_name(&current)?;
    if post_serves(name, kind) {
        return None;
    }
    let serves = if LATHE_POSTS.contains(&name) {
        "lathe"
    } else if PLASMA_POSTS.contains(&name) {
        "plasma"
    } else if WIRE_POSTS.contains(&name) {
        "wire EDM"
    } else if SINKER_POSTS.contains(&name) {
        "sinker EDM"
    } else {
        "mill"
    };
    // #669: an offer that would not serve the mount either (a plasma
    // table before any plasma post ships) is no offer.
    let offer = default_post_for(kind);
    post_name(&offer).is_some_and(|n| post_serves(n, kind)).then_some((serves, offer))
}


/// The mount path a built-in library entry records (stage C): the
/// pane's combo and the `LoadMachine::builtin` command say the same
/// string, so "loaded from" never quotes a file that does not exist.
#[must_use]
pub fn builtin_path(name: &str) -> String {
    format!("built-in: {name}")
}

/// The machine this document posts to (M3.3 A): the mounted `.mch`
/// restated in the static post grammar, or the default VMC where the
/// document says no machine. The same fact the limit gate judges, now
/// also shaping the script's `machine` global and the sim's traverse
/// clock - post, gate and timeline cannot disagree about the machine.
#[must_use]
pub fn machine_value(world: &World) -> Value {
    machine_value_for(&crate::machine::of(world))
}

/// The post the program runs: the active setup's (#343 - its own, else
/// the document's `PostRef` singleton). Setup furniture moved through
/// commands or a document load, never ad hoc - the `machine::set` law
/// wearing the post's own clothes.
#[must_use]
pub fn post_of(world: &World) -> PostRef {
    post_of_setup(world, crate::setup::active(world).as_deref())
}

/// The document's post as stored - what every setup that owns no post
/// follows (#343). For `SelectPost`, the save and the document section.
#[must_use]
pub fn document_post_of(world: &World) -> PostRef {
    crate::setup_mount::document_post(world)
}

/// The spindle speed a program commands for `tool`, rpm (#565): a touch
/// probe is never spun (0 - its catalog `rpm` is never posted, and the
/// posts start no spindle for it, #523); every cutter runs its own. The
/// one answer the post and the machine's spindle-range check both read.
#[must_use]
pub fn posted_rpm(tool: &crate::tools::Tool) -> f64 {
    // A torch or wire states rpm 0 by law (#670, #2011): no spindle word, like a probe.
    if matches!(tool.kind, crate::tools::ToolKind::Probe | crate::tools::ToolKind::Torch | crate::tools::ToolKind::Wire) { 0.0 } else { tool.rpm }
}

/// Write the singleton. For the `SelectPost` command and the project
/// reader only.
pub fn post_set(world: &World, next: &PostRef) {
    world.set(next.clone());
}

/// The post options the program posts with (issue #34): the active
/// setup's (#389 - its own, else the document's `PostOptions`
/// singleton, [`crate::setup_post_options::document`]).
#[must_use]
pub fn post_options(world: &World) -> PostOptions {
    crate::setup_post_options::in_force(world)
}

/// Write the options singleton. For `SetPostOptions` and the project
/// reader only.
pub fn post_options_set(world: &World, next: PostOptions) {
    world.set(next);
}

/// The options as the script's `postOptions` object: the words a
/// script compares against (`Threading`: `"G32"` or `"G76"`;
/// `EndUnits`: `"inch"` or `"mm"`, absent when the program's unit is
/// left modal, #596).
#[must_use]
pub fn options_value(o: PostOptions) -> Value {
    let threading = match o.threading {
        ThreadingOutput::G32Passes => "G32",
        ThreadingOutput::G76Cycle => "G76",
    };
    let mut v = serde_json::json!({ "Threading": threading });
    // #616: stated only when on - off, the object a script sees is
    // byte-identical to before (the posts read the per-op PARAMs; this
    // word is for a script that wants the document-wide switch).
    if o.smoothing {
        v["Smoothing"] = serde_json::json!(true);
    }
    // #1386: stated only when on, like Smoothing - off, the object is
    // byte-identical to before and no post explains anything.
    if o.explain {
        v["Explain"] = serde_json::json!(true);
    }
    let end = match o.end_units {
        EndUnits::Leave => None,
        EndUnits::Inch => Some("inch"),
        EndUnits::Millimeters => Some("mm"),
    };
    if let (Some(end), Some(map)) = (end, v.as_object_mut()) {
        map.insert("EndUnits".to_owned(), Value::from(end));
    }
    if o.wcs_rotation == crate::schema::WcsRotationOutput::Control
        && let Some(map) = v.as_object_mut()
    {
        map.insert("WcsRotation".to_owned(), Value::from("control"));
    }
    v
}

/// The document's post for display: built-in name, or the file path
/// as chosen. Never touches disk - a pane may draw this every frame,
/// and it names exactly what `script_source` resolves. One truth,
/// read two ways.
#[must_use]
pub fn post_label(world: &World) -> String {
    post_ref_label(&post_of(world))
}

/// [`post_label`] for any post reference (#343: a setup's own post).
#[must_use]
pub fn post_ref_label(post: &PostRef) -> String {
    match post.clone() {
        PostRef::Builtin => match BUILTINS.first() {
            Some((name, _)) => format!("{name} (built-in)"),
            None => "built-in".to_owned(),
        },
        PostRef::Named(name) => match BUILTINS.iter().find(|(n, _)| *n == name) {
            Some((n, _)) => format!("{n} (built-in)"),
            // Unresolvable at display time: name the stranger. A
            // removed registry entry must show as itself, not as the
            // default it will never post as (script_source refuses).
            None => format!("{name} (built-in, missing)"),
        },
        PostRef::File(path) => path,
    }
}

/// The script source this document posts with: (label, text). A
/// `File` post whose text cannot be read, or a `Named` post missing
/// from the registry, is a refusal naming what is gone - never a
/// silent run with the default instead (a substituted post is exactly
/// the silent wrong answer this whole codebase exists to prevent; the
/// Prime Directive).
pub fn script_source(world: &World) -> Result<(String, String), String> {
    script_of(&post_of(world))
}

/// [`script_source`] for any post reference (#388: a mount's own).
pub fn script_of(post: &PostRef) -> Result<(String, String), String> {
    match post.clone() {
        PostRef::Builtin => {
            let (name, _) = BUILTINS.first().ok_or_else(|| "the built-in post registry is empty".to_owned())?;
            Ok((format!("{name} (built-in)"), builtin_post_text(name)?.to_owned()))
        }
        PostRef::Named(name) => Ok((format!("{name} (built-in)"), builtin_post_text(&name)?.to_owned())),
        PostRef::File(path) => {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("post script {path}: {e}"))?;
            Ok((path, text))
        }
    }
}

/// What a successful post says back.
#[derive(Debug)]
pub struct Posted {
    pub ops: usize,
    pub lines: usize,
    /// The exact program that was written - the M1.7d viewer shows the
    /// disk's content without re-reading it (one source of truth: text
    /// and positions come from the same engine run).
    pub program: String,
    /// One entry per program line: the world position of the move that
    /// line writes, or `None` for header, tool and section lines.
    pub line_pos: Vec<Option<[f64; 3]>>,
    /// Block-by-block trace of this very run (M3.3 stage D): which
    /// toolpath record wrote which program lines, in the engine's own
    /// words. The viewer's trace pane rides the same run that wrote
    /// the file - one source of truth, never a re-post.
    pub blocks: Vec<engine::TraceBlock>,
    /// The material sim's "did not judge this program: <reason>" line
    /// when it refused the program and the generator-time verify
    /// covered every op ([`sim_clause`]'s non-blocking arm). `None`
    /// means the sim judged it. Never written into the NC text - the
    /// program a judged post writes stays byte-identical - but every
    /// surface that reports the post (status line, viewer header,
    /// script result) shows it.
    pub not_judged: Option<String>,
}

#[path = "posting_sim.rs"]
mod sim_clause_impl;
#[path = "posting_plasma.rs"]
mod plasma_clause_impl;
pub use plasma_clause_impl::{plasma_clause, PlasmaClause, PLASMA_VERDICT};
#[path = "posting_wire.rs"]
mod wire;
pub use wire::{wire_clause, WireClause, WIRE_VERDICT};
#[path = "posting_per_op.rs"]
mod per_op;
#[path = "posting_suppressed.rs"]
mod suppressed;
#[path = "posting_wcs_rotation.rs"]
mod wcs_rotation;
#[path = "posting_wcs_slot.rs"]
mod wcs_slot;
#[path = "posting_mounts.rs"]
mod mounts;
#[path = "posting_setup_change.rs"]
mod setup_change;
#[path = "posting_program.rs"]
mod program;
pub use program::{
    check as check_program_info, document_program_id, parse_number as parse_program_number,
    program_id, program_info, program_info_set, PROGRAM_COMMENT_MAX, PROGRAM_NUMBER_MAX,
};
pub(crate) use program::normalised as normalised_program_info;
#[path = "posting_units.rs"]
mod program_unit;
pub use program_unit::{end_units_words, program_options, unit_word};

/// Declared per-post options read from a [`PostRef`]'s `.cps` source (#258).
pub fn declared_options_for_post(post: &PostRef) -> Result<Vec<DeclaredOption>, String> {
    let (_, cps) = script_of(post)?;
    options::declared_options(&cps).map_err(|e| e.to_string())
}

/// Host [`program_options`] layered with a script's `declaredOptions` defaults,
/// stored overrides for `(setup, post)`, and optional per-run `extra` overrides (#258).
pub fn program_options_for_script(
    world: &World,
    cps: &str,
    post: Option<&PostRef>,
    setup: Option<&str>,
    extra: Option<&BTreeMap<String, OptionValue>>,
) -> Result<Value, String> {
    let host = match setup {
        Some(s) => {
            let mut v = options_value(crate::setup_post_options::of(world, Some(s)));
            if let Some(map) = v.as_object_mut() {
                map.insert(
                    "Units".to_owned(),
                    Value::from(unit_word(crate::units::of(world))),
                );
            }
            v
        }
        None => program_options(world),
    };
    let schema = options::declared_options(cps).map_err(|e| e.to_string())?;
    let mut overrides = post
        .map(|p| crate::setup_post_options::script_for_post(world, setup, p))
        .unwrap_or_default();
    if let Some(extra) = extra {
        for (k, v) in extra {
            overrides.insert(k.clone(), v.clone());
        }
    }
    options::merge_post_options(&schema, &host, &overrides)
}

/// Convert a JSON `{ key: scalar }` map into post script overrides (#258).
pub(crate) fn json_map_to_overrides(val: &Value) -> Result<BTreeMap<String, OptionValue>, String> {
    let map = val
        .as_object()
        .ok_or_else(|| "\"options\" must be an object".to_owned())?;
    let mut out = BTreeMap::new();
    for (k, v) in map {
        if options::RESERVED_KEYS.contains(&k.as_str()) {
            return Err(format!("post option '{k}' is reserved by host settings"));
        }
        let ov: OptionValue = serde_json::from_value(v.clone())
            .map_err(|_| format!("post option '{k}' must be a bool, number, or string"))?;
        if !ov.is_finite() {
            return Err(format!("post option '{k}' must be finite"));
        }
        out.insert(k.clone(), ov);
    }
    Ok(out)
}

/// Resolve a built-in post name or `.cps` file path to its [`PostRef`] and script text (#258, #2190).
pub fn resolve_cps(cps: &str) -> Result<(PostRef, String), String> {
    if BUILTINS.iter().any(|(n, _)| *n == cps) {
        return Ok((PostRef::Named(cps.to_owned()), builtin_post_text(cps)?.to_owned()));
    }
    if Path::new(cps)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cps"))
    {
        let text = std::fs::read_to_string(cps)
            .map_err(|e| format!("cannot read post script {cps}: {e}"))?;
        return Ok((PostRef::File(cps.to_owned()), text));
    }
    let names: Vec<&str> = BUILTINS.iter().map(|(n, _)| *n).collect();
    Err(format!(
        "unknown built-in post '{cps}' - names: {}; or give a .cps file path",
        names.join(", ")
    ))
}

/// Resolve a built-in post name or `.cps` file path to its script text (#258).
pub fn resolve_cps_text(cps: &str) -> Result<String, String> {
    resolve_cps(cps).map(|(_, text)| text)
}

fn scalar_from_cli(raw: &str) -> Value {
    match raw {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => {
            if let Ok(i) = raw.parse::<i64>() {
                Value::Number(i.into())
            } else if let Ok(f) = raw.parse::<f64>()
                && f.is_finite()
                && let Some(n) = serde_json::Number::from_f64(f)
            {
                Value::Number(n)
            } else {
                Value::String(raw.to_owned())
            }
        }
    }
}

/// Parse repeatable CLI `key=value` post option specs, validating early against `cps` when known (#258).
pub fn parse_cli_post_options(
    specs: &[String],
    cps: Option<&str>,
) -> Result<serde_json::Map<String, Value>, String> {
    if specs.is_empty() {
        return Ok(serde_json::Map::new());
    }
    if let Some(cps_spec) = cps {
        let src = resolve_cps_text(cps_spec)?;
        let schema = options::declared_options(&src).map_err(|e| e.to_string())?;
        let mut parsed = BTreeMap::new();
        for spec in specs {
            let (k, v) = options::parse_cli_override(&schema, spec)?;
            parsed.insert(k, v);
        }
        return Ok(parsed.into_iter().map(|(k, v)| (k, v.to_json())).collect());
    }
    let mut out = serde_json::Map::new();
    for spec in specs {
        let (key, raw) = spec
            .split_once('=')
            .ok_or_else(|| format!("--post-option expects key=value, got '{spec}'"))?;
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("--post-option key must not be empty in '{spec}'"));
        }
        if options::RESERVED_KEYS.contains(&key) {
            return Err(format!("post option '{key}' is reserved by host settings"));
        }
        out.insert(key.to_owned(), scalar_from_cli(raw));
    }
    Ok(out)
}
#[path = "posting_serves.rs"]
mod serves;
#[path = "posting_edm.rs"]
mod edm_params;
#[path = "posting_nose.rs"]
pub mod nose;
#[path = "posting_index_swing.rs"]
pub mod index_swing;
#[path = "posting_swing_standing.rs"]
pub mod swing_standing;
#[path = "posting_section_mode.rs"]
mod section_mode;
pub(crate) use wcs_rotation::{offset_frame, OffsetFrame};
pub use mounts::{
    machine_checks, machine_value_for, machine_value_of, post_mounts,
    post_mounts_for_post_with_options, post_mounts_script, post_mounts_script_with_options,
    post_mounts_with_script_options, post_of_setup, PostedMount,
};
pub use per_op::{
    file_stem, post_per_op, post_per_op_for_post_with_options, post_per_op_script, post_per_op_script_with_options,
    post_per_op_with_script_options, PostedFile,
};
pub use sim_clause_impl::{sim_clause, SimClause, NOT_JUDGED};

/// The verification gate's whole answer: the blocking problems, and
/// the stock sim's visible not-judged line (see [`sim_clause`]).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct GateReport {
    pub problems: Vec<String>,
    pub not_judged: Option<String>,
}

/// Post every operation of the document, in tree order, to one NC
/// file. Every refusal names its reason; nothing partial is ever
/// written. Cached generation warnings ride the stream as `WARN`
/// records, so the engine's own warning count is the gate.
///
/// One file holds one post for one machine: the program's own mount
/// (#388 - its setups', not the active one's). A program over several
/// mounts is refused here by name; [`post_mounts`] writes it as one
/// file per mount.
pub fn post_setup(world: &World, out_path: &Path) -> Result<Posted, String> {
    post_setup_with_script_options(world, out_path, None)
}

/// [`post_setup`] with temporary per-run script option overrides (#258).
pub fn post_setup_with_script_options(
    world: &World,
    out_path: &Path,
    extra: Option<&BTreeMap<String, OptionValue>>,
) -> Result<Posted, String> {
    let post = mounts::single(world, true)?.post;
    let (label, cps) = script_of(&post)?;
    post_script_as(world, out_path, &cps, Some(&label), Some(&post), extra)
}

/// The gate's post at a given script (M3.3 A): the stream and the
/// verification are the document's own; only the translation is
/// parameterized - what a caller holding a script text runs, and
/// what the tests use to exercise the script seam directly.
pub fn post_script(world: &World, out_path: &Path, cps: &str) -> Result<Posted, String> {
    post_script_with_script_options(world, out_path, cps, None)
}

/// [`post_script`] with temporary per-run script option overrides (#258).
pub fn post_script_with_script_options(
    world: &World,
    out_path: &Path,
    cps: &str,
    extra: Option<&BTreeMap<String, OptionValue>>,
) -> Result<Posted, String> {
    post_script_as(world, out_path, cps, None, None, extra)
}

/// [`post_script`] for a resolved [`PostRef`] with temporary per-run script option overrides (#258, #2190).
pub fn post_script_for_post_with_options(
    world: &World,
    out_path: &Path,
    post: &PostRef,
    cps: &str,
    extra: Option<&BTreeMap<String, OptionValue>>,
) -> Result<Posted, String> {
    let label = post_ref_label(post);
    post_script_as(world, out_path, cps, Some(&label), Some(post), extra)
}

/// [`post_script`] with the script's label (a post file's path), for
/// the refusal that names a post not serving the machine (#1232).
fn post_script_as(
    world: &World,
    out_path: &Path,
    cps: &str,
    label: Option<&str>,
    post: Option<&PostRef>,
    extra: Option<&BTreeMap<String, OptionValue>>,
) -> Result<Posted, String> {
    let started = crate::clock::Instant::now();
    let posted = post_script_body_with(world, cps, label, post, extra)
        .and_then(|p| write_program(out_path, p));
    match &posted {
        Ok(p) => tracing::info!(
            file = %crate::logging::file_name(out_path),
            bytes = p.program.len(),
            ops = p.ops,
            lines = p.lines,
            ms = crate::logging::elapsed_ms(started),
            "NC program written"
        ),
        Err(why) => tracing::warn!(file = %crate::logging::file_name(out_path), "NC program not written: {why}"),
    }
    posted
}

/// The whole program gate of [`post_setup`] with nothing written (#1431):
/// the same mount, stream, verification and engine run, the program held
/// in memory. The browser build hands it to a download, which has no path
/// to write to; a program over several mounts is refused by name, as the
/// one-file post refuses it.
pub fn post_setup_unwritten(world: &World) -> Result<Posted, String> {
    let post = mounts::single(world, true)?.post;
    let (label, cps) = script_of(&post)?;
    let posted = post_script_body_with(world, &cps, Some(&label), Some(&post), None);
    match &posted {
        Ok(p) => tracing::info!(bytes = p.program.len(), ops = p.ops, lines = p.lines, "NC program posted, not written"),
        Err(why) => tracing::warn!("NC program not posted: {why}"),
    }
    posted
}

/// Write a gated program to `out_path`; a failed write removes what it
/// left, because a half-written program is the silent wrong answer.
fn write_program(out_path: &Path, posted: Posted) -> Result<Posted, String> {
    std::fs::write(out_path, posted.program.as_bytes()).map_err(|e| {
        let _ = std::fs::remove_file(out_path);
        format!("cannot write {}: {e}", out_path.display())
    })?;
    Ok(posted)
}

fn post_script_body_with(
    world: &World,
    cps: &str,
    label: Option<&str>,
    post: Option<&PostRef>,
    extra: Option<&BTreeMap<String, OptionValue>>,
) -> Result<Posted, String> {
    // #388: the program's one machine (a program over several refuses).
    let mount = mounts::single(world, false)?;
    let streams = op_streams_by(world, false)?;
    // #1190: a turning dialect opens only what a lathe may run.
    crate::lathe_voice::at_post(world, streams.iter().map(|(n, _)| n.as_str()), cps, label)?;
    // #1232: and a post writes only for a machine kind it serves.
    let names: Vec<&str> = streams.iter().map(|(n, _)| n.as_str()).collect();
    let unserved = serves::refusal(world, &mount.setups, &names, &mount.machine, cps, label)?;
    let machine = mount.machine;
    let records = join_streams(streams);
    // The verification gate (M2.3 D): the same two program-level
    // judgments the Deviate cell shows, made binding. Evidence is
    // computed here and now - never a cached verdict carried in from
    // the viewer (the PostView law: a verdict about an older world
    // must not release this one's metal).
    // #1174: the workholding clause reads the dialect of THIS script.
    let voice = post::dialect::work_voice(cps).map_err(|e| format!("post engine: {e}"))?;
    let mut report = verification_report_by(world, &|_| voice);
    report.problems.splice(0..0, unserved);
    // #760: Control mode checks against the explicit script when invoked directly.
    for msg in wcs_rotation::explicit_script_problems(world, cps) {
        if !report.problems.contains(&msg) {
            report.problems.push(msg);
        }
    }
    // One file, one register per word (#196): two setups posting the same
    // word from different placements refuse here - per-op files are exempt.
    report.problems.extend(wcs_slot::shared_word_problems(world));
    if !report.problems.is_empty() {
        return Err(format!("post blocked - {}", report.problems.join("; ")));
    }
    let stream = cnc::serialize(&records);
    let id = document_program_id(world, 1)?;
    let mount_setup = mount
        .setups
        .first()
        .cloned()
        .or_else(|| crate::setup::active(world));
    let opts = program_options_for_script(world, cps, post, mount_setup.as_deref(), extra)?;
    // #2319: no mounted machine and a bad default file refuses by name.
    let mch = mounts::posted_machine_for(&machine)?;
    let out = engine::run_program_with(cps, &mch, &stream, &opts, &id)
        .map_err(|e| format!("post engine: {e}"))?;
    if !out.warnings.is_empty() {
        return Err(format!(
            "post blocked - {} warning(s): {}",
            out.warnings.len(),
            out.warnings.join("; ")
        ));
    }
    let ops = records
        .iter()
        .filter(|r| matches!(r, cnc::Record::Section { begin: true }))
        .count();
    let lines = out.program.lines().count();
    let line_pos = viewer_map(&records, &out);
    Ok(Posted {
        ops,
        lines,
        program: out.program,
        line_pos,
        blocks: out.blocks,
        not_judged: report.not_judged,
    })
}

/// Program line -> world position, by the stream's own laws: the dump
/// emits exactly one Move record per path move in chain order
/// ([`post::dump`]), so the k-th Move record is the k-th global move -
/// the same counter that seats cycles' `first_move` stream-global
/// above. Move coordinates are pre-origin-shift world millimeters (the
/// script applies the origin, M1.7b's law) - the very numbers the
/// viewport draws, so a highlighted line and a highlighted move cannot
/// disagree.
fn viewer_map(records: &[cnc::Record], out: &engine::PostOutput) -> Vec<Option<[f64; 3]>> {
    let moves: Vec<[f64; 3]> = records
        .iter()
        .filter_map(|r| match r {
            cnc::Record::Move(m) => Some([m.x, m.y, m.z]),
            // Probing (#108): each touch, retreat and protected move (#174)
            // stands for one move.
            cnc::Record::Touch(t) => Some(t.expect),
            cnc::Record::Retreat(t) => Some(t.expect),
            cnc::Record::Guard(g) => Some(g.to),
            _ => None,
        })
        .collect();
    let mut line_pos = vec![None; out.program.lines().count()];
    for info in &out.line_map {
        let Some(rec) = info.record.filter(|i| *i < records.len()) else {
            continue; // header lines carry no record; a broken map must not panic
        };
        let pos = match &records[rec] {
            cnc::Record::Move(m) => Some([m.x, m.y, m.z]),
            cnc::Record::Touch(t) => Some(t.expect),
            cnc::Record::Retreat(t) => Some(t.expect),
            cnc::Record::Guard(g) => Some(g.to),
            cnc::Record::Cycle(c) => moves.get(c.first_move).copied(),
            _ => None,
        };
        if let Some(slot) = line_pos.get_mut(info.line) {
            *slot = pos;
        }
    }
    line_pos
}

/// The program in posting order: every operation entity, document
/// order, validated the way posting validates - all generated, all
/// fresh, or a refusal naming the op. `stream_records` walks on it;
/// the M2.3 program sim walks on it too, so *what the sim judges is
/// what would post* holds by construction, not by two copies of the
/// law agreeing (stage D makes the agreement load-bearing).
pub fn program_ops(world: &World) -> Result<Vec<(String, Entity)>, String> {
    gated_ops(world, false).map(|(ops, _)| ops)
}

/// Operations by name, in program order.
pub(crate) type OpList = Vec<(String, Entity)>;

/// [`program_ops`] for the stock playback only (#2280): the same order
/// and the same gates, but the walk ends before the first operation
/// with no toolpath, which comes back with every op from it on. The
/// playback shows the program up to there; it never skips that op and
/// cuts the next ones on a stock the skipped op did not cut. The post
/// and the Deviate verdict keep [`program_ops`]: it refuses.
pub(crate) fn program_ops_until_ungenerated(world: &World) -> Result<(OpList, OpList), String> {
    gated_ops(world, true)
}

/// The walk under [`program_ops`]; `stop_at_ungenerated` turns the
/// "no toolpath yet" refusal into the end of the list.
fn gated_ops(world: &World, stop_at_ungenerated: bool) -> Result<(OpList, OpList), String> {
    // THE order law by call (M3.2 A): setup rank, then operation position - the same
    // walk re-machining, the sim and the machine-limit check make.
    // A suppressed op (#110) is not in the program: skipped before the
    // freshness law, so a stale op the user left out never holds the
    // post - and the program sim walks this same list, so what it
    // judges is still exactly what posts.
    let all: Vec<(String, Entity)> = crate::setup::operation_order(world);
    if all.is_empty() {
        return Err("no operations to post".into());
    }
    let mut ops: Vec<(String, Entity)> = all
        .into_iter()
        .filter(|(_, e)| !model::operation_of(world, e).is_some_and(|op| op.suppressed))
        .collect();
    if ops.is_empty() {
        return Err("no operations to post: every operation is suppressed".into());
    }

    // Validate early: every operation must be generated and fresh
    // before a single record is built (a half-posted program is the
    // silent wrong answer this file exists to prevent).
    // #2280: the playback ends before the first op with no toolpath -
    // nothing at or after it is cut, so their gates are not read.
    let rest = match ops.iter().position(|(_, e)| operation::status_of(world, e) == OpStatus::Never) {
        Some(i) if stop_at_ungenerated => ops.split_off(i),
        _ => Vec::new(),
    };
    for (name, e) in &ops {
        // M10.1 C first: an op whose tool is gone or cannot cut its kind
        // is blocked by name - "regenerate" would be the wrong advice,
        // since generation refuses it too - and a cache generated before
        // the tool went wrong never posts behind the warning.
        if let Some(why) = operation::tool_gate::tool_problem_of(world, e) {
            return Err(format!("{why} - fix the tool before posting"));
        }
        if let Some(why) = review_problem_of(world, name, e) {
            return Err(why);
        }
        // Issue #62 at the post, before freshness (like the tool gate:
        // "regenerate" alone is the wrong advice when generation would
        // refuse too): the post measures every diameter from where the
        // work offset is NOW, so the spindle law is read again against
        // that origin - a moved offset refuses by name, whatever the
        // stamp says.
        if let Some(why) = operation::spindle_law::post_problem(world, name, e) {
            return Err(why);
        }
        match operation::status_of(world, e) {
            OpStatus::Never => {
                return Err(format!("{name}: no toolpath yet - generate it first"))
            }
            OpStatus::Stale => {
                return Err(format!(
                    "{name}: inputs moved since generation - regenerate before posting"
                ))
            }
            OpStatus::Ok => {}
        }
    }
    Ok((ops, rest))
}

/// The program-level verification, made binding (M2.3 D): the two
/// judgments the Deviate cell informs about, read again here as the
/// release gate over exactly the op list `program_ops` just blessed -
/// the shared-gate law, so the verdict and the gate can never disagree
/// about WHICH program they judged. Every line already names its op;
/// a refusal that cannot say where is trivia, not a warning.
///
/// What blocks: a GOUGE (the program cuts below the model's final
/// geometry - the Prime Directive's "never", located); a stated machine
/// limit the program outruns; and a PROBE ask the program's own cuts
/// cannot measure to (M5.3 D): the part is what the program's blades
/// cut together, so the ask is judged on that composition, at the
/// post - the law and the lines in `operation::probe_problems`. What
/// does not: REST material on a program that asked no probe (a
/// roughing program posts with its promise of a later finish op -
/// Deviate shows the shortfall, the gate respects the intent; an
/// authored ask is what turns that promise into a fact), and a sim
/// that cannot run at all (no stock, rotated stock, a revolved path's
/// own frame, a rotary sector, a posed move) WHEN every op carries its
/// generator-time verify verdict - and then it is never silent: the
/// not-judged line rides [`GateReport::not_judged`] to every surface
/// that reports the post. A sim refusal with an op no generator
/// verdict covers blocks (see [`sim_clause`]).
#[must_use]
pub fn verification_gate(world: &World) -> Vec<String> {
    verification_report(world).problems
}

/// [`verification_gate`] with the stock sim's not-judged line beside
/// the problems - what the post, the script API and the status line
/// read, so the visible line and the binding gate are one run.
#[must_use]
pub fn verification_report(world: &World) -> GateReport {
    verification_report_by(world, &crate::setup_workholding::mounted_voices(world))
}

/// [`verification_report`] with the workholding clause (#1174) judged
/// for the post scripts that will actually run: `voice` names the
/// dialect of the script each operation's setup context posts through
/// (an explicit script: the same for every op).
#[must_use]
pub fn verification_report_by(world: &World, voice: &dyn Fn(Option<&str>) -> post::dialect::WorkVoice) -> GateReport {
    let mut problems = Vec::new();

    // The fidelity clause (#864), first: an operation judged against a
    // model imported too coarsely to verify is refused by name - the sim
    // below then refuses to judge it too, so no gouge verdict is read off
    // a mesh that cannot carry one.
    problems.extend(crate::model_fidelity::gate_problems(world));
    problems.extend(review_problems(world));

    let sim = sim_clause(world);
    problems.extend(sim.problem);
    // The plasma clause (#672): the 3D stock sim carves nothing for a
    // torch, so a plasma op's verdict is the 2D through-cut laws, run
    // here - a refusal blocks, and the not-judged line says the sim did
    // not run for it, never a silent skip.
    let plasma = plasma_clause(world);
    problems.extend(plasma.problems);
    // The wire clause (#534): likewise for a wire EDM cut - the 2.5D
    // kerf, threading and slug laws, every finding binding.
    let wire = wire_clause(world);
    problems.extend(wire.problems);

    // The suppression clause (#110 review B1): an op generated against
    // a suppressed op's cut refuses by name - the cut it assumed is not
    // in the program.
    problems.extend(suppressed::suppressed_problems(world));
    // The rotated-offset clause (#315): the post carries translation only.
    problems.extend(wcs_rotation::rotated_wcs_problems(world));
    // #1649: a flipped axis is carried by the post (X, Y); what is not
    // carried - a flipped Z, a rotated or unverified case - is refused.
    problems.extend(crate::setup_wcs_flip::post_problems(world));
    // The offset-range clause (#931): each setup's offset index against
    // its own post's maximum - never posted at offset 1.
    problems.extend(crate::setup_offsets::range_problems(world));
    // #389: the setups' post options must agree on what shapes the whole
    // program (the Output, the thread output and, #1540, the End units) -
    // never posted half one way. Judged program-wide, ahead of #388's
    // per-mount split.
    problems.extend(crate::setup_post_options::program_problems(world));
    // #1031/#1174: the work spindle over a setup's workholding maximum,
    // by name - judged for the post dialect each op runs through.
    problems.extend(crate::setup_workholding::program_problems_by(world, voice));
    // The carried-stock clause (#319): a feed move into carried material
    // outside a setup's declared stock is engagement nothing verified.
    problems.extend(crate::operation::carried_problems(world));

    // The limit check (M2.3 D), each op against the machine its own
    // setup mounts (#388): one block per machine, the program's order.
    problems.extend(mounts::machine_problems(world));
    // #804: before a work-turning A indexes, the retracted tool tip
    // clears what the stock, parts and turning fixtures sweep about it.
    problems.extend(index_swing::index_swing_problems(world));
    // #1107: ... and the work it sweeps clears the face the rotary
    // stands on and the fixed fixtures over its length.
    let standing = swing_standing::standing_problems(world);
    problems.extend(standing.problems);

    // The workholding clause (M3.2 B, live-gate law): the same fixture
    // bands and rapid laws generation reports early, re-run HERE against
    // the paths as cached and the steel as it stands now. A vise slid
    // under a programmed tool after generation - or a machine file that
    // just grew a table the paths were never checked against - blocks
    // the post without staling a single stamp (the M2.3 machine-limits
    // precedent: what the room is made of is verified now, never baked
    // into a digest).
    problems.extend(fixture_problems(world));
    // #321: a carried setup's fixtures against the body it is entered
    // with - workholding modelled into material the program carries in.
    problems.extend(crate::fixture_carried::carried_fixture_problems(world));

    // The in-process measurement ask (M5.3 D): a Part op's probe
    // budget is a claim about the FINISHED part, and the finished
    // part is the program's blades composed - so it is judged here,
    // against every blade leg's cached path, not at any single
    // operation's generation (a rough's left floor is a fact about
    // what came after it). The law and lines live in
    // `operation::probe_problems`.
    problems.extend(crate::operation::probe_problems(world));
    let plasma_line = match (plasma.line, wire.line) {
        (Some(p), Some(w)) => Some(format!("{p}; {w}")),
        (p, w) => p.or(w),
    };
    let not_judged = match (sim.not_judged, plasma_line) {
        (Some(s), Some(p)) => Some(format!("{s}; {p}")),
        (None, Some(p)) => Some(format!("{NOT_JUDGED}: {p}")),
        (s, None) => s,
    };
    // #1107: standing steel the swing clause could not read, named.
    let swing = (!standing.not_judged.is_empty())
        .then(|| format!("{}: {}", swing_standing::SWING_NOT_JUDGED, standing.not_judged.join("; ")));
    let not_judged = match (not_judged, swing) {
        (Some(s), Some(w)) => Some(format!("{s}; {w}")),
        (s, w) => s.or(w),
    };
    GateReport { problems, not_judged }
}

/// The gate's fixture walk: one problem string per operation with a
/// live collision, naming the op, the move, and the struck element.
/// Ops that lost their tool or path mid-session are skipped here -
/// `stream_records` refuses those by name before this ever releases
/// metal; the gate's job is the steel, not the bookkeeping.
fn fixture_problems(world: &World) -> Vec<String> {
    let Ok(f) = fixture_findings(world) else {
        return Vec::new();
    };
    let hits = f.hits.into_iter().map(|(name, hit)| format!("{name}: {hit}"));
    f.unjudged.into_iter().chain(hits).collect()
}

/// Review refusal for one operation whose migration dropped un-carriable parameters (#1517).
pub(crate) fn review_problem_of(world: &World, key: &str, entity: &Entity) -> Option<String> {
    let meta = model::operation_meta_of(world, entity);
    if meta.needs_review.is_empty() {
        return None;
    }
    let label = meta.label.unwrap_or_else(|| key.to_owned());
    Some(format!(
        "{label}: needs review before posting ({}) - inspect the operation and mark it reviewed",
        meta.needs_review.join("; ")
    ))
}

/// Every unsuppressed operation still carrying migration `needs_review` notes (#1517).
fn review_problems(world: &World) -> Vec<String> {
    crate::setup::operation_order(world)
        .into_iter()
        .filter(|(_, e)| !model::operation_of(world, e).is_some_and(|op| op.suppressed))
        .filter_map(|(name, e)| review_problem_of(world, &name, &e))
        .collect()
}

/// The fixture walk's verdict as data (#181), read by both the gate's
/// strings and the MCP `get_fixture_findings` tool.
#[derive(Debug, Default)]
pub struct FixtureFindings {
    /// `(operation key, warning)` per live collision, in program order.
    pub hits: Vec<(String, crate::verify::Warning)>,
    /// Operations the walk could not judge, by name and reason (#368: a
    /// modelled tool body that no longer fits its tool) - each a problem
    /// in its own right, never read as clear.
    pub unjudged: Vec<String>,
}

/// The fixture walk (#181). `Err` is the program-order refusal (a stale
/// op): no verdict, never "clear".
pub fn fixture_findings(world: &World) -> Result<FixtureFindings, String> {
    let mut out = FixtureFindings::default();
    let ops = program_ops(world)?;
    let mut mesh_cache: std::collections::HashMap<Option<String>, Vec<crate::fixture::MeshedFixture>> =
        std::collections::HashMap::new();
    for (name, e) in ops {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        let Some(cache) = model::toolpath_of(world, &e) else {
            continue;
        };
        // Split by whether the steel turns with the part (M13.8 (2)):
        // fixed boxes are judged in the machine frame where the op's
        // rotary axis is known, in the path frame otherwise.
        let (turning, fixed) = crate::fixture::workholding_for(world, op.setup.as_deref());
        // #339: the tagged models' meshes, indexed once per setup and
        // shared by every op in it.
        let meshes = mesh_cache
            .entry(crate::setup::effective(world, op.setup.as_deref()))
            .or_insert_with(|| crate::fixture::meshes_for(world, op.setup.as_deref()));
        // The one per-op law, shared with the viewport's struck-fixture
        // view (#339 slice 4) so the two can never disagree. Its `Err`
        // is #368's tool body that no longer fits its tool: unjudged,
        // by name, never read as clear.
        match crate::fixture_struck::op_hits(world, &op, &cache.path, &turning, &fixed, meshes) {
            Ok(hits) => out.hits.extend(hits.into_iter().map(|hit| (name.clone(), hit))),
            Err(why) => out.unjudged.push(format!("{name}: {why}")),
        }
    }
    for meshes in mesh_cache.values() {
        crate::fixture::log_mesh_refusals(meshes);
    }
    Ok(out)
}

/// The stream we would feed the engine: every operation's records,
/// in document order, validated first. Split from the write so the
/// gate's evidence (WARN records, cycle anchors) is testable without
/// touching the filesystem or the script. Public (M13.5) as the
/// dialect-golden seam: a test posts the document's own stream
/// through one script without the verification gate, which the
/// posting suites lock separately - the golden locks the translation.
pub fn stream_records(world: &World) -> Result<Vec<cnc::Record>, String> {
    mounts::refuse_per_op(world, "one combined stream")?;
    Ok(join_streams(op_streams(world)?))
}

/// Join per-op streams into one program's records, cycle anchors made
/// stream-global - [`stream_records`]' join, and one mount's file's
/// (#388) over its own ops.
pub(crate) fn join_streams(streams: Vec<(String, Vec<cnc::Record>)>) -> Vec<cnc::Record> {
    let mut records: Vec<cnc::Record> = Vec::new();
    let mut moved = 0usize; // cycle AT anchors are stream-global
    for (_name, mut recs) in streams {
        if moved > 0 {
            for r in &mut recs {
                if let cnc::Record::Cycle(c) = r {
                    c.first_move += moved;
                }
            }
        }
        // The emitted motion records, not the path's moves: a
        // control-compensated span (M13.11) writes its programmed
        // contour in place of its centre moves.
        moved += recs
            .iter()
            .filter(|r| matches!(r, cnc::Record::Move(_) | cnc::Record::Touch(_) | cnc::Record::Retreat(_) | cnc::Record::Guard(_)))
            .count();
        records.extend(recs);
    }
    records
}

/// Each program operation's own records (op entity name, records), in
/// program order, cycle anchors local to the op - the unit one file per
/// operation (#111) posts on its own, and [`stream_records`] joins.
pub fn op_streams(world: &World) -> Result<Vec<(String, Vec<cnc::Record>)>, String> {
    op_streams_by(world, true)
}

/// [`op_streams`], with the setup-change stops (#1532) placed for files
/// grouped as `setup_mount::program_mounts(world, by_post)` groups them:
/// `false` for an explicit script, which posts every post's setups of a
/// machine in one file.
pub(crate) fn op_streams_by(world: &World, by_post: bool) -> Result<Vec<(String, Vec<cnc::Record>)>, String> {
    let ops = program_ops(world)?;

    // The T../H.. law (#221): a tool's own number when it has one, else
    // its 1-based position among the named tools in document order - and
    // two tools of one machine posting the same number refuse by name.
    let tool_numbers = crate::tool_numbers::numbers(world);
    // Judged per machine (#438): two machines are two tool tables.
    if let Some(msg) = crate::tool_numbers::clash_per_machine(world, &ops) {
        return Err(msg);
    }

    // #530: the family door again, at post (`posting_family`), and
    // #532/#533's law on tool-less kinds: only the Wire Contour posts.
    crate::posting_family::family_at_post(world, &ops)?;

    // #155: Pause / Home entries fold into the gap they stand in; #389:
    // "Skip pauses and homes" is read per op, from its own setup.
    let mut afters = crate::program_entry::posted_afters(world, &ops, |e| {
        model::operation_of(world, e)
            .is_some_and(|op| crate::setup_post_options::of(world, op.setup.as_deref()).skip_stops)
    })?;
    // #1532: a setup change stops for the re-clamp (by default).
    let changes = setup_change::fold(world, &ops, &mut afters, by_post)?;
    let mut streams = Vec::with_capacity(ops.len());
    for (name, e) in &ops {
        let op = model::operation_of(world, e)
            .ok_or_else(|| format!("{name}: lost its operation"))?;
        let cache = model::toolpath_of(world, e)
            .ok_or_else(|| format!("{name}: lost its toolpath"))?;
        if let OpParams::WireContour(p) = &op.params {
            let after = afters.get(name).copied().unwrap_or_default();
            let mut records = wire::stream(world, name, e, &op, &cache, p, after)?;
            setup_change::ask(&changes, name, &mut records);
            streams.push((name.clone(), records));
            continue;
        }
        let tool = model::find_by_name(world, &op.tool)
            .and_then(|t| model::tool_of(world, &t))
            .ok_or_else(|| format!("{name}: tool {} is not in the library", op.tool))?;
        let setup = op.setup.as_deref();
        let hs = operation::resolve_params(world, setup, Some(&op.src), &op.params)
            .map_err(|err| format!("{name}: {err}"))?;
        let tool_number = tool_numbers
            .iter()
            .find(|(n, _)| *n == op.tool)
            .map_or(0, |(_, t)| *t);
        let (wcs, origin) = wcs_context(world, setup);
        // The stock box is THIS operation's setup context (M3.2 A): a
        // later setup's block is where and how tall its own stock says.
        let boxz = model::stock_world_aabb_for(world, setup);
        let (stock, stock_top_z) = match &boxz {
            Some(a) => ([a.min.x, a.max.x, a.min.y, a.max.y], a.max.z),
            None => ([0.0; 4], 0.0),
        };
        // #1666: a turning machine's stream states the radius the stock
        // sweeps about the spindle axis, so a square bar's corners are
        // known to the post; a mill's stream stays as it was.
        let stock_turn_radius = matches!(
            crate::machine::kind_of_setup(world, setup),
            crate::machine::Kind::Lathe | crate::machine::Kind::MillTurn
        )
        .then(|| crate::stock_turn::turn_radius_for(world, setup, [origin[0], origin[1]]))
        .flatten();
        // The feeder signals ride the section (M5.2 G.2): only a
        // Part operation can carry them, and a load with an advance
        // of 0 is still a load - the ACT, not the number. The
        // handoff step joins the family (M5.3 C) - same extraction,
        // same law: a signal, no geometry, the gate says whether the
        // machine states the second spindle it names. The
        // measurement ask joins last (M5.3 D) with its own shape:
        // the NUMBER is the switch - an authored budget above 0
        // asks the probe, 0 is silence, so no second flag exists to
        // disagree with it.
        let (part_load, part_unload, part_transfer, probe_tol) = match &op.params {
            OpParams::Part {
                bar_load,
                bar_advance,
                bar_unload,
                bar_transfer,
                inspect_tol,
                ..
            } => (
                bar_load.then_some(*bar_advance),
                *bar_unload,
                *bar_transfer,
                (*inspect_tol > 0.0).then_some(*inspect_tol),
            ),
            _ => (None, false, false, None),
        };
        let display_name = model::operation_label(world, name, e);
        // The probe (#108): its result rides the section, and a touch
        // probe states no spindle (the preamble writes none for 0).
        let probe_result = match &op.params {
            OpParams::Probe { set_wcs, tolerance, .. } => Some(post::probe_rec::ProbeResult {
                set_wcs: *set_wcs,
                tol_mm: *tolerance,
            }),
            _ => None,
        };
        let rpm = posted_rpm(&tool);
        // Commentary for the program (#257): kinds by their panel
        // labels, the setup by the resolution law (implicit members
        // name the first setup, as the setup sheet does).
        let op_kind = op.params.kind().to_string();
        let tool_kind = tool.kind.to_string();
        let setup_name = crate::setup::effective(world, setup).unwrap_or_default();
        // #630: a nose-centre turning path posts as the tool's control
        // point for its orientation (or is refused by name when the
        // orientation cannot cut it); the cache stays the nose truth.
        let (posted, tool_orientation) = nose::posted_path(&op.params, &op.tool, &tool, &cache.path, origin)
            .map_err(|e| format!("{display_name}: {e}"))?;
        let d = Dump {
            op_kind: &op_kind,
            setup_name: &setup_name,
            tool_kind: &tool_kind,
            tool_corner_r: tool.corner_r,
            tool_orientation,
            probe_result,
            op_name: &display_name,
            part_load,
            part_unload,
            part_transfer,
            probe_tol,
            // #616: the op's smoothing ask, only when its OWN setup's
            // post options turn smoothing on (off: byte-identical output) -
            // read per op like "Skip pauses and homes" (#389), never from
            // whichever setup happens to be active.
            smoothing: if crate::setup_post_options::of(world, op.setup.as_deref()).smoothing {
                crate::smoothing::of(&op.params)
            } else {
                None
            },
            // #891: a turning op whose tool states turning data asks
            // for constant surface speed; everything else posts G97.
            // #1031: under the setup's workholding rpm limit too.
            css: crate::setup_workholding::css_of(world, setup, crate::cutting_turn::css_of(&op.params, &tool)),
            coolant: op.coolant,
            // "Skip pauses and homes" (#110) mutes every after-action
            // at post; the ops keep their choices. Entries ride it (#155).
            after: afters.get(name).copied().unwrap_or_default(),
            tool_number,
            tool_name: &op.tool,
            tool_diameter: tool.diameter,
            rpm,
            clearance_z: hs.clearance,
            retract_z: hs.retract.unwrap_or(hs.clearance),
            top_z: stock_top_z, // the TopOfStock PARAM: the stock, not the cut
            wcs,
            origin,
            wcs_yaw: wcs_yaw(world, setup),
            wcs_flip: crate::setup_wcs_flip::of_setup(world, setup),
            wcs_rotation_control: wcs_yaw(world, setup).is_some()
                && crate::setup_post_options::of(world, op.setup.as_deref()).wcs_rotation
                    == crate::schema::WcsRotationOutput::Control,
            stock,
            stock_top_z,
            stock_turn_radius,
            // The scallop reading (M13.9) is a quality note: shown on
            // the op, never a WARN record - a WARN record holds the
            // post, and a scallop is neither a gouge nor a collision.
            warnings: cache
                .warnings
                .iter()
                .filter(|w| w.blocks_post())
                .map(|w| format!("{name}: {w}"))
                .collect(),
        };
        let mut records = post::dump(&posted, &d);
        // #542: a sinker burn's condition and pickup ride the section.
        edm_params::insert(&op.params, &tool, &mut records);
        // #988: a left-hand-cut thread mill turns M4.
        if let OpParams::Thread { passes, .. } = &op.params
            && passes.spindle_reverse
        {
            post::ask_reverse_spindle(&mut records);
        }
        setup_change::ask(&changes, name, &mut records);
        section_mode::insert(&op.params, &posted, &mut records);
        streams.push((name.clone(), records));
    }
    Ok(streams)
}

/// The work offset an operation of the `setup` context posts to: the
/// setup's 1-based offset index (#196, #342, #931: its label, else 1)
/// and the setup's one WCS origin as the machine would measure it.
/// `pub(crate)` so the machine-limit travel check (M2.3 C) reads program
/// coordinates by exactly this law, not by a second copy of it.
///
/// The stream carries the INDEX; the post writes its own word for it. An
/// index the post cannot express is never read as offset 1 here - the
/// gate refuses it by setup name ([`crate::setup_offsets::range_problems`]).
/// The ENTITY the origin is measured from is the setup's WCS (M3.2 A,
/// #342), whatever offset it is labelled.
pub(crate) fn wcs_context(world: &World, setup: Option<&str>) -> (u32, [f64; 3]) {
    let index = crate::setup_offsets::index(world, setup);
    let origin = crate::setup::wcs_entity(world, setup)
        .and_then(|e| model::transform_of(world, &e))
        .map_or([0.0; 3], |t| [t.pos.x, t.pos.y, t.pos.z]);
    (index, origin)
}

/// #760: signed Z-axis rotation in degrees (`yaw_deg`) for a setup's
/// WCS when WCS `+Z` stays aligned with machine `+Z` and `|yaw| > 1e-9`
/// rad; `None` when square, tilted, or non-finite.
#[must_use]
pub(crate) fn wcs_yaw(world: &World, setup: Option<&str>) -> Option<f64> {
    let loc = crate::setup::wcs_entity(world, setup).and_then(|e| model::transform_of(world, &e))?;
    match offset_frame(&loc) {
        OffsetFrame::Yaw { yaw_deg, .. } => Some(yaw_deg),
        _ => None,
    }
}

/// Every height a parameter set references (the same walk
/// `resolve_params` makes, but of the references themselves).
pub trait HeightRefs {
    fn heights(&self) -> Vec<&Height>;
}

impl HeightRefs for OpParams {
    fn heights(&self) -> Vec<&Height> {
        match self {
            OpParams::Probe { clearance_z, .. } => vec![clearance_z],
            OpParams::PlasmaProfile(p) => vec![&p.clearance_z, &p.top_z, &p.bottom_z],
            OpParams::WireContour(p) => vec![&p.top_z, &p.bottom_z],
            OpParams::SinkerBurn(p) | OpParams::SinkerSideBurn(crate::operation::SinkerSideBurnOp { burn: p, .. }) => vec![&p.clearance_z, &p.top_z, &p.floor_z],
            OpParams::Face {
                clearance_z,
                cut_z,
                top_z,
                ..
            } => {
                let mut v = vec![clearance_z, cut_z];
                v.extend(top_z.iter());
                v
            }
            OpParams::Contour {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            } => vec![clearance_z, retract_z, top_z, bottom_z],
            OpParams::Pocket {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            // The twins reference the pocket's four heights (M2.4 A).
            | OpParams::Adaptive2D {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::Offset2D {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::Slot {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::TrochoidalSlot {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::Trace {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            // The engrave twin states trace's four verbatim, and the
            // circular ladder is the pocket family's shape - vault or
            // flat floors, both read top and bottom (M2.4 B).
            | OpParams::Engrave {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::Circular {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::Pocket3D {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::Finish3D {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            // The 5-axis twins carry the finish's height shape one for
            // one (M5.1 stage C) - the optional hop included.
            | OpParams::Contour5D {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::Parallel5D {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            // ...and the swarf family's (M5.1 stage D).
            | OpParams::Swarf {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::AdvancedSwarf {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            // The rotary twins' shape is the same four world planes;
            // the generator reads them into axial stations itself
            // (M5.1 stage E), so the post sees the finish's law.
            | OpParams::RotaryContour {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::RotaryParallel {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            // The stamped ladder's heights are the pocket family's
            // four, hop included (M5.1 stage E3), and flow states
            // the same four (M5.1 I).
            | OpParams::MultiAxisRough {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::Flow5D {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            // The drive-curve contour states the same four (M5.1 J).
            | OpParams::MultiAxisContour {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            } => {
                let mut v = vec![clearance_z, top_z, bottom_z];
                v.extend(retract_z.iter());
                v
            }
            // The chamfer ride resolves three: its floor is DERIVED
            // from width and angle, so there is no bottom plane to
            // reference - the top plane is the only height the ride
            // reads (M2.4 B, variant docs).
            OpParams::Chamfer2D {
                clearance_z,
                retract_z,
                top_z,
                ..
            } => {
                let mut v = vec![clearance_z, top_z];
                v.extend(retract_z.iter());
                v
            }
            // The deburr ride resolves three too, the finish's shape
            // without the hop: the kernel returns to clearance
            // between runs itself, and the window's two planes clip
            // the loci (M2.5 E B).
            OpParams::Deburr {
                clearance_z,
                top_z,
                bottom_z,
                ..
            } => vec![clearance_z, top_z, bottom_z],
            // The wrap ladder's three - no hop plane exists to ask
            // about; the kernel's rapids ride the skin, not a height
            // (M5.1 stage E2).
            OpParams::RotaryOffset {
                clearance_z,
                top_z,
                bottom_z,
                ..
            }
            // The facing ladder states the same three: the station
            // rapids ride the profile, not a hop height (M5.2 A) -
            // and so do the profile rungs, whose rapids clear the
            // ride's reach (M5.2 B).
            | OpParams::Facing { clearance_z, top_z, bottom_z, .. }
            | OpParams::ProfileRough { clearance_z, top_z, bottom_z, .. }
            | OpParams::ProfileFinish { clearance_z, top_z, bottom_z, .. }
            // The blade kinds carry the same three heights - the
            // parting plane is one of them, spelled `bottom_z`
            // (M5.2 C).
            | OpParams::GrooveRough { clearance_z, top_z, bottom_z, .. }
            | OpParams::GrooveFinish { clearance_z, top_z, bottom_z, .. }
            // Threading carries the same three - the rapids plane
            // and the window of turns (M5.2 D) - and so do the
            // contour rides: the plane and the face's two stations
            // (M5.2 E).
            | OpParams::Part { clearance_z, top_z, bottom_z, .. }
            | OpParams::Threading { clearance_z, top_z, bottom_z, .. }
            | OpParams::Chamfering { clearance_z, top_z, bottom_z, .. }
            | OpParams::TaperTurning { clearance_z, top_z, bottom_z, .. }
            | OpParams::MultiPassTurning { clearance_z, top_z, bottom_z, .. }
            // The crown gauge rides the same trio (M5.2 E2), and so
            // does the boring bar (M5.2 F).
            | OpParams::FormTurning { clearance_z, top_z, bottom_z, .. }
            | OpParams::BoringTurning { clearance_z, top_z, bottom_z, .. } =>
                vec![clearance_z, top_z, bottom_z],
            // Bore and Thread reference the drill's four (M2.4 C) -
            // the same optional-floor, optional-hop shape.
            OpParams::Drill {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::Bore {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::Thread {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            } => {
                let mut v = vec![clearance_z, top_z];
                v.extend(retract_z.iter());
                v.extend(bottom_z.iter());
                v
            }
        }
    }
}
