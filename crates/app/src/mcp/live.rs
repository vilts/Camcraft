//! The live seam (#183): one MCP wire line answered against a document
//! the caller already owns - the GUI's world and its command History -
//! instead of the headless session's own. Same tool table, same
//! dispatch, same commands: an agent's step lands on the app's own
//! History, so the app's Ctrl+Z undoes it like any user step.
//!
//! Provenance: our own seam over this module's JSON-RPC handler
//! (JSON-RPC 2.0 framed as the public MCP specification, 2025-06-18).
//! Clean-room, public sources only.

use super::selection::SelectionContext;
use super::{tool_err, Server};
use crate::commands::History;
use crate::project::DiskStamp;
use crate::script::Env;

use flecs_ecs::prelude::World;
use serde_json::{json, Value};
use std::path::Path;

/// Tools the live session refuses, with the reason an agent reads. The
/// app owns these: opening a file over the user's document skips the
/// app's unsaved-changes prompt and its per-document reset.
const LIVE_REFUSED: &[(&str, &str)] = &[(
    "open_project",
    "open_project is not available in a live app session - it would replace the user's document \
without the app's unsaved-changes prompt; ask the user to open the file (File > Open)",
)];

/// Why a live MCP tool call requires explicit user approval before dispatch (#1942).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateKind {
    /// G-code posting (`post`). Never auto-approved.
    Posting,
    /// Machine mount or machine definition edits (`load_machine`, `edit_machine`). Never auto-approved.
    MachineEdit,
    /// Setup post or workholding limit edits (`set_setup_post`, `set_workholding_max_rpm`, `set_setup_nose_to_axis`,
    /// `set_sheet_job`: its allowance is a floor the post holds the program to).
    LimitEdit,
    /// File writes (`save_project`, `export_tool_library`, `export_fixture_dxf` with `path`).
    FileWrite,
    /// File-path reads or arguments (`import_model`, `import_tool_library`, `set_tool_body`,
    /// `set_lathe_tool_body`, `attach_tool_body_piece`, `import_dxf_fixture` with `path`,
    /// or any tool call carrying a non-null `path` or `output` argument).
    FilePathAccess,
}

impl GateKind {
    /// G-code posting and machine edits are NEVER auto-approvable.
    #[must_use]
    pub const fn allows_auto_approve(self) -> bool {
        !matches!(self, Self::Posting | Self::MachineEdit)
    }
}

/// Classify whether a tool call requires explicit user approval at live dispatch.
#[must_use]
pub fn requires_approval(tool: &str, args: &Value) -> Option<GateKind> {
    match tool {
        "post" | "export_agie_inspection" => Some(GateKind::Posting),
        "load_machine" | "edit_machine" => Some(GateKind::MachineEdit),
        "set_setup_post" | "set_workholding_max_rpm" | "set_setup_nose_to_axis" | "set_sheet_job" => {
            Some(GateKind::LimitEdit)
        }
        "save_project" | "export_tool_library" => Some(GateKind::FileWrite),
        "export_fixture_dxf" if has_non_null_arg(args, "path") => Some(GateKind::FileWrite),
        "import_model"
        | "import_tool_library"
        | "set_tool_body"
        | "set_lathe_tool_body"
        | "attach_tool_body_piece" => Some(GateKind::FilePathAccess),
        "import_dxf_fixture" if has_non_null_arg(args, "path") => Some(GateKind::FilePathAccess),
        _ if has_non_null_arg(args, "path") || has_non_null_arg(args, "output") => {
            Some(GateKind::FilePathAccess)
        }
        _ => None,
    }
}

fn has_non_null_arg(args: &Value, key: &str) -> bool {
    args.get(key).is_some_and(|v| !v.is_null())
}

fn normalize_args(args: Value) -> Value {
    if args.is_null() { json!({}) } else { args }
}

/// Explicit single-call user approval binding exact tool name and arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovedCall {
    tool: String,
    arguments: Value,
}

impl ApprovedCall {
    /// Bind user approval to one exact tool call and argument object.
    #[must_use]
    pub fn new(tool: impl Into<String>, arguments: Value) -> Self {
        Self {
            tool: tool.into(),
            arguments: normalize_args(arguments),
        }
    }

    fn matches(&self, tool: &str, args: &Value) -> bool {
        self.tool == tool && self.arguments == normalize_args(args.clone())
    }
}

/// Permission state evaluated before `answer_live` dispatches a gated tool.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveGate {
    /// Single-call explicit user approval from the UI Allow/Deny confirmation.
    pub approved: Option<ApprovedCall>,
    /// Optional caller opt-in for non-posting, non-machine file/limit tools.
    /// G-code posting (`post`) and machine edits (`load_machine`, `edit_machine`)
    /// ignore this flag and ALWAYS require explicit [`Self::approved`].
    pub auto_approve_non_critical: bool,
}

impl LiveGate {
    /// Construct a gate permitting one exact tool call approved by the user.
    #[must_use]
    pub fn allow_once(tool: impl Into<String>, arguments: Value) -> Self {
        Self {
            approved: Some(ApprovedCall::new(tool, arguments)),
            auto_approve_non_critical: false,
        }
    }

    /// Check whether `tool` with `args` is authorized under this gate.
    pub fn check(&self, tool: &str, args: &Value) -> Result<(), String> {
        let Some(kind) = requires_approval(tool, args) else {
            return Ok(());
        };
        if self.approved.as_ref().is_some_and(|a| a.matches(tool, args)) {
            return Ok(());
        }
        if self.auto_approve_non_critical && kind.allows_auto_approve() {
            return Ok(());
        }
        Err(refusal_message(tool, kind))
    }
}

fn refusal_message(tool: &str, kind: GateKind) -> String {
    match kind {
        GateKind::Posting => format!(
            "{tool}: explicit user approval is required before posting G-code (auto-approval is forbidden)"
        ),
        GateKind::MachineEdit => format!(
            "{tool}: explicit user approval is required before changing machine configuration (auto-approval is forbidden)"
        ),
        GateKind::LimitEdit => format!(
            "{tool}: explicit user approval is required before changing post or workholding limits"
        ),
        GateKind::FileWrite => format!(
            "{tool}: explicit user approval is required before writing files"
        ),
        GateKind::FilePathAccess => format!(
            "{tool}: explicit user approval is required before accessing file-path arguments"
        ),
    }
}

/// Caller-supplied live gate, UI selection context and read-only state for
/// [`answer_live_with`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LiveOptions {
    pub gate: LiveGate,
    pub selection: Option<SelectionContext>,
    /// The window's project is open read-only (`camcraft-ui --read-only`,
    /// #3609): `save_project` refuses to save over `doc_path`.
    pub read_only: bool,
    /// #3608: the window's disk stamp of `doc_path`, so that an agent's save
    /// over a file whose copy on disk changed is refused as in a headless
    /// session.
    pub doc_stamp: Option<DiskStamp>,
}

/// What one live line did.
#[derive(Debug, Default)]
pub struct LiveAnswer {
    /// The wire answer, `None` for a notification.
    pub reply: Option<String>,
    /// The tool a `tools/call` named, when the line was one: the document
    /// may have changed (a read-only tool too - the caller refreshes its
    /// views, which is cheap and never wrong).
    pub tool: Option<String>,
    /// #3608: the session's disk stamp after the call - new after a save -
    /// for the window to keep when it is the stamp of the window's file.
    /// `None` when the call did not run.
    pub doc_stamp: Option<DiskStamp>,
}

/// Answer one wire line against `world` and `history` using the default
/// unapproved [`LiveGate`] (blocking unconfirmed posting, file writes,
/// file-path arguments, and machine/post/workholding limit edits).
pub fn answer_live(world: &World, history: &mut History, doc_path: Option<&Path>, line: &str) -> LiveAnswer {
    answer_live_with(world, history, doc_path, line, &LiveOptions::default())
}

/// Answer one wire line against `world` and `history` with explicit live
/// gate approvals and optional UI selection snapshot (#1942).
pub fn answer_live_with(
    world: &World,
    history: &mut History,
    doc_path: Option<&Path>,
    line: &str,
    options: &LiveOptions,
) -> LiveAnswer {
    let parsed = serde_json::from_str::<Value>(line.trim()).ok();
    let call_params = parsed
        .as_ref()
        .filter(|m| m.get("method").and_then(Value::as_str) == Some("tools/call"))
        .and_then(|m| m.get("params"));
    let tool = call_params
        .and_then(|p| p.get("name")?.as_str().map(str::to_owned));
    let args = call_params
        .and_then(|p| p.get("arguments").cloned())
        .map_or_else(|| json!({}), normalize_args);
    if let (Some(msg), Some(name)) = (&parsed, &tool) {
        if let Some((_, why)) = LIVE_REFUSED.iter().find(|(n, _)| n == name) {
            let reply = msg
                .get("id")
                .map(|id| json!({ "jsonrpc": "2.0", "id": id, "result": tool_err(why) }).to_string());
            return LiveAnswer { reply, tool: None, doc_stamp: None };
        }
        if let Err(why) = options.gate.check(name, &args) {
            let reply = msg
                .get("id")
                .map(|id| json!({ "jsonrpc": "2.0", "id": id, "result": tool_err(&why) }).to_string());
            return LiveAnswer { reply, tool: None, doc_stamp: None };
        }
    }
    let mut server = Server {
        env: Env {
            world: world.clone(),
            history: std::mem::take(history),
            logs: Vec::new(),
            errors: Vec::new(),
            doc_path: doc_path.map(Path::to_path_buf),
            doc_read_only: options.read_only,
            doc_stamp: options.doc_stamp.clone(),
            posted: None,
            last_value: Value::Null,
            selection: options.selection.clone(),
        },
        // The live bridge answers one line at a time: no background lane.
        bg: None,
        gate: None,
    };
    let reply = server.handle_line(line);
    *history = std::mem::take(&mut server.env.history);
    LiveAnswer { reply, tool, doc_stamp: server.env.doc_stamp.take() }
}
