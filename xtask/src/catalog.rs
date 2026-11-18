//! `cargo xtask catalog` (#2392, #2393): rebuild every example catalog case's
//! `<case>.camproj` and `<case>.nc` from `build.camscript`, check layout/parity
//! (`--check`), render off-screen screenshots onto an images-only branch
//! (`--screenshots`), and optionally build per-case PDFs (`--pdf`).
//!
//! Provenance: our own build/regression harness over `camcraft-cli`,
//! `camcraft-ui` off-screen rendering, git plumbing, pandoc, and headless Chrome.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::catalog_media::{self, MediaArgs};
use crate::catalog_pdf;
use crate::exec;

const USAGE: &str = "\
usage:
  cargo xtask catalog [--case <family/case>] [--check]
                      [--screenshots [--screenshots-branch <branch>] [--screenshots-out <dir>] [--with-guide]]
                      [--pdf [--pdf-out <dir>]] [--rebuild]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogCase {
    pub family: String,
    pub name: String,
    pub dir: PathBuf,
}

impl CatalogCase {
    pub fn id(&self) -> String {
        format!("{}/{}", self.family, self.name)
    }

    pub fn project_path(&self) -> PathBuf {
        self.dir.join(format!("{}.camproj", self.name))
    }

    pub fn nc_path(&self) -> PathBuf {
        self.dir.join(format!("{}.nc", self.name))
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Args {
    pub case: Option<String>,
    pub check: bool,
    pub rebuild: bool,
    pub media: MediaArgs,
}

pub fn parse(raw: &[String]) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = raw.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--case" => {
                let val = it.next().ok_or("--case requires <family/case>")?;
                out.case = Some(val.clone());
            }
            "--check" => out.check = true,
            "--rebuild" => out.rebuild = true,
            "--screenshots" => out.media.screenshots = true,
            "--with-guide" => out.media.with_guide = true,
            "--screenshots-branch" => {
                let val = it.next().ok_or("--screenshots-branch requires a branch name")?;
                out.media.screenshots_branch = Some(val.clone());
            }
            "--screenshots-out" => {
                let val = it.next().ok_or("--screenshots-out requires a directory path")?;
                out.media.screenshots_out = Some(PathBuf::from(val));
            }
            "--pdf" => out.media.pdf = true,
            "--pdf-out" => {
                let val = it.next().ok_or("--pdf-out requires a directory path")?;
                out.media.pdf_out = Some(PathBuf::from(val));
            }
            "-h" | "--help" => return Err(format!("help\n{USAGE}")),
            other => return Err(format!("unknown catalog option '{other}'\n{USAGE}")),
        }
    }
    if out.check && out.rebuild {
        return Err("--check and --rebuild cannot be used together".into());
    }
    if out.media.with_guide && !out.media.screenshots {
        return Err("--with-guide requires --screenshots".into());
    }
    Ok(out)
}

pub fn run(root: &Path, raw: &[String]) -> Result<i32, String> {
    if raw.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return Ok(0);
    }
    let args = parse(raw)?;
    let cases = select_cases(root, args.case.as_deref())?;
    let do_rebuild = args.rebuild || (!args.check && !args.media.screenshots && !args.media.pdf);

    if args.check {
        check_cases(root, &cases)?;
    } else if do_rebuild {
        rebuild_cases(root, &cases)?;
    }

    let rendered = if args.media.screenshots {
        Some(catalog_media::run_screenshots(root, &cases, &args.media, args.case.as_deref())?)
    } else {
        None
    };

    if args.media.pdf {
        let pdf_res = catalog_pdf::run_pdf(root, &cases, &args.media, rendered.as_deref());
        if args.media.screenshots_out.is_none()
            && let Some(dir) = rendered.as_ref()
        {
            let _ = fs::remove_dir_all(dir);
        }
        pdf_res?;
    } else if args.media.screenshots_out.is_none()
        && let Some(dir) = rendered.as_ref()
    {
        let _ = fs::remove_dir_all(dir);
    }

    Ok(0)
}

/// Changed catalog case files (`examples/catalog/...`) select `app:example_catalog` (#2393).
pub fn map_catalog(
    root: &Path,
    changed: &[String],
    whole: &mut BTreeSet<String>,
    why: &mut BTreeMap<String, String>,
) {
    if changed.iter().any(|f| f.starts_with("examples/catalog/"))
        && root.join("crates/app/tests/example_catalog.rs").is_file()
    {
        whole.insert("app:example_catalog".into());
        why.entry("app:example_catalog".into())
            .or_insert_with(|| "examples/catalog/ changed".into());
    }
}

pub fn discover_cases(root: &Path) -> Result<Vec<CatalogCase>, String> {
    let catalog = root.join("examples").join("catalog");
    if !catalog.is_dir() {
        return Err(format!("catalog root missing: {}", catalog.display()));
    }
    let mut families = sorted_subdirs(&catalog)?;
    families.sort();
    let mut cases = Vec::new();
    for fam_dir in families {
        let family = fam_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_owned();
        let mut sub = sorted_subdirs(&fam_dir)?;
        sub.sort();
        for dir in sub {
            let name = dir
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_owned();
            cases.push(CatalogCase { family: family.clone(), name, dir });
        }
    }
    if cases.is_empty() {
        return Err(format!("no cases found under {}", catalog.display()));
    }
    Ok(cases)
}

pub fn select_cases(root: &Path, filter: Option<&str>) -> Result<Vec<CatalogCase>, String> {
    let all = discover_cases(root)?;
    let Some(want) = filter else {
        return Ok(all);
    };
    let want_norm = want.replace('\\', "/");
    let matched: Vec<_> = all.into_iter().filter(|c| c.id() == want_norm).collect();
    if matched.is_empty() {
        return Err(format!("no catalog case named '{want}' under examples/catalog/"));
    }
    Ok(matched)
}

fn sorted_subdirs(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let rd = fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    let mut out = Vec::new();
    for entry in rd {
        let entry = entry.map_err(|e| format!("read entry in {}: {e}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

pub fn validate_layout(case: &CatalogCase, require_outputs: bool) -> Result<(), String> {
    let id = case.id();
    let model = case.dir.join("model");
    if !model.is_dir() {
        return Err(format!("{id}: missing model/ directory"));
    }
    let has_model_file = fs::read_dir(&model)
        .map_err(|e| format!("{id}: read model/: {e}"))?
        .filter_map(Result::ok)
        .any(|e| e.path().is_file());
    if !has_model_file {
        return Err(format!("{id}: model/ contains no files"));
    }
    for req in ["build.camscript", "README.md"] {
        require_non_empty(&case.dir.join(req), &id, req)?;
    }
    if require_outputs {
        let proj = format!("{}.camproj", case.name);
        let nc = format!("{}.nc", case.name);
        require_non_empty(&case.project_path(), &id, &proj)?;
        require_non_empty(&case.nc_path(), &id, &nc)?;
    }
    Ok(())
}

fn require_non_empty(path: &Path, id: &str, label: &str) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!("{id}: missing {label}"));
    }
    let len = fs::metadata(path)
        .map_err(|e| format!("{id}: stat {label}: {e}"))?
        .len();
    if len == 0 {
        return Err(format!("{id}: empty {label}"));
    }
    Ok(())
}

fn build_cli(root: &Path) -> Result<PathBuf, String> {
    let mut cmd = Command::new(exec::cargo());
    cmd.current_dir(root)
        .args(["build", "-p", "camcraft-app", "--bin", "camcraft-cli"]);
    for (k, v) in exec::gate_cache_env() {
        cmd.env(k, v);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("failed to build camcraft-cli: {e}"))?;
    if !status.success() {
        return Err(format!("cargo build camcraft-cli exited with {status}"));
    }
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let bin = target_dir
        .join("debug")
        .join(format!("camcraft-cli{}", std::env::consts::EXE_SUFFIX));
    if !bin.is_file() {
        return Err(format!("built camcraft-cli missing at {}", bin.display()));
    }
    Ok(bin)
}

fn run_case_script(cli: &Path, case_dir: &Path, id: &str) -> Result<(), String> {
    let out = Command::new(cli)
        .current_dir(case_dir)
        .args(["script", "build.camscript"])
        .output()
        .map_err(|e| format!("{id}: failed to run camcraft-cli: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{id}: build.camscript failed ({})\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

fn rebuild_cases(root: &Path, cases: &[CatalogCase]) -> Result<(), String> {
    for c in cases {
        validate_layout(c, false)?;
    }
    let cli = build_cli(root)?;
    for c in cases {
        run_case_script(&cli, &c.dir, &c.id())?;
        validate_layout(c, true)?;
        println!("rebuilt {}", c.id());
    }
    Ok(())
}

fn check_cases(root: &Path, cases: &[CatalogCase]) -> Result<(), String> {
    for c in cases {
        validate_layout(c, true)?;
    }
    let cli = build_cli(root)?;
    let scratch = std::env::temp_dir().join(format!("xtask-catalog-check-{}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);
    let res = (|| -> Result<(), String> {
        for c in cases {
            let work = scratch.join(&c.family).join(&c.name);
            copy_dir_all(&c.dir, &work)?;
            let _ = fs::remove_file(work.join(format!("{}.camproj", c.name)));
            let _ = fs::remove_file(work.join(format!("{}.nc", c.name)));
            run_case_script(&cli, &work, &c.id())?;
            let want = fs::read(c.nc_path())
                .map_err(|e| format!("{}: read committed .nc: {e}", c.id()))?;
            let got = fs::read(work.join(format!("{}.nc", c.name)))
                .map_err(|e| format!("{}: read rebuilt .nc: {e}", c.id()))?;
            if got != want {
                return Err(format!(
                    "{}: rebuilt NC differs from committed {} (run `cargo xtask catalog --case {}`)",
                    c.id(),
                    c.nc_path().display(),
                    c.id()
                ));
            }
            println!("checked {}", c.id());
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&scratch);
    res
}

pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("mkdir {}: {e}", dst.display()))?;
    let rd = fs::read_dir(src).map_err(|e| format!("read {}: {e}", src.display()))?;
    for entry in rd {
        let entry = entry.map_err(|e| format!("read entry in {}: {e}", src.display()))?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &target)?;
        } else {
            fs::copy(&path, &target)
                .map_err(|e| format!("copy {} -> {}: {e}", path.display(), target.display()))?;
        }
    }
    Ok(())
}
