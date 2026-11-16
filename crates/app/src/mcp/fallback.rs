//! Local model fallbacks prepare requests; they never execute MCP tools.
//! Provenance: issue #1943's prompt JSON and read-only Q&A contract, over the live MCP registry.

use super::{provider_calls::ToolCall, schema_validation, tools};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackMode {
    Native,
    PromptJson,
    ReadOnly,
}

impl FallbackMode {
    #[must_use]
    pub fn for_capabilities(native_tools: bool, prompt_json: bool) -> Self {
        if native_tools {
            Self::Native
        } else if prompt_json {
            Self::PromptJson
        } else {
            Self::ReadOnly
        }
    }
}

/// A validated proposal. The caller must show it and pass the user's decision
/// to `authorize` before dispatch. Private fields prevent edits after validation.
#[derive(Debug)]
pub struct PendingCall {
    call: ToolCall,
    confirmation: bool,
}

impl PendingCall {
    #[must_use]
    pub fn requires_confirmation(&self) -> bool {
        self.confirmation
    }

    /// Display the exact tool name and arguments in the confirmation UI.
    #[must_use]
    pub fn proposal(&self) -> &ToolCall {
        &self.call
    }

    pub fn authorize(self, confirmed: bool) -> Result<ToolCall, String> {
        if self.confirmation && !confirmed {
            return Err(format!(
                "{} requires user confirmation before execution",
                self.call.name
            ));
        }
        Ok(self.call)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    tool: String,
    arguments: Value,
}

/// Accept exactly one object, optionally enclosed in one JSON code fence.
/// Reject surrounding prose or more than one proposal instead of guessing intent.
pub fn parse_prompt(text: &str) -> Result<PendingCall, String> {
    let trimmed = text.trim();
    let json = if let Some(inner) = trimmed.strip_prefix("```json\n") {
        inner
            .strip_suffix("```")
            .ok_or_else(|| "prompt JSON has an unclosed code fence".to_owned())?
            .trim()
    } else {
        trimmed
    };
    let proposal: Proposal = serde_json::from_str(json).map_err(|e| format!("prompt JSON: {e}"))?;
    schema_validation::arguments(&proposal.tool, &proposal.arguments)?;
    // An explicit read-only allowlist: a new tool is never trusted by its name prefix.
    let read_only = matches!(
        proposal.tool.as_str(),
        "get_job"
            | "list_operation_types"
            | "list_tool_catalog"
            | "list_holder_library"
            | "list_features"
            | "get_operation_geometry"
            | "get_tool_cutter_profile"
            | "suggest_cutting_data"
    );
    Ok(PendingCall {
        call: ToolCall {
            id: String::new(),
            name: proposal.tool,
            arguments: proposal.arguments,
        },
        confirmation: !read_only,
    })
}

#[must_use]
pub fn prompt_context(project: &Value) -> Value {
    json!({"instructions":"Return exactly one JSON object: {\"tool\":\"name\",\"arguments\":{}}. Use only the listed tools. Mutating edits require user confirmation before execution.",
        "project":project,"tools":tools::list()})
}

/// Context for an ordinary text answer, with no executable tools attached.
#[must_use]
pub fn qa_context(project: &Value) -> Value {
    json!({"instructions":"Answer questions from this read-only project context. Do not request or perform tool calls or edits.",
        "project":project,"tools":[]})
}
