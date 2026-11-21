//! The light tier (`cargo xtask gate light`, #2569): the light twins of the heavy tests, in release, in a
//! few minutes. A light twin is an `#[ignore = "light lane: ..."]` test named `<full test>_light`, next to
//! its full heavy test, on a smaller fixture or a reduced set.
//! Provenance: our own build law (AGENTS.md "Three gate tiers", "The light lane"); libtest's public
//! `--ignored` flag and name filters.

use std::path::Path;

use crate::plan::{self, Plan, Step};
use crate::scan;

/// The start of the ignore reason that marks a light twin.
pub const MARK: &str = "light lane";
/// The end of a light twin's name: `<full test>_light`. The libtest filter of a light step relies on it.
pub const SUFFIX: &str = "_light";
/// The name of the one build step of the light plan.
pub const BUILD: &str = "light build (release --no-run)";

/// One light twin: the crate dir, the test target (`lib` or the integration binary), the fn name and
/// the source file (from the workspace root, `/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Twin {
    pub dir: String,
    pub target: String,
    pub test: String,
    pub file: String,
}

/// True when one of the attribute lines of a test is an ignore whose reason starts with `mark`.
fn ignored_as(lines: &[&str], t: &scan::TestFn, mark: &str) -> bool {
    let want = format!("#[ignore = \"{mark}");
    lines[t.start.saturating_sub(1)..t.end.min(lines.len())].iter().any(|l| l.trim().starts_with(&want))
}

/// The light twins of one source text. A twin is refused by name when its name does not end in
/// `_light`, or when the file has no ignored test `<name without _light>` that is not a twin itself:
/// the full test keeps the golden asserts, so a twin never stands in for it.
pub fn parse(text: &str, dir: &str, target: &str, file: &str) -> Result<Vec<Twin>, String> {
    if !text.contains(MARK) {
        return Ok(Vec::new());
    }
    let lines: Vec<&str> = text.lines().collect();
    let fns = scan::test_fns(text)?;
    let mut out = Vec::new();
    for t in fns.iter().filter(|t| ignored_as(&lines, t, MARK)) {
        let Some(full) = t.name.strip_suffix(SUFFIX).filter(|f| !f.is_empty()) else {
            return Err(format!("light twin {} in {file}: its name must be <full test>{SUFFIX}", t.name));
        };
        let has_full = fns.iter().any(|f| f.name == full && f.ignored && !ignored_as(&lines, f, MARK));
        if !has_full {
            return Err(format!(
                "light twin {} in {file}: the file has no #[ignore]d full test {full} (the full test stays in the heavy lane, next to its twin)",
                t.name
            ));
        }
        out.push(Twin { dir: dir.into(), target: target.into(), test: t.name.clone(), file: file.into() });
    }
    Ok(out)
}

/// Each light twin of the heavy crates (`plan::HEAVY_CRATES`), in crate, tree and file order.
pub fn discover(root: &Path) -> Result<Vec<Twin>, String> {
    let mut out = Vec::new();
    for dir in plan::HEAVY_CRATES {
        let bins = scan::binaries(root, dir);
        for tree in ["src", "tests"] {
            let base = root.join("crates").join(dir).join(tree);
            for file in scan::rs_files_recursive(&base) {
                let text = scan::read(&file);
                if !text.contains(MARK) {
                    continue;
                }
                let rel = file.strip_prefix(root).unwrap_or(&file).to_string_lossy().replace('\\', "/");
                let target = if tree == "src" {
                    "lib".to_string()
                } else {
                    let first = file.strip_prefix(&base).ok().and_then(|p| p.components().next());
                    let first = first.map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
                    first.trim_end_matches(".rs").to_string()
                };
                let twins = parse(&text, dir, &target, &rel)?;
                if !twins.is_empty() && tree == "tests" && !bins.contains(&target) {
                    return Err(format!("light twin in {rel}: {target} is not a test binary of camcraft-{dir}"));
                }
                out.extend(twins);
            }
        }
    }
    Ok(out)
}

/// The light plan: one release build of the crates that hold a twin, then one step for each test target
/// with a twin. A step runs only the twins of its target (`--ignored <name>_light ...`).
pub fn plan(root: &Path) -> Result<Plan, String> {
    let twins = discover(root)?;
    if twins.is_empty() {
        return Err(format!("gate light: no test with the ignore reason \"{MARK}: ...\" in crates/{{{}}}", plan::HEAVY_CRATES.join(",")));
    }
    let mut out = Plan::default();
    out.notes.push(format!(
        "light: {} light twin(s) of the heavy tests, in release; a red step means: run its full heavy lane locally",
        twins.len()
    ));
    let mut targets: Vec<(&str, &str)> = Vec::new();
    for t in &twins {
        if !targets.contains(&(t.dir.as_str(), t.target.as_str())) {
            targets.push((&t.dir, &t.target));
        }
    }
    let mut build: Vec<String> = vec!["test".into(), "--release".into(), "-j".into(), plan::J.into()];
    for d in plan::HEAVY_CRATES.iter().filter(|d| targets.iter().any(|(dir, _)| dir == *d)) {
        build.push("-p".into());
        build.push(plan::pkg(d));
    }
    build.push("--no-run".into());
    out.steps.push(Step::new(BUILD, &[], "").with(&build));
    for (dir, target) in targets {
        let p = plan::pkg(dir);
        let mut args = vec!["test", "--release", "-j", plan::J, "-p", &p, "--no-fail-fast"];
        match target {
            "lib" => args.push("--lib"),
            bin => args.extend(["--test", bin]),
        }
        args.extend(["--", plan::T, "--ignored"]);
        args.extend(twins.iter().filter(|t| t.dir == dir && t.target == target).map(|t| t.test.as_str()));
        out.steps.push(Step::new(format!("{dir} light: {target}"), &args, ""));
    }
    Ok(out)
}

/// The lines a red light run prints, for the note on the PR: the full heavy step of each red light step,
/// as a command. Empty when no step failed.
pub fn rerun_note(failed: &[String]) -> Vec<String> {
    if failed.is_empty() {
        return Vec::new();
    }
    let mut out = vec![
        "== light lane RED - write a note on the PR that names these steps. Run the full heavy lane of each one locally, in release:".to_string(),
    ];
    for name in failed {
        let heavy = if name == BUILD { "heavy build".to_string() } else { name.replacen(" light: ", " heavy: ", 1) };
        out.push(format!("  {name}: cargo xtask gate release --only \"{heavy}\""));
    }
    out.push("  The full steps must pass before the next cut gate. A local run does not replace the cut gate.".to_string());
    out
}
