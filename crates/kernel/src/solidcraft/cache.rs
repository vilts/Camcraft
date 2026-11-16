//! The Solidcraft model cache (#1456): its key checked against the document
//! and payload, and the final state's one body found and decoded.
//! Provenance: Solidcraft's published source (tangency-labs/solidcraft,
//! Apache-2.0): the manifest fields, slice rules and body table of
//! crates/io/src/design/native_cache.rs (`read_geometry`), and the final
//! state of crates/doc/src/model_cache.rs (`SavedModel::final_state`: the
//! last feature not skipped names its state; a state lists its model
//! bodies as runs `[first, count]`; a model body names its entry of the
//! body table). Bodies are decoded by solidcore-record `native::decode`.

use serde_json::Value;

use super::sha256::hex_digest;
use super::{CACHE_FORMATS, CACHE_KERNEL, CACHE_PRODUCER, MAX_COORD_MM, refused};
use crate::KernelError;

pub const MANIFEST: &str = "cache/model.json";
pub const PAYLOAD: &str = "cache/bodies.bin";
/// Solidcraft's bounds: distinct stored bodies, and body records.
const MAX_UNIQUE: usize = 100_000;
const MAX_RECORDS: usize = 200_000;
/// A mesh body's slice starts with this tag instead of the B-rep codec's.
const MESH_TAG: &[u8; 8] = b"SCMESH01";

/// The final state's body: its name in the design and its B-rep record.
pub struct FinalBody {
    pub name: String,
    pub record: solidcore_record::brep::Brep,
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn index(v: Option<&Value>, len: usize, what: &str) -> Result<usize, KernelError> {
    v.and_then(Value::as_u64)
        .and_then(|i| usize::try_from(i).ok())
        .filter(|i| *i < len)
        .ok_or_else(|| refused(format!("model cache: {what} out of range")))
}

fn list<'a>(v: &'a Value, key: &str, cap: usize) -> Result<&'a Vec<Value>, KernelError> {
    v.get(key)
        .and_then(Value::as_array)
        .filter(|l| l.len() <= cap)
        .ok_or_else(|| refused(format!("model cache: '{key}' missing or too long")))
}

/// Every vertex of the final body lies within `MAX_COORD_MM` of the origin
/// on each axis, at finite coordinates. The decoder refuses non-finite
/// numbers, coordinates past 1e12 and tolerances outside (0, 1e3] itself;
/// this is the reader's own, tighter coordinate bound (#1467), checked
/// before the kernel builds anything.
fn check_vertices(b: &solidcore_record::brep::Brep) -> Result<(), KernelError> {
    for (i, v) in b.vertices.iter().enumerate() {
        if let Some(x) = v.point.iter().find(|x| !x.is_finite()) {
            return Err(refused(format!(
                "the final body's vertex {i} has a non-finite coordinate ({x})"
            )));
        }
        if let Some(x) = v.point.iter().find(|x| x.abs() > MAX_COORD_MM) {
            return Err(refused(format!(
                "the final body's vertex {i} lies at {x:e} mm, past the {MAX_COORD_MM:e} mm \
                 coordinate limit"
            )));
        }
    }
    Ok(())
}

/// The cache is one we read and is the evaluation of exactly this document
/// (`document` digest) with exactly this payload (`payload` digest, checked
/// in [`final_body`]).
pub fn check_key(manifest: &Value, document: &[u8]) -> Result<(), KernelError> {
    let format = text(manifest, "format");
    if !CACHE_FORMATS.contains(&format) {
        return Err(refused(format!(
            "model cache format '{format}' is not one this reader knows ({})",
            CACHE_FORMATS.join(", ")
        )));
    }
    let producer = text(manifest, "producer");
    if producer != CACHE_PRODUCER {
        return Err(refused(format!(
            "model cache producer '{producer}' is not {CACHE_PRODUCER}"
        )));
    }
    let kernel = text(manifest, "kernel");
    if kernel != CACHE_KERNEL {
        return Err(refused(format!(
            "model cache kernel '{kernel}' is not {CACHE_KERNEL}"
        )));
    }
    if text(manifest, "document") != hex_digest(document) {
        return Err(refused(
            "model cache is stale: its document digest does not match document.json",
        ));
    }
    Ok(())
}

/// The final state's single body, decoded from `payload`.
pub fn final_body(manifest: &Value, payload: &[u8]) -> Result<FinalBody, KernelError> {
    if text(manifest, "payload") != hex_digest(payload) {
        return Err(refused(
            "model cache payload digest does not match cache/bodies.bin",
        ));
    }
    // The slices: contiguous, in order, covering the payload exactly.
    let slices = list(manifest, "slices", MAX_UNIQUE)?;
    let mut ranges = Vec::with_capacity(slices.len());
    let mut end = 0usize;
    for s in slices {
        let num = |i: usize| {
            s.get(i)
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| refused("model cache: a slice is not two counts"))
        };
        let (offset, length) = (num(0)?, num(1)?);
        if offset != end {
            return Err(refused("model cache: slices overlap or leave gaps"));
        }
        end = offset
            .checked_add(length)
            .filter(|e| *e <= payload.len())
            .ok_or_else(|| refused("model cache: a slice runs past cache/bodies.bin"))?;
        ranges.push((offset, end));
    }
    if end != payload.len() {
        return Err(refused("model cache: trailing data after the last slice"));
    }
    let table = list(manifest, "bodies", MAX_RECORDS)?;
    let model = manifest
        .get("model")
        .ok_or_else(|| refused("model cache: no model records"))?;
    let features = list(model, "features", usize::MAX)?;
    let states = list(model, "states", usize::MAX)?;
    let model_bodies = list(model, "model_bodies", MAX_RECORDS)?;
    // The final state: the last feature not skipped names it.
    let last = features
        .iter()
        .rev()
        .find(|f| f.get("skipped").and_then(Value::as_bool) == Some(false))
        .ok_or_else(|| refused("the design has no evaluated feature, so no body"))?;
    let state = states
        .get(index(last.get("state"), states.len(), "final state")?)
        .ok_or_else(|| refused("model cache: final state missing"))?;
    let mut held = Vec::new();
    for run in state
        .get("bodies")
        .and_then(Value::as_array)
        .ok_or_else(|| refused("model cache: final state lists no bodies"))?
    {
        let first = index(run.get(0), model_bodies.len(), "body run")?;
        let count = run
            .get(1)
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| {
                first
                    .checked_add(*n)
                    .is_some_and(|e| e <= model_bodies.len())
            })
            .ok_or_else(|| refused("model cache: body run out of range"))?;
        held.extend(first..first + count);
        if held.len() > 1 {
            break;
        }
    }
    let m = match held.as_slice() {
        [m] => *m,
        [] => return Err(refused("the design's final state holds no body")),
        _ => {
            return Err(refused(
                "the design's final state holds several bodies: multi-body designs are not \
                 imported yet",
            ));
        }
    };
    let mb = model_bodies
        .get(m)
        .ok_or_else(|| refused("model cache: model body missing"))?;
    let entry = table
        .get(index(mb.get("body"), table.len(), "body table entry")?)
        .and_then(Value::as_array)
        .ok_or_else(|| refused("model cache: body table entry is not a list"))?;
    if entry.len() != 1 {
        return Err(refused(
            "the final body is a placed copy of a part (an assembly instance): not imported yet",
        ));
    }
    let (start, stop) = *ranges
        .get(index(entry.first(), ranges.len(), "body slice")?)
        .ok_or_else(|| refused("model cache: body slice missing"))?;
    let bytes = payload
        .get(start..stop)
        .ok_or_else(|| refused("model cache: body slice missing"))?;
    if bytes.starts_with(MESH_TAG) {
        return Err(refused(
            "the final body is a mesh body (an imported STL or 3MF), not a B-rep: import that mesh file instead",
        ));
    }
    if !bytes.starts_with(solidcore_record::native::MAGIC) {
        return Err(refused(
            "the final body has a bad magic: not a solidcore B-rep record (SCBREP05)",
        ));
    }
    let saved = solidcore_record::native::decode(bytes)
        .map_err(|e| refused(format!("the final body does not decode: {e}")))?;
    check_vertices(&saved.solid)?;
    Ok(FinalBody {
        name: text(mb, "name").to_owned(),
        record: saved.solid,
    })
}
