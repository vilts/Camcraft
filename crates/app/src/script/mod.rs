//! The job-script host (M5.3 F): the batch lane's door - a script owns
//! a document, runs the verbs, and gets structured JSON back.
//!
//! Provenance: specced from this repo's own ROADMAP line ("Typescript
//! bindings for batch ops"); the engine is the same hard-pinned boa
//! the post uses, and nothing here adapts any vendor's scripting
//! surface. Clean-room, public sources only.
//!
//! Law of the seam: a script is a COMMAND EXECUTANT, not a UI. Every
//! world write a verb makes rides a real command through a real
//! History, so the batch lane and the app agree on what "generate"
//! means - the Prime Directive has one shape everywhere. What a
//! refusal looks like: a JS `Error` whose text names the reason,
//! catchable in the script, and recorded in the run's `errors` even
//! when caught - a job that swallowed its refusals is not a clean job.
//!
//! The bridge across the engine's edge is JSON text, both directions:
//! one host function `__cam_call(name, argsJson)` returns an envelope
//! `{ok, value} | {error}`, and the `cam` object is a shim the verb
//! table itself writes. This is js.rs's doctrine worn at a wider door:
//! boa's host closures cannot capture an environment (only `Copy`
//! captures are sound, and an `Env` is neither `Copy` nor
//! GC-traceable), so the closure carries one raw pointer to the run's
//! `RefCell<Env>` and all data crosses as serialized text, never as a
//! handle. The pointer is valid by ownership order, not by luck:
//! `run` declares the `Rc<RefCell<Env>>` before the `Context`, so the
//! context and every function registered inside it is dropped first.

pub(crate) mod api;
pub use api::refusal_text;
mod report;

use crate::commands::History;
use crate::world;

use boa_engine::{Context, JsError, JsString, JsValue, NativeFunction, Source};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

/// A run's finished report: whether it stood up, what the script
/// printed, and what the last expression's value was (the REPL shape,
/// on purpose - a trailing `cam.report()` is the log of a job).
#[derive(Debug)]
pub struct ScriptOutcome {
    pub ok: bool,
    /// `null` where the script's last expression had no JSON shape.
    pub result: Value,
    pub logs: Vec<String>,
    /// Refusal text in engine order: a boot or eval error first, then
    /// what the verbs refused. A script that caught its errors still
    /// posts those lines here even if it ended well - the record is
    /// the host's, not the script's.
    pub errors: Vec<String>,
}

/// One script run's whole world: the document, its command history,
/// and the console the script writes to. Owned by the run - a batch
/// job gets a fresh world per script, which is the same isolation the
/// app's Open gives (the loader clears it by law; M1.2).
pub(crate) struct Env {
    pub world: flecs_ecs::prelude::World,
    pub history: History,
    pub logs: Vec<String>,
    pub errors: Vec<String>,
    /// The document file a `cam.load` opened, or a `cam.save` wrote to
    /// - what a bare `cam.save()` means the way the app's Ctrl+S does.
    pub doc_path: Option<PathBuf>,
    pub posted: Option<PathBuf>,
    /// The last verb result, parked for the shapes the host cannot
    /// pull (`cam.result()`, and the non-string read of a parked
    /// object). One value pushed into the sandbox on demand.
    pub last_value: Value,
}

/// The `cam` methods, as data: name, arity, TypeScript line, and the
/// Rust verb. The table is what both the JS shim and the bindings
/// file are generated FROM, so a script can never see a method the
/// header does not name, nor a header line the runtime does not back.
pub(crate) const VERBS: &[Verb] = &[
    Verb::new("load", 1, api::v_load, "load(path: string): DocumentInfo"),
    Verb::new("save", -1, api::v_save, "save(path?: string): { path: string }"),
    Verb::new("import", -1, api::v_import, "import(path: string, name?: string | null, facetBudget?: number): { model: string; facets: number; facet_budget?: number }"),
    Verb::new("exportTools", 1, api::v_export_tools, "exportTools(path: string): { path: string; tools: number }"),
    Verb::new("importTools", -1, api::v_import_tools, "importTools(path: string, mode?: \"add\" | \"update\" | \"replace\", holders?: boolean): { path: string; mode: string; added: string[]; renamed: { from: string; to: string }[]; skipped: string[]; updated: string[]; replaced: string[]; removed: string[]; holders: number; holders_left_out: number; summary: string }"),
    Verb::new("operations", 0, api::v_operations, "operations(): OperationInfo[]"),
    Verb::new("generate", -1, api::v_generate, "generate(...names: string[]): OperationInfo[]"),
    Verb::new("verify", -1, api::v_verify, "verify(...names: string[]): VerifyReport"),
    Verb::new("post", 1, api::v_post, "post(options: PostOptions): PostResult"),
    Verb::new("report", 0, api::v_report, "report(): JobReport"),
    Verb::new("check", 0, api::v_check, "check(): VerifyReport"),
    Verb::new("result", 0, api::v_result, "result(): unknown"),
    Verb::new("log", -1, api::v_log, "log(...values: unknown[]): null"),
];

pub(crate) type VerbFn = fn(&mut Env, Vec<Value>) -> Result<Value, String>;

pub(crate) struct Verb {
    pub name: &'static str,
    /// Fixed count, or -1 for variadic.
    pub arity: i32,
    /// The member line of the generated header, without its semicolon.
    pub ts: &'static str,
    pub f: VerbFn,
}

impl Env {
    /// A fresh document and an empty history - one per run, or one per
    /// MCP session (#128), which drives the same verbs without a script.
    pub(crate) fn fresh() -> Self {
        Self {
            world: world::new_world(),
            history: History::default(),
            logs: Vec::new(),
            errors: Vec::new(),
            doc_path: None,
            posted: None,
            last_value: Value::Null,
        }
    }
}

impl Verb {
    pub(crate) const fn new(name: &'static str, arity: i32, f: VerbFn, ts: &'static str) -> Self {
        Self { name, arity, ts, f }
    }
}

/// Run one job script in a fresh world. Never a panic: a bad source,
/// a thrown error, and a refused verb all come back in `errors`, and
/// the world they touched is freed here, not carried out.
#[must_use]
pub fn run(source: &str) -> ScriptOutcome {
    // Declaration order IS the lifetime law: env outlives ctx, so the
    // raw pointer the host closure carries is never dangling.
    let env = Rc::new(RefCell::new(Env::fresh()));
    let raw = Rc::as_ptr(&env);
    let mut ctx = match boot(raw) {
        Ok(ctx) => ctx,
        Err(e) => return outcome(&env, Value::Null, Err(e)),
    };
    match ctx.eval(Source::from_bytes(source.as_bytes())) {
        Ok(v) => {
            let out = v.to_json(&mut ctx).ok().flatten().unwrap_or(Value::Null);
            outcome(&env, out, Ok(()))
        }
        Err(e) => outcome(&env, Value::Null, Err(engine_text(&e))),
    }
}

/// The door plus the shim: `__cam_call` on the raw pointer, then the
/// `cam` object the VERBS table writes. The shim source is generated,
/// never hand-typed - one table, two faces.
fn boot(raw: *const RefCell<Env>) -> Result<Context, String> {
    // The capture is one raw pointer - `Copy`, which is exactly what
    // `from_copy_closure` demands (and what makes it sound: no
    // GC-traceable type lives inside, Env holds a world, Rust
    // commands, and JSON, never a JsValue). The pointer's referent is
    // the `Rc` run() declared before the `Context`, so this context -
    // and every function registered inside it - is dropped first;
    // dispatch carries the deref's own SAFETY note.
    let nf = NativeFunction::from_copy_closure(move |_this, args, _ctx| {
        Ok(JsValue::from(JsString::from(dispatch(raw, args))))
    });
    let mut ctx = Context::default();
    ctx.register_global_callable(JsString::from("__cam_call"), 2, nf)
        .map_err(|e| format!("host boot: {e}"))?;
    ctx.eval(Source::from_bytes(shim().as_bytes()))
        .map_err(|e| format!("host boot: {}", engine_text(&e)))?;
    Ok(ctx)
}

/// The JS side of the table: every verb one line, identical shape.
fn shim() -> String {
    let mut s = String::from(
        "var cam = (function () {\n \"use strict\";\n\
         function invoke(name, args) {\n\
         \x20 var env = JSON.parse(String(__cam_call(name, JSON.stringify(args))));\n\
         \x20 if (!env.ok) { throw new Error(env.error); }\n\
         \x20 return env.value;\n\
         }\n var api = {};\n",
    );
    for v in VERBS {
        s.push_str(&format!(
            " api.{0} = function () {{ return invoke(\"{0}\", [].slice.call(arguments)); }};\n",
            v.name
        ));
    }
    s.push_str(" return api;\n})();");
    s
}

/// One door call: parse, check arity, run, envelope. Every failure
/// here is a refusal, never a panic, and a refusal is recorded in
/// `env.errors` even when the script catches the thrown Error.
fn dispatch(raw: *const RefCell<Env>, args: &[JsValue]) -> String {
    // SAFETY: `raw` is valid for the whole life of any context that
    // can call this function (boot's ownership-order law); by the time
    // the Rc frees, no context that holds this closure is alive.
    let cell = unsafe { &*raw };
    let Some(name) = args.first().and_then(|v| v.as_string()) else {
        return err_envelope("host door: the verb name must be a string");
    };
    let name = name.to_std_string_lossy();
    let text = args
        .get(1)
        .and_then(|v| v.as_string())
        .map(|s| s.to_std_string_lossy())
        .unwrap_or_else(|| "[]".to_owned());
    let Some(verb) = VERBS.iter().find(|v| v.name == name) else {
        return err_envelope(format!("unknown cam method {name}"));
    };
    let list: Vec<Value> = match serde_json::from_str(&text) {
        Ok(Value::Array(a)) => a,
        Ok(_) => return err_envelope(format!("{name}: the argument pack must be an array")),
        Err(e) => return err_envelope(format!("{name}: argument pack unreadable: {e}")),
    };
    let Ok(mut env) = cell.try_borrow_mut() else {
        // Re-entrance a script must never see: a verb that called
        // back into JS while the host held the borrow. Nothing does
        // today; if one ever did, this names it instead of panicking.
        return err_envelope(format!("{name}: the host is already inside a verb"));
    };
    // An arity break is a refusal like any other: recorded even when
    // the script catches it, because the job did not run as typed.
    if verb.arity >= 0 && list.len() != verb.arity as usize {
        let line = format!("{name}: takes {} argument(s), got {}", verb.arity, list.len());
        env.errors.push(line.clone());
        return err_envelope(line);
    }
    match (verb.f)(&mut env, list) {
        Ok(v) => {
            env.last_value = v.clone();
            drop(env);
            ok_envelope(v)
        }
        Err(msg) => {
            let line = format!("{name}: {msg}");
            env.errors.push(line.clone());
            drop(env);
            err_envelope(line)
        }
    }
}

fn ok_envelope(v: Value) -> String {
    json!({ "ok": true, "value": v }).to_string()
}

fn err_envelope(msg: impl std::fmt::Display) -> String {
    json!({ "ok": false, "error": msg.to_string() }).to_string()
}

fn outcome(env: &Rc<RefCell<Env>>, result: Value, eval: Result<(), String>) -> ScriptOutcome {
    let mut e = env.borrow_mut();
    if let Err(text) = eval {
        // An uncaught refusal already rode the errors list from the
        // dispatcher; the engine re-presenting it (decorated with the
        // Error: prefix and a place) is one event, not two.
        if !matches!(e.errors.last(), Some(last) if text.contains(last)) {
            e.errors.insert(0, text);
        }
    }
    let errors = std::mem::take(&mut e.errors);
    let ok = errors.is_empty();
    let logs = std::mem::take(&mut e.logs);
    drop(e);
    ScriptOutcome { ok, result, logs, errors }
}

/// The engine's error text through its Display: the kind, the
/// message, and the place when boa has one - its parse and throw
/// diagnostics carry "at line N, col M" for most failures (a refusal
/// that cannot say where is trivia, not a diagnostic).
fn engine_text(e: &JsError) -> String {
    e.to_string()
}

/// The TypeScript header the bindings file is this text: the verb
/// table rendered, plus the shapes it names. The single source is
/// `VERBS`; the committed `assets/scripts/cam.d.ts` is checked
/// against this by a law test, so bindings and runtime cannot drift.
#[must_use]
pub fn bindings() -> String {
    let mut s = String::from(DTS_TYPES);
    s.push_str("interface CamApi {\n");
    for v in VERBS {
        let note = if v.arity < 0 { "variadic".to_owned() } else { format!("arity {}", v.arity) };
        s.push_str(&format!("  {};  // {note}\n", v.ts));
    }
    s.push_str("}\n\ndeclare const cam: CamApi;\n");
    s
}

/// The shared preamble of every generated header: the shapes the
/// verbs return, specced from this repo's own views (the setup sheet,
/// the tool list, the post gate).
const DTS_TYPES: &str = r#"// CamCraft job-script API - the `cam` global of a batch run.
//
// GENERATED SHAPE: the verb lines below are the VERBS table in
// crates/app/src/script/mod.rs, and the law test in
// crates/app/tests/scripting.rs fails if the two drift. The types
// they name are this repo's own views: the operation table, the tool
// list, the post gate. Provenance: clean-room, specced from ROADMAP
// M5.3 F ("Typescript bindings for batch ops").
//
// There is no `fs`, no `fetch`, no `console` in a job - every side
// effect is a cam verb, so every world write rides the command
// history and nothing happens the batch report cannot show.

type OpStatus = "ok" | "stale" | "never";

interface OperationInfo {
  name: string;
  kind: string;
  /** null for the implicit first setup (the document's own law). */
  setup: string | null;
  status: OpStatus;
  /** Generation warnings carried by the cached path, if any. */
  warnings?: string[];
}

interface DocumentInfo {
  path: string;
  operations: OperationInfo[];
}

interface VerifyReport {
  /** The post's whole verdict: every op in scope fresh, the gate passed,
   *  and no op in scope holding the post with its own warnings. */
  ok: boolean;
  operations: OperationInfo[];
  /** The program-level gate; skipped (with reason) while anything is stale. */
  gate: { ok: boolean; problems: string[]; skipped?: string };
  /** Ops in scope whose own cached warnings refuse the post (#722). */
  held_by: string[];
}

interface PostOptions {
  /** The .nc (or .cnc) file to write. Required. */
  to: string;
  /** A built-in post name - "Fanuc", "Haas", "Siemens 840D",
   *  "Heidenhain", "Mazak", "Tormach PathPilot" - or the path of a .cps file.
   *  Omitted means the document's own post. */
  cps?: string;
  /** Also return the program text in the result. */
  program?: boolean;
}

interface PostResult {
  path: string;
  ops: number;
  lines: number;
  program?: string;
  /** Setups on different machines or posts: every file written, one per mount. */
  files?: string[];
  mounts?: PostedMountInfo[];
}

interface PostedMountInfo {
  path: string;
  setups: string[];
  machine: string;
  post: string;
  ops: number;
  lines: number;
  program?: string;
}

interface ToolRow {
  tool: string;
  ops: string[];
  cut_mm: number;
  rapid_mm: number;
  minutes: number;
  changes: number;
}

interface JobReport {
  document: string | null;
  posted: string | null;
  operations: OperationInfo[];
  tools: ToolRow[];
  minutes: number;
  cut_mm: number;
  rapid_mm: number;
}

"#;

/// f64 -> a JSON number, NaN-safe: non-finite answers are `null`,
/// never the invalid JSON a raw NaN would be (js.rs's `lit` law).
pub(crate) fn fnum(v: f64) -> Value {
    if v.is_finite() {
        json!(v)
    } else {
        Value::Null
    }
}

/// The document's operation inventory, in program order, for the two
/// views that read the same list (the `operations()` verb and the
/// per-op `verify()` roll-up).
pub(crate) fn inventory(env: &Env) -> Vec<(String, flecs_ecs::prelude::Entity, crate::operation::OpStatus)> {
    crate::setup::operation_order(&env.world)
        .into_iter()
        .map(|(name, e)| {
            let status = crate::operation::status_of(&env.world, &e);
            (name, e, status)
        })
        .collect()
}

/// The embedded post-processor source a `cps: "<name>"` option picks:
/// the registry posting::script_source reads, mirrored by name.
pub(crate) fn builtin_cps(name: &str) -> Option<&'static str> {
    crate::posting::BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, text)| *text)
}

/// Keys an options object must not carry, named by the verb that
/// refuses (a silent ignore is the wrong answer to a typo).
pub(crate) fn only_keys(obj: &Value, allowed: &[&str]) -> Result<BTreeMap<String, Value>, String> {
    let mut out = BTreeMap::new();
    let Value::Object(map) = obj else {
        return Err("an options object was expected".into());
    };
    for (k, v) in map {
        if !allowed.contains(&k.as_str()) {
            return Err(format!("unknown option \"{k}\" - this verb knows: {}", allowed.join(", ")));
        }
        out.insert(k.clone(), v.clone());
    }
    Ok(out)
}

/// The string of a positional argument, or a refusal naming what it
/// wanted (the dispatcher adds the verb - no double prefix here).
pub(crate) fn arg_str(args: &[Value], i: usize, what: &str) -> Result<String, String> {
    match args.get(i) {
        None | Some(Value::Null) => Err(format!("{what} is required")),
        Some(Value::String(s)) => Ok(s.clone()),
        Some(other) => Err(format!("{what} must be a string, got {other}")),
    }
}

/// Optional positional argument: absent/null reads as None.
pub(crate) fn arg_opt(args: &[Value], i: usize, what: &str) -> Result<Option<String>, String> {
    match args.get(i) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Err(format!("{what} must be a string, got {other}")),
    }
}
