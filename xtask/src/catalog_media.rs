//! Off-screen screenshot hook (onto an images-only branch, never into the
//! product tree) for `cargo xtask catalog --screenshots` (#2392, #2393).
//!
//! Provenance: our own build tooling over `camcraft-ui`'s headless WGPU
//! capture and git index plumbing.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::catalog::CatalogCase;
use crate::exec;

pub const DEFAULT_SCREENSHOTS_BRANCH: &str = "catalog-screenshots";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MediaArgs {
    pub screenshots: bool,
    pub with_guide: bool,
    pub screenshots_branch: Option<String>,
    pub screenshots_out: Option<PathBuf>,
    pub pdf: bool,
    pub pdf_out: Option<PathBuf>,
}

pub fn run_screenshots(
    root: &Path,
    cases: &[CatalogCase],
    media: &MediaArgs,
    case_filter: Option<&str>,
) -> Result<PathBuf, String> {
    let branch = media
        .screenshots_branch
        .as_deref()
        .unwrap_or(DEFAULT_SCREENSHOTS_BRANCH);
    validate_screenshots_branch(root, branch)?;

    let scratch = std::env::temp_dir().join(format!("xtask-catalog-shots-{}", std::process::id()));
    let out_dir = match &media.screenshots_out {
        Some(dir) => {
            let dir = resolve_approx(dir)?;
            ensure_outside_tree(root, &dir, "--screenshots-out")?;
            dir
        }
        None => {
            let _ = fs::remove_dir_all(&scratch);
            scratch.clone()
        }
    };
    fs::create_dir_all(&out_dir)
        .map_err(|e| format!("create screenshot output {}: {e}", out_dir.display()))?;

    run_catalog_renderer(root, &out_dir, case_filter)?;
    if media.with_guide {
        run_guide_renderer_hook(root, &out_dir)?;
    }

    let pngs = collect_pngs(&out_dir, cases, media.with_guide)?;
    if pngs.is_empty() {
        return Err(
            "catalog screenshots: no PNGs rendered (missing GPU/software adapter; install Mesa/Vulkan or set CAMCRAFT_REQUIRE_GPU=1)"
                .into(),
        );
    }

    commit_images_to_branch(root, branch, &pngs)?;
    println!(
        "committed {} screenshot(s) to images-only branch '{branch}'",
        pngs.len()
    );
    Ok(out_dir)
}

fn run_catalog_renderer(
    root: &Path,
    out_dir: &Path,
    case_filter: Option<&str>,
) -> Result<(), String> {
    let mut cmd = Command::new(exec::cargo());
    cmd.current_dir(root.join("crates").join("ui"))
        .env("CAMCRAFT_CATALOG_SCREENSHOTS_OUT", out_dir)
        .args([
            "test",
            "-p",
            "camcraft-ui",
            "--test",
            "catalog_render",
            "--",
            "render_all_catalog_screenshots",
            "--exact",
            "--ignored",
            "--nocapture",
        ]);
    if let Some(c) = case_filter {
        cmd.env("CAMCRAFT_CATALOG_CASE", c);
    }
    for (k, v) in exec::gate_cache_env() {
        cmd.env(k, v);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("failed to run catalog_render: {e}"))?;
    if !status.success() {
        return Err(format!("camcraft-ui catalog_render exited with {status}"));
    }
    Ok(())
}

/// Optional hook into #1902's `guide_images` off-screen renderer when present.
fn run_guide_renderer_hook(root: &Path, out_dir: &Path) -> Result<(), String> {
    let guide_mod = root.join("crates/ui/src/egui_app/tests/guide_images.rs");
    if !guide_mod.is_file() {
        return Err(format!(
            "--with-guide requires #1902's renderer ({} not found)",
            guide_mod.display()
        ));
    }
    let guide_out = out_dir.join("guide");
    let proj_out = std::env::temp_dir().join(format!("xtask-guide-projs-{}", std::process::id()));
    fs::create_dir_all(&guide_out)
        .map_err(|e| format!("create {}: {e}", guide_out.display()))?;
    fs::create_dir_all(&proj_out)
        .map_err(|e| format!("create {}: {e}", proj_out.display()))?;
    let mut cmd = Command::new(exec::cargo());
    cmd.current_dir(root.join("crates").join("ui"))
        .env("CAMCRAFT_GUIDE_IMAGES_OUT", &guide_out)
        .env("CAMCRAFT_GUIDE_PROJECTS_OUT", &proj_out)
        .args([
            "test",
            "-p",
            "camcraft-ui",
            "--lib",
            "--",
            "egui_app::tests::guide_images::render_all_guide_images",
            "--exact",
            "--ignored",
            "--nocapture",
        ]);
    for (k, v) in exec::gate_cache_env() {
        cmd.env(k, v);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("failed to run #1902 guide_images renderer: {e}"))?;
    let _ = fs::remove_dir_all(&proj_out);
    if !status.success() {
        return Err(format!("#1902 guide_images renderer exited with {status}"));
    }
    Ok(())
}

fn collect_pngs(
    out_dir: &Path,
    cases: &[CatalogCase],
    with_guide: bool,
) -> Result<Vec<(String, PathBuf)>, String> {
    let mut out = Vec::new();
    for c in cases {
        let p = out_dir
            .join(&c.family)
            .join(&c.name)
            .join(format!("{}.png", c.name));
        if p.is_file() {
            let rel = format!("catalog/{}/{}/{}.png", c.family, c.name, c.name);
            out.push((rel, p));
        }
    }
    if with_guide {
        let guide_dir = out_dir.join("guide");
        if guide_dir.is_dir() {
            let rd = fs::read_dir(&guide_dir)
                .map_err(|e| format!("read {}: {e}", guide_dir.display()))?;
            for entry in rd.filter_map(Result::ok) {
                let p = entry.path();
                if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png"))
                    && let Some(name) = p.file_name().and_then(|s| s.to_str())
                {
                    out.push((format!("guide/{name}"), p));
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

pub fn validate_screenshots_branch(root: &Path, branch: &str) -> Result<(), String> {
    let b = branch.trim();
    if b.is_empty()
        || b.starts_with('-')
        || b.contains("..")
        || b.chars().any(char::is_whitespace)
    {
        return Err(format!("invalid screenshots branch name '{branch}'"));
    }
    if matches!(b, "main" | "master" | "HEAD") {
        return Err(format!(
            "refusing to write screenshots to '{b}'; use an images-only branch"
        ));
    }
    if let Ok(head) = exec::git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        && head.trim() == b
    {
        return Err(format!(
            "refusing to write screenshots onto the checked-out branch '{b}'"
        ));
    }
    let ref_name = format!("refs/heads/{b}");
    let exists = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["show-ref", "--verify", "--quiet", &ref_name])
        .status()
        .map_err(|e| format!("git show-ref: {e}"))?
        .success();
    if exists {
        let top = exec::git(root, &["ls-tree", "--name-only", &ref_name])?;
        if top.lines().any(|l| matches!(l.trim(), "Cargo.toml" | "crates" | "AGENTS.md")) {
            return Err(format!(
                "branch '{b}' holds product source files; choose an images-only branch"
            ));
        }
    }
    Ok(())
}

pub fn commit_images_to_branch(
    root: &Path,
    branch: &str,
    files: &[(String, PathBuf)],
) -> Result<String, String> {
    validate_screenshots_branch(root, branch)?;
    if files.is_empty() {
        return Err("no screenshot files to commit".into());
    }
    let idx = std::env::temp_dir().join(format!(
        "xtask-catalog-idx-{}-{}",
        std::process::id(),
        branch.replace('/', "_")
    ));
    let _ = fs::remove_file(&idx);
    let ref_name = format!("refs/heads/{branch}");
    let parent = exec::git(root, &["rev-parse", "--verify", "--quiet", &ref_name])
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    let res = (|| -> Result<String, String> {
        if let Some(p) = &parent {
            run_git_idx(root, &idx, &["read-tree", p])?;
        }
        for (rel, disk) in files {
            let oid = exec::git(root, &["hash-object", "-w", &disk.to_string_lossy()])?;
            let spec = format!("100644,{},{}", oid.trim(), rel);
            run_git_idx(root, &idx, &["update-index", "--add", "--cacheinfo", &spec])?;
        }
        let tree = run_git_idx(root, &idx, &["write-tree"])?;
        let mut commit_args = vec![
            "-c",
            "user.name=CamCraft Catalog",
            "-c",
            "user.email=catalog@camcraft.invalid",
            "commit-tree",
            tree.trim(),
        ];
        if let Some(p) = &parent {
            commit_args.push("-p");
            commit_args.push(p);
        }
        commit_args.push("-m");
        commit_args.push("docs(catalog): update rendered catalog screenshots");
        let commit_oid = exec::git(root, &commit_args)?;
        let commit_oid = commit_oid.trim().to_owned();
        exec::git(root, &["update-ref", &ref_name, &commit_oid])?;
        Ok(commit_oid)
    })();
    let _ = fs::remove_file(&idx);
    res
}

fn run_git_idx(root: &Path, idx: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .env("GIT_INDEX_FILE", idx)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn ensure_outside_tree(root: &Path, path: &Path, flag: &str) -> Result<(), String> {
    let root_canon = root.canonicalize().map_err(|e| format!("canonicalize {}: {e}", root.display()))?;
    let resolved = resolve_approx(path)?;
    if resolved == root_canon || resolved.starts_with(&root_canon) {
        return Err(format!(
            "{flag} must point outside the repository tree (got {})",
            path.display()
        ));
    }
    Ok(())
}

pub fn resolve_approx(path: &Path) -> Result<PathBuf, String> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("cwd: {e}"))?
            .join(path)
    };
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = abs.as_path();
    while !cur.exists() {
        let Some(name) = cur.file_name() else {
            break;
        };
        tail.push(name.to_owned());
        cur = cur.parent().unwrap_or(cur);
    }
    let mut base = cur
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {e}", cur.display()))?;
    for part in tail.into_iter().rev() {
        if matches!(Component::Normal(&part), Component::Normal(_)) {
            base.push(part);
        }
    }
    Ok(base)
}

pub fn find_on_path(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in names {
            let candidate = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}
