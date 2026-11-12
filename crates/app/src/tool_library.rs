//! The native tool library file (#300): the document's tools written as
//! a JSON array of `{name, definition}` - the exact tool shape the MCP
//! server's `get_job` / `add_tool` speak and a `.camproj` stores - inside
//! a small `camcraft-tools/1` envelope, and the strict reader that turns
//! such a file back into named tools for [`crate::commands::ImportTools`].
//!
//! Provenance: our own format, built from our own project DTOs
//! (`project::tool_to_json` / `tool_from_json`); no vendor library format
//! (Fusion `.json`, `.hsmlib`) was read or consulted - those stay out of
//! scope by the owner's clean-room decision on #300.
//!
//! #889: a definition may also carry the tool's body (`body`, the very
//! `ToolBodyDto` a `.camproj` tool stores - a drawn holder profile, a
//! placed mesh or a lathe holder). The key is additive: a file without
//! it reads as before, and a build that predates it skips it (the
//! tool record alone). A body is judged here exactly as the project
//! loader and `SetToolBody` judge one - re-derived from its outline or
//! facets and fitted to its tool - so a body the holder laws would
//! misread refuses the whole file by name.

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use crate::model;
use crate::project::{self, ToolBodyDto};
use crate::schema::ToolBody;
use crate::tool_body;
use crate::tools::Tool;

/// The envelope's `format` tag.
pub const FORMAT: &str = "camcraft-tools";
/// The envelope's `version`; a reader refuses any other.
pub const VERSION: u64 = 1;
/// The extension the file dialogs offer.
pub const EXTENSION: &str = "camtools";

/// The path an export writes: the chosen one, with `.camtools` added
/// when the name has no extension. A save panel only suggests the
/// extension - macOS keeps a typed name without it for a file type it
/// does not know - and a suffix-less library is easy to lose and is
/// hidden by the import dialog's filter. A name that already carries
/// an extension (`.camtools`, or a deliberate `.json`) is kept as typed.
pub fn with_library_extension(path: &std::path::Path) -> std::path::PathBuf {
    match path.extension() {
        Some(_) => path.to_path_buf(),
        None => path.with_extension(EXTENSION),
    }
}

/// One named tool of a library file.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryTool {
    pub name: String,
    pub tool: Tool,
    /// #889: the body the file carries for this tool (already judged
    /// against it), `None` for a tool with only its holder cylinder.
    pub body: Option<ToolBody>,
}

impl LibraryTool {
    /// A library tool with no body - what every pre-#889 file holds.
    #[must_use]
    pub fn bare(name: impl Into<String>, tool: Tool) -> Self {
        Self { name: name.into(), tool, body: None }
    }
}

/// The document's tools, sorted by name, as library JSON (pretty, so a
/// person can read and hand-edit it). Lengths are millimetres, as the
/// document stores them, whatever the display units. Refuses an empty
/// document: a library with no tools is never what the user meant.
/// #889: a tool's body rides its definition as `body`.
pub fn export_tool_library(world: &World) -> Result<String, String> {
    let mut tools: Vec<Value> = Vec::new();
    for (n, e) in model::named_entities(world) {
        let Some(t) = model::tool_of(world, &e) else { continue };
        let mut def = project::tool_to_json(&t);
        if let Some(b) = tool_body::body_of(world, &e) {
            let body = serde_json::to_value(ToolBodyDto::from_body(&b))
                .map_err(|err| format!("tool '{n}': body not written: {err}"))?;
            if let Value::Object(o) = &mut def {
                o.insert("body".into(), body);
            }
        }
        tools.push(json!({ "name": n, "definition": def }));
    }
    if tools.is_empty() {
        return Err("the document has no tools to export".into());
    }
    let doc = json!({ "format": FORMAT, "version": VERSION, "units": "mm", "tools": tools });
    serde_json::to_string_pretty(&doc).map_err(|e| format!("tool library not written: {e}"))
}

/// Read a library file's text. The whole file is checked before anything
/// is returned: malformed JSON, a wrong format tag or version, a missing
/// or empty name, a name twice, a definition that does not parse, or a
/// tool `Tool::is_sane` refuses each refuse the whole file, naming the
/// tool. Accepts the envelope or the bare `[{name, definition}]` array
/// (what MCP's `get_job` lists), so either can be pasted into a file.
pub fn parse_tool_library(text: &str) -> Result<Vec<LibraryTool>, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| format!("not a tool library (bad JSON: {e})"))?;
    let rows = match &root {
        Value::Array(rows) => rows,
        Value::Object(o) => {
            match o.get("format").and_then(Value::as_str) {
                Some(FORMAT) => {}
                Some(other) => return Err(format!("not a tool library: format is '{other}', not '{FORMAT}'")),
                None => return Err(format!("not a tool library: no \"format\": \"{FORMAT}\"")),
            }
            match o.get("version").and_then(Value::as_u64) {
                Some(VERSION) => {}
                Some(v) => return Err(format!("tool library version {v} is not supported (this build reads {VERSION})")),
                None => return Err("not a tool library: no \"version\"".into()),
            }
            if let Some(u) = o.get("units").and_then(Value::as_str)
                && u != "mm"
            {
                return Err(format!("tool library units '{u}' are not supported (lengths are mm)"));
            }
            o.get("tools")
                .and_then(Value::as_array)
                .ok_or("not a tool library: no \"tools\" array")?
        }
        _ => return Err("not a tool library: expected an object or an array".into()),
    };
    let mut out: Vec<LibraryTool> = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let n = i.saturating_add(1);
        let name = row
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("tool #{n} has no name"))?;
        if out.iter().any(|t| t.name == name) {
            return Err(format!("tool '{name}' appears twice in the library"));
        }
        let def = row.get("definition").ok_or_else(|| format!("tool '{name}' has no definition"))?;
        let tool = project::tool_from_json(def).map_err(|e| format!("tool '{name}': {e}"))?;
        if let Some(why) = tool.torch_refusal() {
            return Err(format!("tool '{name}' is refused: {why}"));
        }
        if !tool.is_sane() {
            return Err(format!(
                "tool '{name}' is refused: its sizes must be finite and positive, corner radius within the diameter"
            ));
        }
        let body = read_body(def, &tool, name)?;
        out.push(LibraryTool { name: name.to_owned(), tool, body });
    }
    if out.is_empty() {
        return Err("the tool library holds no tools".into());
    }
    Ok(out)
}

/// #889: a definition's `body`, judged as the project loader judges it
/// (`project::validate`): re-derived by this build's envelope law and
/// fitted to its tool, or the file refuses naming the tool.
fn read_body(def: &Value, tool: &Tool, name: &str) -> Result<Option<ToolBody>, String> {
    let Some(raw) = def.get("body").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let dto: ToolBodyDto =
        serde_json::from_value(raw.clone()).map_err(|e| format!("tool '{name}': body is malformed: {e}"))?;
    let body = dto.to_body(name).map_err(|e| format!("{e} (body refused)"))?;
    tool_body::fits(tool, &body).map_err(|e| format!("tool '{name}': body refused: {e}"))?;
    Ok(Some(body))
}

/// How many of a library's tools carry a body (#889) - what the import
/// page and the MCP reply name, so the holders choice is informed.
#[must_use]
pub fn bodies_in(library: &[LibraryTool]) -> usize {
    library.iter().filter(|t| t.body.is_some()).count()
}
