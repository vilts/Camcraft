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
//!
//! #2016: a library holder rides its definition as `holder` (the
//! reference a `.camproj` tool stores: entry name, gauge length,
//! projection) in place of `body`, and the file then says version 4
//! (`HOLDER_REF_VERSION`): a reader that predates the key would drop it
//! and judge the tool's smaller holder cylinder, so it must refuse the
//! file. An entry this build's library does not hold refuses the whole
//! file by name.
//!
//! #3904: a tool's drawn cutter (`ToolCutter`, #93) rides its definition
//! as `cutter` - the `[z, r, arc]` points a `.camproj` tool stores - and
//! the file then says version 5 (`CUTTER_VERSION`): the stack laws read
//! its neck (#852), so a reader that predates the key would drop it and
//! judge a different tool; it must refuse the file. A cutter is judged as
//! the project loader judges one (`cutter_profile::check`); one that
//! disagrees with its tool's numbers refuses the whole file by name.

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use crate::model;
use crate::project::{self, HolderRefDto, ToolBodyDto};
use crate::schema::ToolBody;
use crate::spun::{SpunPoint, SpunProfile};
use crate::tool_body;
use crate::tool_cutter;
use crate::tools::Tool;

/// The envelope's `format` tag.
pub const FORMAT: &str = "camcraft-tools";
/// The original envelope version, retained for libraries without left-hand taps.
pub const VERSION: u64 = 1;
/// Older readers must refuse libraries containing left-hand taps.
pub const LEFT_HAND_VERSION: u64 = 2;
/// #2015: older readers must refuse a library whose cutting data holds
/// an `unknown` row or a stepdown / stepover (they would drop or refuse
/// it); a library with neither keeps the older number.
pub const CUTTING_STEPS_VERSION: u64 = 3;
/// #2016: older readers (including v0.22.0 at version 2 and version-3
/// builds) must refuse libraries whose tools name a library holder (`holder`).
pub const HOLDER_REF_VERSION: u64 = 4;
/// #3904: older readers must refuse libraries whose tools carry a drawn
/// cutter (`cutter`); a library without one keeps the older number.
pub const CUTTER_VERSION: u64 = 5;
/// #3368: older readers (version 5 builds included) must refuse a library
/// with a flat or ball taper tip: they would read the tip as sharp, a
/// smaller tool than the real one. A library without one keeps its older
/// number.
pub const TIP_FORM_VERSION: u64 = 6;
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
    /// #2566: explicit controller tool number (`T..` / `H..`) carried by
    /// a machine tool-table file (PathPilot `.csv` or Hurco `.TCM`),
    /// `None` for `.camtools` JSON which leaves numbers to the document.
    pub number: Option<u32>,
    /// #3904: the drawn cutter the file carries for this tool (already
    /// judged against it), `None` for the outline its numbers draw.
    pub cutter: Option<SpunProfile>,
}

impl LibraryTool {
    /// A library tool with no body and no explicit controller number.
    #[must_use]
    pub fn bare(name: impl Into<String>, tool: Tool) -> Self {
        Self { name: name.into(), tool, body: None, number: None, cutter: None }
    }
}

/// The document's tools, sorted by name, as library JSON (pretty, so a
/// person can read and hand-edit it). Lengths are millimetres, as the
/// document stores them, whatever the display units. Refuses an empty
/// document: a library with no tools is never what the user meant.
/// #889: a tool's body rides its definition as `body`; #3904: its drawn
/// cutter as `cutter`.
pub fn export_tool_library(world: &World) -> Result<String, String> {
    let mut tools: Vec<Value> = Vec::new();
    let mut version = VERSION;
    for (n, e) in model::named_entities(world) {
        let Some(t) = model::tool_of(world, &e) else { continue };
        if t.kind == crate::tools::ToolKind::Tap
            && t.thread_hand == camcraft_tools::thread_hand::ThreadHand::Left
        {
            version = version.max(LEFT_HAND_VERSION);
        }
        if needs_steps_version(&t) {
            version = version.max(CUTTING_STEPS_VERSION);
        }
        if !t.tip_form.is_sharp() {
            version = version.max(TIP_FORM_VERSION);
        }
        let mut def = project::tool_to_json(&t);
        if let Some(b) = tool_body::body_of(world, &e) {
            // #2016: a library holder is written as its reference.
            let (key, value) = match project::body_to_dto(&b) {
                (Some(h), _) => {
                    version = version.max(HOLDER_REF_VERSION);
                    ("holder", serde_json::to_value(h))
                }
                (None, body) => ("body", serde_json::to_value(body)),
            };
            let value = value.map_err(|err| format!("tool '{n}': {key} not written: {err}"))?;
            if let Value::Object(o) = &mut def {
                o.insert(key.into(), value);
            }
        }
        if let Some(c) = tool_cutter::cutter_of(world, &e) {
            version = version.max(CUTTER_VERSION);
            if let Value::Object(o) = &mut def {
                let pts: Vec<[f64; 3]> = c.points().iter().map(|p| [p.z, p.r, p.arc]).collect();
                o.insert("cutter".into(), json!(pts));
            }
        }
        tools.push(json!({ "name": n, "definition": def }));
    }
    if tools.is_empty() {
        return Err("the document has no tools to export".into());
    }
    let doc = json!({ "format": FORMAT, "version": version, "units": "mm", "tools": tools });
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
                Some(
                    VERSION | LEFT_HAND_VERSION | CUTTING_STEPS_VERSION | HOLDER_REF_VERSION | CUTTER_VERSION
                    | TIP_FORM_VERSION,
                ) => {}
                Some(v) => {
                    return Err(format!(
                        "tool library version {v} is not supported (this build reads {VERSION} to {TIP_FORM_VERSION})"
                    ));
                }
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
        // #1186: the rule the tool breaks, by name.
        if let Some(why) = tool.why_not_sane() {
            return Err(format!("tool '{name}' is refused: {why}"));
        }
        let body = read_body(def, &tool, name)?;
        let cutter = read_cutter(def, &tool, name)?;
        out.push(LibraryTool { name: name.to_owned(), tool, body, number: None, cutter });
    }
    if out.is_empty() {
        return Err("the tool library holds no tools".into());
    }
    Ok(out)
}

/// #2015: does `t` write cutting data a version-1/2 reader cannot keep:
/// an `Unknown` row that is not just its own rpm / feed, or a row's
/// stepdown / stepover?
fn needs_steps_version(t: &Tool) -> bool {
    t.cutting.iter().any(|r| {
        (r.is_unknown() && !camcraft_tools::cutting_rows::is_derived_unknown(r, t.rpm, t.feed))
            || r.stepdown.is_some()
            || r.stepover.is_some()
    })
}

/// #889: a definition's `body`, judged as the project loader judges it
/// (`project::validate`): re-derived by this build's envelope law and
/// fitted to its tool, or the file refuses naming the tool.
fn read_body(def: &Value, tool: &Tool, name: &str) -> Result<Option<ToolBody>, String> {
    let key = |k: &str| def.get(k).filter(|v| !v.is_null());
    let dto: Option<ToolBodyDto> = key("body")
        .map(|raw| serde_json::from_value(raw.clone()).map_err(|e| format!("tool '{name}': body is malformed: {e}")))
        .transpose()?;
    // #2016: a library holder by reference, drawn from this build's
    // library; an unknown entry refuses the file by name.
    let holder: Option<HolderRefDto> = key("holder")
        .map(|raw| serde_json::from_value(raw.clone()).map_err(|e| format!("tool '{name}': holder is malformed: {e}")))
        .transpose()?;
    let what = if holder.is_some() { "holder" } else { "body" };
    let Some(body) = project::stated_body(holder.as_ref(), dto.as_ref(), name).map_err(|e| format!("{e} ({what} refused)"))? else {
        return Ok(None);
    };
    tool_body::fits(tool, &body).map_err(|e| format!("tool '{name}': {what} refused: {e}"))?;
    Ok(Some(body))
}

/// #3904: a definition's `cutter`, judged as the project loader judges
/// it (`cutter_profile::check`, the judge of `cutter_from_dto`), or the
/// file refuses naming the tool.
fn read_cutter(def: &Value, tool: &Tool, name: &str) -> Result<Option<SpunProfile>, String> {
    let Some(raw) = def.get("cutter").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let pts: Vec<[f64; 3]> =
        serde_json::from_value(raw.clone()).map_err(|e| format!("tool '{name}': cutter is malformed: {e}"))?;
    let points = pts.iter().map(|p| SpunPoint { z: p[0], r: p[1], arc: p[2] }).collect();
    camcraft_tools::cutter_profile::check(tool, points)
        .map(Some)
        .map_err(|e| format!("tool '{name}': cutter profile: {e}"))
}

/// How many of a library's tools carry a body (#889) - what the import
/// page and the MCP reply name, so the holders choice is informed.
#[must_use]
pub fn bodies_in(library: &[LibraryTool]) -> usize {
    library.iter().filter(|t| t.body.is_some()).count()
}
