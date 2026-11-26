//! Provenance: issue #2407's generic ATC macro contract; controller words remain post-authored.
//!
//! A post can declare `toolChangeMacros = { before: "M100\nM101", after: "M102" }`.
//! Fields default to empty. On the first tool and each tool-number change,
//! macros wrap the complete hook, including retracts and spindle commands.
//! Repeated tools still reach the hook; the engine never interprets macro NC.

use super::{js::Script, PostError};
use serde::Deserialize;
use serde_json::json;

#[derive(Default, Deserialize)]
pub(super) struct Macros {
    #[serde(default)]
    before: String,
    #[serde(default)]
    after: String,
    #[serde(skip)]
    last_tool: Option<u32>,
}

impl Macros {
    /// Validate before any lifecycle hook can emit a partial program.
    pub(super) fn read(script: &mut Script) -> Result<Self, PostError> {
        let value = script.eval_src(
            r#"(function () {
                if (typeof toolChangeMacros === "undefined") return "{}";
                var m = toolChangeMacros;
                if (m === null || typeof m !== "object" || Array.isArray(m))
                    throw new Error("toolChangeMacros must be an object with before and after strings");
                Object.keys(m).forEach(function (key) {
                    if (key !== "before" && key !== "after")
                        throw new Error("toolChangeMacros." + key + " is unknown; use before or after");
                    if (typeof m[key] !== "string")
                        throw new Error("toolChangeMacros." + key + " must be a string");
                });
                return JSON.stringify(m);
            })()"#,
        )?;
        let text = value
            .as_string()
            .ok_or_else(|| PostError::Refused("toolChangeMacros must contain before and after strings".into()))?
            .to_std_string_escaped();
        let mut macros: Self = serde_json::from_str(&text)
            .map_err(|e| PostError::Refused(format!("toolChangeMacros: {e}")))?;
        for (field, text) in [("before", &mut macros.before), ("after", &mut macros.after)] {
            *text = text.replace("\r\n", "\n");
            if text.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
                return Err(PostError::Refused(format!(
                    "toolChangeMacros.{field} contains a control character; use NC text with newlines, tabs or CRLF"
                )));
            }
        }
        Ok(macros)
    }

    pub(super) fn call(&mut self, script: &mut Script, number: u32, name: &str) -> Result<(), PostError> {
        let changes = self.last_tool != Some(number);
        if changes {
            Self::emit(script, &self.before)?;
        }
        script.call("onToolChange", &format!("{}, {}", json!(number), json!(name)))?;
        if changes {
            Self::emit(script, &self.after)?;
        }
        self.last_tool = Some(number);
        Ok(())
    }

    fn emit(script: &mut Script, text: &str) -> Result<(), PostError> {
        for line in text.lines() {
            // Encode as a JS string literal: quotes and backslashes are NC data.
            script.eval_src(&format!("writeBlock({});", json!(line)))?;
        }
        Ok(())
    }
}
