//! Provenance: #3459; Git's detached worktrees and Cargo's metadata and JSON diagnostics.
//! Only a task-owned worktree is checked out. Cargo checks tests, never links test executables.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::json::{self, Json};
use crate::merge_plan::{Facts, Plan, Pr};

type Errors = BTreeMap<u32, Vec<String>>;
#[cfg(test)]
#[path = "merge_trial_tests.rs"]
mod tests;
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn output(command: &mut Command) -> Result<Output, String> {
    command.output().map_err(|e| format!("merge trial: {command:?}: {e}"))
}
fn successful(command: &mut Command) -> Result<String, String> {
    let out = output(command)?;
    if !out.status.success() {
        return Err(format!("merge trial: {command:?}: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

struct Worktree {
    root: PathBuf,
    path: PathBuf,
    active: bool,
}
impl Worktree {
    fn new(root: &Path, commit: &str) -> Result<Self, String> {
        let root = fs::canonicalize(root).map_err(|e| format!("trial root: {e}"))?;
        let parent = root.join(".claude/worktrees");
        fs::create_dir_all(&parent).map_err(|e| format!("trial directory: {e}"))?;
        let path = parent.join(format!(
            "merge-trial-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // Reserve the name atomically. Never reuse or remove an existing worktree.
        fs::create_dir(&path).map_err(|e| format!("reserve {}: {e}", path.display()))?;
        let mut tree = Self { root, path, active: true };
        if let Err(e) = successful(
            Command::new("git")
                .arg("-C")
                .arg(&tree.root)
                .args(["worktree", "add", "--detach"])
                .arg(&tree.path)
                .arg(commit),
        ) {
            // An unsuccessful add can leave either a registered worktree or the reserved directory.
            let _ = tree.close();
            return Err(e);
        }
        Ok(tree)
    }
    fn close(&mut self) -> Result<(), String> {
        let out = output(
            Command::new("git")
                .arg("-C")
                .arg(&self.root)
                .args(["worktree", "remove", "--force"])
                .arg(&self.path),
        )?;
        if !out.status.success() {
            // If add never registered the path, only an empty reservation may be removed.
            fs::remove_dir(&self.path).map_err(|e| {
                format!(
                    "remove trial {}: {e}; {}",
                    self.path.display(),
                    String::from_utf8_lossy(&out.stderr)
                )
            })?;
        }
        self.active = false;
        Ok(())
    }
}
impl Drop for Worktree {
    fn drop(&mut self) {
        if self.active
            && let Err(e) = self.close()
        {
            eprintln!("warning: {e}");
        }
    }
}

/// Read workspace members from Cargo rather than assuming that a directory and package have the same name.
fn packages(root: &Path, metadata: &str, paths: &[String]) -> Result<Vec<String>, String> {
    let meta = json::parse(metadata).ok_or("trial metadata is not JSON")?;
    let members = meta
        .get("workspace_members")
        .ok_or("trial metadata has no workspace_members")?
        .items();
    let mut packages = Vec::new();
    for p in meta.get("packages").ok_or("trial metadata has no packages")?.items() {
        if !members.iter().any(|m| m.str() == p.text("id")) {
            continue;
        }
        let name = p.text("name").ok_or("trial package has no name")?;
        let manifest = Path::new(p.text("manifest_path").ok_or("trial package has no manifest")?);
        let dir = manifest
            .parent()
            .ok_or("trial manifest has no parent")?
            .strip_prefix(root)
            .map_err(|e| format!("trial package is outside the workspace: {e}"))?;
        packages.push((name.to_string(), dir.to_path_buf()));
    }
    let mut selected = BTreeSet::new();
    for path in paths {
        let path = Path::new(path);
        if let Some((name, _)) = packages
            .iter()
            .filter(|(_, dir)| path.starts_with(dir))
            .max_by_key(|(_, dir)| dir.components().count())
        {
            selected.insert(name.clone());
        } else if !path.extension().is_some_and(|e| e == "md") && !path.starts_with("docs") && !path.starts_with(".claude") {
            // Workspace manifests, lock/toolchain/config files and shared assets can affect every member.
            selected.extend(packages.iter().map(|(name, _)| name.clone()));
        }
    }
    Ok(selected.into_iter().collect())
}

fn spans(message: &Json, root: &Path, paths: &mut BTreeSet<String>) {
    for span in message.get("spans").map(Json::items).unwrap_or(&[]) {
        if let Some(file) = span.text("file_name") {
            let file = Path::new(file);
            let rel = file.strip_prefix(root).unwrap_or(file).to_string_lossy().replace('\\', "/");
            paths.insert(rel);
        }
    }
    for child in message.get("children").map(Json::items).unwrap_or(&[]) {
        spans(child, root, paths);
    }
}

fn diagnostics(root: &Path, stdout: &str, prs: &[Pr]) -> Errors {
    let mut errors = Errors::new();
    for line in stdout.lines() {
        let Some(m) = json::parse(line) else { continue };
        let Some(message) = m.get("message").filter(|_| m.text("reason") == Some("compiler-message")) else {
            continue;
        };
        if message.text("level") != Some("error") {
            if let Some(rendered) = message.text("rendered") {
                eprint!("{rendered}");
            }
            continue;
        }
        let mut paths = BTreeSet::new();
        spans(message, root, &mut paths);
        let owners: Vec<_> = prs.iter().filter(|p| p.paths.iter().any(|f| paths.contains(f))).collect();
        // Do not guess a single culprit for errors in dependencies, macros or generated files.
        let owners = if owners.is_empty() { prs.iter().collect() } else { owners };
        let files = paths.into_iter().collect::<Vec<_>>().join(", ");
        eprintln!(
            "trial error: {} [{files}]\n{}",
            crate::merge_plan::refs(&owners.iter().map(|p| p.number).collect::<Vec<_>>()),
            message.text("rendered").unwrap_or("compiler error")
        );
        for p in owners {
            errors.entry(p.number).or_default().push(files.clone());
        }
    }
    errors
}

fn check(root: &Path, commit: &str, prs: &[Pr]) -> Result<Errors, String> {
    let mut tree = Worktree::new(root, commit)?;
    let result = (|| {
        let metadata = successful(
            Command::new("cargo")
                .current_dir(&tree.path)
                .args(["metadata", "--no-deps", "--format-version", "1"]),
        )?;
        let paths = prs.iter().flat_map(|p| p.paths.iter().cloned()).collect::<Vec<_>>();
        let selected = packages(&tree.path, metadata.trim(), &paths)?;
        let mut errors = Errors::new();
        // Check packages separately: do not unify unrelated features in a combined workspace build.
        for package in selected {
            eprintln!("trial: cargo check --tests -p {package} (tree {commit})");
            let out = output(
                Command::new("cargo")
                    .current_dir(&tree.path)
                    .args(["check", "--tests", "-p", &package, "--message-format=json", "--target-dir"])
                    .arg(root.join("target"))
                    .env("CARGO_INCREMENTAL", "0"),
            )?;
            eprint!("{}", String::from_utf8_lossy(&out.stderr));
            let found = diagnostics(&tree.path, &String::from_utf8_lossy(&out.stdout), prs);
            if !out.status.success() {
                if found.is_empty() {
                    eprintln!("trial: check failed without an owned compiler diagnostic; exclude the whole wave");
                    for p in prs {
                        errors
                            .entry(p.number)
                            .or_default()
                            .push(format!("unattributed check failure in {package}"));
                    }
                } else {
                    for (n, files) in found {
                        errors.entry(n).or_default().extend(files);
                    }
                }
            }
        }
        Ok(errors)
    })();
    tree.close()?;
    result
}

/// Replan and recheck the survivors. A failed check never becomes a proposed merge, including stacked children.
pub fn validate(root: &Path, facts: &Facts, mut plan_tree: impl FnMut(&Facts) -> Result<(Plan, String), String>) -> Result<Plan, String> {
    let mut remaining = facts.clone();
    let mut excluded = Errors::new();
    loop {
        let (mut plan, tree) = plan_tree(&remaining)?;
        let prs = remaining
            .prs
            .iter()
            .filter(|p| plan.wave1.contains(&p.number))
            .cloned()
            .collect::<Vec<_>>();
        let errors = if prs.is_empty() { Errors::new() } else { check(root, &tree, &prs)? };
        if errors.is_empty() {
            plan.checked = true;
            for files in excluded.values_mut() {
                files.sort();
                files.dedup();
            }
            for p in &facts.prs {
                plan.heads.insert(p.number, p.head.clone());
                plan.titles.insert(p.number, p.title.clone());
            }
            plan.excluded = excluded;
            return Ok(plan);
        }
        excluded.extend(errors);
        loop {
            let children = remaining
                .prs
                .iter()
                .filter(|p| {
                    !excluded.contains_key(&p.number)
                        && facts
                            .before
                            .get(&p.number)
                            .is_some_and(|parents| parents.iter().any(|n| excluded.contains_key(n)))
                })
                .map(|p| p.number)
                .collect::<Vec<_>>();
            if children.is_empty() {
                break;
            }
            for n in children {
                excluded.insert(n, vec!["parent excluded by trial check".into()]);
            }
        }
        remaining.prs.retain(|p| !excluded.contains_key(&p.number));
    }
}
