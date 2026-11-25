//! The stale gate base warning (#3739): the quick and heavy tiers diff against a local branch that a
//! worktree agent never pulls (#1201), so it warns when that branch is behind its remote copies.
//! Provenance: our own process law (AGENTS.md "A worktree agent never touches the main checkout's HEAD").

use std::path::Path;

use crate::exec;

/// The remotes whose copy of the base branch is compared, in the order the warnings name them.
const REMOTES: [&str; 2] = ["upstream", "origin"];

/// One warning line for each remote copy of `base` that has commits the local `base` lacks. Empty when
/// `base` is not a local branch (a remote ref or a sha), when no remote copy exists, or when git cannot
/// tell: the check only warns, it never stops the gate. It reads the refs of the last `git fetch`.
pub fn warnings(root: &Path, base: &str) -> Vec<String> {
    let local = format!("refs/heads/{base}");
    REMOTES
        .iter()
        .filter_map(|remote| {
            let theirs = format!("refs/remotes/{remote}/{base}");
            // A missing local branch or remote copy makes rev-list fail: no warning.
            let range = format!("{local}..{theirs}");
            let count = exec::git(root, &["rev-list", "--count", &range]).ok()?;
            let behind: u64 = count.trim().parse().ok()?;
            warning(base, &format!("{remote}/{base}"), behind)
        })
        .collect()
}

/// The warning text for a local `base` that is `behind` commits behind `remote_ref`, or `None` at 0.
pub fn warning(base: &str, remote_ref: &str, behind: u64) -> Option<String> {
    (behind > 0).then(|| {
        let s = if behind == 1 { "" } else { "s" };
        format!(
            "warning: the local '{base}' is {behind} commit{s} behind '{remote_ref}' (as of the last fetch): \
             the diff can hold files that this branch did not change. Use --base {remote_ref} after git fetch."
        )
    })
}
