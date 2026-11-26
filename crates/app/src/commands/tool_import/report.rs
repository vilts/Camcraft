//! Import mode and summary report types for [`super::ImportTools`]
//! (#300, #889, #899); extracted from `tool_import` to keep each source
//! module focused and under 400 lines.

/// What an import does with a library tool whose name the document has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImportMode {
    /// Skip an identical tool, rename a different one (the original law).
    #[default]
    Add,
    /// Take the imported definition in place; add the unmatched.
    Update,
    /// Update, then remove document tools the library lacks (refused if used).
    Replace,
}

impl ImportMode {
    /// `add` / `update` / `replace`, case-blind, for scripts and MCP.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "add" => Ok(Self::Add),
            "update" => Ok(Self::Update),
            "replace" => Ok(Self::Replace),
            other => Err(format!("unknown import mode '{other}' (use add, update or replace)")),
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Update => "update",
            Self::Replace => "replace",
        }
    }
}

/// What an import did with each library tool, for the status line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportReport {
    /// Tools added under their own name.
    pub added: Vec<String>,
    /// `(library name, name it was added as)` for a name already taken.
    pub renamed: Vec<(String, String)>,
    /// Library names skipped: the document holds that exact tool already.
    pub skipped: Vec<String>,
    /// Update mode: tools that took the imported definition in place.
    pub updated: Vec<String>,
    /// Replace mode: tools that took the imported definition in place.
    pub replaced: Vec<String>,
    /// Replace mode: document tools the library lacked, removed.
    pub removed: Vec<String>,
    /// #889: tools that landed (added or changed) with their file's holder body.
    pub holders: usize,
    /// #889: bodies the file carried that the import left out (holders off).
    pub holders_left_out: usize,
}

fn list(names: &[String]) -> String {
    names.join(", ")
}

impl ImportReport {
    /// One line: "imported 3 tools; 1 renamed (A -> A (2)); 2 already in the document (B, C)".
    #[must_use]
    pub fn summary(&self) -> String {
        let n = self.added.len().saturating_add(self.renamed.len());
        let mut parts: Vec<String> = Vec::new();
        let in_place = self
            .updated
            .len()
            .saturating_add(self.replaced.len())
            .saturating_add(self.removed.len());
        if n > 0 || in_place == 0 {
            parts.push(format!("imported {n} tool{}", if n == 1 { "" } else { "s" }));
        }
        if !self.renamed.is_empty() {
            let l: Vec<String> = self.renamed.iter().map(|(a, b)| format!("{a} -> {b}")).collect();
            parts.push(format!("{} renamed ({})", self.renamed.len(), l.join(", ")));
        }
        for (what, names) in [
            ("updated", &self.updated),
            ("replaced", &self.replaced),
            ("removed", &self.removed),
        ] {
            if !names.is_empty() {
                parts.push(format!("{} {what} ({})", names.len(), list(names)));
            }
        }
        if !self.skipped.is_empty() {
            parts.push(format!("{} already in the document ({})", self.skipped.len(), list(&self.skipped)));
        }
        if self.holders > 0 {
            parts.push(format!(
                "{} with {} holder",
                self.holders,
                if self.holders == 1 { "its" } else { "their" }
            ));
        }
        if self.holders_left_out > 0 {
            let s = if self.holders_left_out == 1 { "" } else { "s" };
            parts.push(format!("{} holder{s} in the file not imported", self.holders_left_out));
        }
        parts.join("; ")
    }
}
