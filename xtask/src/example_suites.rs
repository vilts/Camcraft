//! The quick tier's shipped-example rule (#3521): a change to a post script, a machine file, a shipped
//! example, or the post or toolpath engine runs the app suites that post or read the shipped examples.
//! Provenance: our own build law (AGENTS.md "Test the change, not everything"); the misses of PR #3478
//! (`example_catalog`) and PR #3464 (`first_part_programs`, red main #3572).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Inputs that change what a shipped example posts: post scripts, machine files, the shipped examples
/// themselves, and the post and toolpath engines.
const INPUTS: [&str; 5] = ["assets/posts/", "assets/machines/", "examples/", "crates/post/src/", "crates/motion/src/"];

/// The `crates/app/tests` binaries that post a shipped example under `examples/`, compare a shipped `.nc`
/// byte for byte, or regenerate a catalog case. A binary that only reads an example as input
/// (`agie_native`) is not here.
pub const SUITES: [&str; 9] = [
    "example_catalog",
    "example_catalog_adaptive",
    "example_catalog_edge_work",
    "first_part_programs",
    "first_router_part",
    "first_4axis_part",
    "continuous_4axis_part",
    "tilt_basis_compat",
    "catalog_groove_part_off",
];

/// Adds the shipped-example suites (each one that exists) as whole app suites when the diff touches one
/// of [`INPUTS`]. The catalog rule (#2393) runs first, so `examples/catalog/` keeps its own reason.
pub fn map(root: &Path, changed: &[String], whole: &mut BTreeSet<String>, why: &mut BTreeMap<String, String>) {
    crate::catalog::map_catalog(root, changed, whole, why);
    let Some(hit) = changed.iter().find(|f| INPUTS.iter().any(|p| f.starts_with(p))) else {
        return;
    };
    for bin in SUITES {
        if !root.join(format!("crates/app/tests/{bin}.rs")).is_file() {
            continue;
        }
        let key = format!("app:{bin}");
        why.entry(key.clone()).or_insert_with(|| format!("{hit} can change a shipped example (#3521)"));
        whole.insert(key);
    }
}
