//! `cargo xtask process-bump`: the one writer of `.claude/PROCESS_VERSION` (#1153). Process PRs no
//! longer carry the bump; the merging session runs this after a merge batch that changed process files.
//! Provenance: our own process law (AGENTS.md "Process changes bump PROCESS_VERSION").

use std::path::Path;

const FILE: &str = ".claude/PROCESS_VERSION";

/// The text after the bump: the single integer in `text` plus one, newline-terminated. A file that is
/// not exactly one non-negative integer is refused rather than guessed at.
pub fn bumped(text: &str) -> Result<String, String> {
    let t = text.trim_start_matches('\u{feff}').trim();
    let n: u64 = t.parse().map_err(|_| format!("{FILE} must hold one integer, got '{t}'"))?;
    let next = n.checked_add(1).ok_or_else(|| format!("{FILE} overflows at {n}"))?;
    Ok(format!("{next}\n"))
}

/// Rewrites the file under `root` with the next number; returns `old -> new` for the operator.
pub fn run(root: &Path, args: &[String]) -> Result<i32, String> {
    if let Some(a) = args.first() {
        return Err(format!("process-bump takes no arguments, got '{a}'"));
    }
    let path = root.join(FILE);
    let old = std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let new = bumped(&old)?;
    std::fs::write(&path, &new).map_err(|e| format!("write {}: {e}", path.display()))?;
    println!("PROCESS_VERSION {} -> {}", old.trim(), new.trim());
    Ok(0)
}
