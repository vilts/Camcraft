//! Solidcraft design import (#1456): the final body a `.sc` design keeps in
//! its model cache, as a checked solidcore solid in millimetres. Only the
//! cached final body is read - no parameters, sketches or feature history -
//! and only when the cache is provably this document's: every version is
//! one we read, the cache's SHA-256 digests match the document and the
//! payload, and the body slice decodes. Anything else is refused by name,
//! never shown stale or in part.
//! Provenance: the container, model-cache and final-state layout are read
//! from Solidcraft's published source and documentation (tangency-labs/
//! solidcraft, Apache-2.0: docs/design-container.md "Model cache",
//! crates/io/src/design/native_cache.rs `read_geometry`, crates/doc/src/
//! model_cache.rs `SavedModel::final_state` and the state records). The
//! body encoding (`SCBREP05`) is decoded by solidcore-record's own
//! `native::decode` (solidcore workspace, Apache-2.0, at the commit pinned
//! in Cargo.toml). No layout was worked out from sample bytes; no HSMWorks
//! material consulted.

mod cache;
mod sha256;
mod zip;

use std::panic::{AssertUnwindSafe, catch_unwind};

use serde_json::Value;
use solidcore::kernel::CancelToken;

use crate::KernelError;
use crate::solidcore_impl::SolidcoreKernel;
use crate::solidcore_step::{StepSolid, build_record};

pub use sha256::hex_digest;
pub use zip::looks_like_zip;

const OP: &str = "import_solidcraft";

/// Largest `.sc` file read (Solidcraft's own per-input bound, 256 MiB).
pub const MAX_SC_BYTES: usize = 256 << 20;
/// Largest `document.json` and `cache/bodies.bin` (Solidcraft's per-part bound).
pub const MAX_PART_BYTES: usize = 256 << 20;
/// Largest `cache/model.json` (Solidcraft's manifest bound).
pub const MAX_MANIFEST_BYTES: usize = 32 << 20;
/// The farthest a final body's vertex may lie from the origin on any axis,
/// in mm (1 km: past every machine envelope; a body beyond it is damage,
/// not a part). The decoder's own bound is 1e12 (#1467).
pub const MAX_COORD_MM: f64 = 1.0e6;
/// Largest `container.json`.
const MAX_CONTAINER_BYTES: usize = 4096;

/// The container format read.
pub const CONTAINER_FORMAT: &str = "solidcraft-container/1";
/// The model-cache formats read: 8 (the sample in testdata) and 10 (the
/// published source); every other version is refused.
pub const CACHE_FORMATS: [&str; 2] = ["solidcraft-model-cache/8", "solidcraft-model-cache/10"];
/// The producer (body adapter) read.
pub const CACHE_PRODUCER: &str = "solidcraft-truck-adapter/4";
/// The kernel whose bodies are read.
pub const CACHE_KERNEL: &str = "solidcore";

/// A Solidcraft design's final body.
#[derive(Debug)]
pub struct SolidcraftImport {
    /// The final body: a checked solid in millimetres.
    pub solid: StepSolid,
    /// The design's own name (`document.json` `name`), may be empty.
    pub design: String,
    /// The document's length unit (always `"mm"`: others are refused).
    pub units: String,
    /// The Solidcraft version that wrote the cache (recorded, not compared).
    pub app: String,
}

pub(crate) fn refused(detail: impl Into<String>) -> KernelError {
    KernelError::InvalidInput {
        op: OP,
        detail: detail.into(),
    }
}

/// Imports the final body of a Solidcraft `.sc` design. `stop` is the time
/// bound (polled between stages and inside the kernel's loops). A panic in
/// the decoder or the kernel is caught and reported; the kernel state is
/// then replaced, as after a time-out.
pub fn import_solidcraft(
    k: &mut SolidcoreKernel,
    bytes: &[u8],
    stop: &CancelToken,
) -> Result<SolidcraftImport, KernelError> {
    if bytes.is_empty() {
        return Err(refused("empty file"));
    }
    if bytes.len() > MAX_SC_BYTES {
        return Err(refused(format!(
            "{} bytes is past the {MAX_SC_BYTES}-byte limit",
            bytes.len()
        )));
    }
    let before = k.k.cancel.replace(stop.clone());
    let guarded = catch_unwind(AssertUnwindSafe(|| import_guarded(k, bytes, stop)));
    k.k.cancel = before;
    let out = guarded.unwrap_or_else(|_| {
        Err(KernelError::Failed {
            kernel: CACHE_KERNEL,
            op: OP,
            detail: "the decoder or the kernel panicked on this file".into(),
        })
    });
    if matches!(
        out,
        Err(KernelError::TimedOut { .. } | KernelError::Failed { .. })
    ) {
        k.k = solidcore::Kernel::default();
    }
    out
}

fn timed_out(stage: &'static str) -> KernelError {
    KernelError::TimedOut { op: OP, stage }
}

fn json(bytes: &[u8], what: &str) -> Result<Value, KernelError> {
    serde_json::from_slice(bytes).map_err(|e| refused(format!("{what} is not valid JSON ({e})")))
}

fn import_guarded(
    k: &mut SolidcoreKernel,
    bytes: &[u8],
    stop: &CancelToken,
) -> Result<SolidcraftImport, KernelError> {
    let archive = zip::Archive::open(bytes).map_err(refused)?;
    let container = json(
        &archive
            .read("container.json", MAX_CONTAINER_BYTES)
            .map_err(refused)?,
        "container.json",
    )?;
    let format = container.get("format").and_then(Value::as_str);
    if format != Some(CONTAINER_FORMAT) {
        return Err(refused(format!(
            "container format {} is not {CONTAINER_FORMAT}",
            format.unwrap_or("(none)")
        )));
    }
    let document_bytes = archive
        .read("document.json", MAX_PART_BYTES)
        .map_err(refused)?;
    let document = json(&document_bytes, "document.json")?;
    let doc_format = document.get("format").and_then(Value::as_str).unwrap_or("");
    let doc_version = doc_format
        .strip_prefix("solidcraft/")
        .and_then(|v| v.parse::<u32>().ok());
    if doc_version.is_none() {
        return Err(refused(format!(
            "document format '{doc_format}' is not a Solidcraft document"
        )));
    }
    let units = document.get("units").and_then(Value::as_str).unwrap_or("");
    if units != "mm" {
        return Err(refused(format!(
            "document units '{units}': only millimetre designs are read (the format does not \
             say how a design in other units stores its bodies)"
        )));
    }
    if !archive.has(cache::MANIFEST) || !archive.has(cache::PAYLOAD) {
        return Err(refused(
            "the design keeps no model cache (cache/model.json and cache/bodies.bin): save it \
             in Solidcraft with the model cache on",
        ));
    }
    if stop.is_cancelled() {
        return Err(timed_out("reading the container"));
    }
    let manifest = archive
        .read(cache::MANIFEST, MAX_MANIFEST_BYTES)
        .map_err(refused)?;
    let manifest = json(&manifest, cache::MANIFEST)?;
    cache::check_key(&manifest, &document_bytes)?;
    let payload = archive
        .read(cache::PAYLOAD, MAX_PART_BYTES)
        .map_err(refused)?;
    let body = cache::final_body(&manifest, &payload)?;
    if stop.is_cancelled() {
        return Err(timed_out("decoding the final body"));
    }
    let solid =
        build_record(k, 0, body.name, body.record, Vec::new()).map_err(|(stage, why)| {
            if stop.is_cancelled() {
                timed_out("building the final body")
            } else {
                KernelError::InvalidResult {
                    kernel: CACHE_KERNEL,
                    op: OP,
                    detail: format!("the final body was refused at {stage}: {why}"),
                }
            }
        })?;
    Ok(SolidcraftImport {
        solid,
        design: document
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        units: units.to_owned(),
        app: manifest
            .get("app")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
    })
}
