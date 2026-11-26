//! The one source of the agent docs (#1947): the MCP tool reference and
//! the compact local-model prompt, both generated from the live tool table.
//!
//! Provenance: our own text over this repo's MCP tool table (`tools.rs`).

use super::tools;
use serde_json::Value;

/// Where the generated tool reference is committed, from the repo root.
/// One neutral path that every agent wrapper links to.
pub const REFERENCE_PATH: &str = "docs/agents/mcp-tools.md";

/// The command that writes the reference again after a tool changes.
pub const REFERENCE_COMMAND: &str =
    "cargo run -p camcraft-app --bin camcraft-cli -- mcp reference > docs/agents/mcp-tools.md";

/// The working rules every agent gets, cloud or local.
pub const RULES: &str = "\
You drive CamCraft, a CAM program, through its MCP tools.
- The usual flow is import_model, set_stock, add_tool, add_operation, generate, verify, post.
- get_job returns the whole document. list_operation_types gives each kind's parameters and defaults.
- Each change is one undo step. undo takes back the last change.
- A refusal names the tool and the reason, and changes nothing. Read it and fix the arguments. Do not repeat the same call.
- An unknown argument is refused. Use only the arguments that a tool lists.
- Post only when verify answers \"ok\": true. Never say that a refused post was written.";

/// Every live tool name, in table order.
#[must_use]
pub fn tool_names() -> Vec<&'static str> {
    tools::all().map(|t| t.name).collect()
}

/// The argument list of a tool's schema: `name` when required, `name?`
/// when optional.
fn arguments(schema: &Value) -> Vec<String> {
    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    schema["properties"]
        .as_object()
        .map(|props| {
            props
                .keys()
                .map(|k| if required.contains(&k.as_str()) { k.clone() } else { format!("{k}?") })
                .collect()
        })
        .unwrap_or_default()
}

/// One line of table text: no line breaks, no bare column bars.
fn cell(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").replace('|', "\\|")
}

/// The first sentence of a description, for the compact prompt.
fn first_sentence(text: &str) -> String {
    let flat = cell(text);
    match flat.find(". ") {
        Some(end) => flat[..=end].to_owned(),
        None => flat,
    }
}

/// The tool reference as Markdown: one row per live tool with its
/// arguments and its full description. Committed at [`REFERENCE_PATH`].
#[must_use]
pub fn reference_markdown() -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "<!-- Generated from crates/app/src/mcp/tools.rs. Do not edit by hand. Write it again with: {REFERENCE_COMMAND} -->\n"
    ));
    out.push_str("# CamCraft MCP tool reference\n\n");
    let all: Vec<_> = tools::all().collect();
    out.push_str(&format!(
        "The server lists {} tools. The live `tools/list` reply is the authority; this file is generated from the same table. An argument with `?` is optional.\n\n",
        all.len()
    ));
    out.push_str("| Tool | Arguments | What it does |\n|---|---|---|\n");
    for t in all {
        let args = arguments(&(t.schema)());
        let args = if args.is_empty() { "-".to_owned() } else { format!("`{}`", args.join("`, `")) };
        out.push_str(&format!("| `{}` | {} | {} |\n", t.name, args, cell(t.description)));
    }
    out
}

/// The compact prompt for a local model (Ollama, LM Studio,
/// `llama-server` and the like): the rules, then one line per tool.
#[must_use]
pub fn local_prompt() -> String {
    let mut out = String::from(RULES);
    out.push_str("\n\nTools, as name(arguments): what it does. An argument with ? is optional.\n");
    for t in tools::all() {
        let args = arguments(&(t.schema)()).join(", ");
        out.push_str(&format!("- {}({}): {}\n", t.name, args, first_sentence(t.description)));
    }
    out
}
