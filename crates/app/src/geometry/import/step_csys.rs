//! Named tool coordinate systems of a vendor STEP file (#370), for the
//! tool-body commands: the nesting scan and the import error type in front
//! of `camcraft_geometry::step_csys`, which reads them (through solidcore's
//! Part 21 parser since #2789) and owns the types re-exported here.
//!
//! Provenance: our own glue; the reader's sources are named in
//! `camcraft_geometry::step_csys`. Nothing from any CAM vendor.

pub use camcraft_geometry::step_csys::{CsysRefusal, CsysRole, NamedCsys, ToolCsysReport};

use super::ImportError;

/// Read every named coordinate system of a STEP file. `Err` only when
/// the file itself can't be read (nesting past the cap, not UTF-8,
/// syntax, no DATA section, an entity id defined twice, a length unit
/// that can't be converted); a bad placement is a refusal in the report.
pub fn read_tool_csys(data: &[u8]) -> Result<ToolCsysReport, ImportError> {
    // #1075: the scan is the nesting law, stricter than the parser's own
    // depth cap (#2789), so it runs before the parser sees the file.
    super::step_depth::check(data)?;
    if data.is_empty() {
        return Err(ImportError::Empty);
    }
    let text = std::str::from_utf8(data)
        .map_err(|_| ImportError::Step("file is not valid UTF-8".into()))?;
    camcraft_geometry::step_csys::read_tool_csys(text).map_err(ImportError::Step)
}
