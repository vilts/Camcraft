//! The overwrite guard of `cam.save` and the MCP tool `save_project`
//! (#3608): a save to the file that the document was loaded from or last
//! saved to is refused, by name, when that file on disk changed after this
//! session opened or saved it. A script or an agent has nobody to ask, so
//! the caller gives `overwrite: true` to replace the file. The window asks
//! instead (`ui::save_guard`).
//!
//! Provenance: our own refusal over the project file's disk stamp
//! (`project::DiskStamp`).

use super::only_keys;
use crate::project::DiskStamp;

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// Refuse a save to `path` when `known` is the stamp of that file and the
/// file changed since. `options` is the verb's options object
/// (`{ overwrite?: boolean }`); a bad option is refused even when there is
/// nothing to guard.
pub(super) fn check(
    known: Option<&DiskStamp>,
    path: &Path,
    options: Option<&Value>,
) -> Result<(), String> {
    let given: BTreeMap<String, Value> = match options {
        None | Some(Value::Null) => BTreeMap::new(),
        Some(v) => only_keys(v, &["overwrite"])?,
    };
    let overwrite = match given.get("overwrite") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(other) => return Err(format!("\"overwrite\" must be true or false, got {other}")),
    };
    if !overwrite && known.is_some_and(|stamp| stamp.changed(path)) {
        return Err(format!(
            "{}: the file on disk changed after this session opened or saved it; nothing was written. \
             Save to a different path, or give overwrite: true to replace the file",
            path.display()
        ));
    }
    Ok(())
}
