//! Provenance: our own catalog refusal-case contract (#2856) over `camcraft-cli`.
//!
//! A case with `expect = "refusal"` in `case.toml` has no `<case>.nc`. Its
//! script saves the project and then runs the refused step without a catch,
//! so the run ends with an error. `refusal.txt` holds the refusal lines that
//! the script host recorded, byte for byte. Each line is a stderr line of the
//! run that starts with `camcraft-cli: `, without that prefix.

use std::fs;
use std::path::Path;
use std::process::Command;

use super::CatalogCase;

#[cfg(test)]
#[path = "catalog_refusal_tests.rs"]
mod tests;

pub const FILE: &str = "refusal.txt";
const PREFIX: &str = "camcraft-cli: ";

/// The refusal lines of one run, as `refusal.txt` holds them: each stderr
/// line that `camcraft-cli` prints for a recorded error, without its prefix.
/// The other stderr lines are the log, with times that change each run.
pub fn lines(stderr: &str) -> String {
    stderr
        .lines()
        .filter_map(|line| line.strip_prefix(PREFIX))
        .map(|line| format!("{line}\n"))
        .collect()
}

/// Run the case script in `work` and return its refusal lines. A run that
/// ends without an error, or that writes `<case>.nc`, is refused by name.
pub fn run(cli: &Path, work: &Path, case: &CatalogCase) -> Result<String, String> {
    let id = case.id();
    let out = Command::new(cli)
        .current_dir(work)
        .args(["script", "build.camscript"])
        .output()
        .map_err(|e| format!("{id}: failed to run camcraft-cli: {e}"))?;
    if out.status.success() {
        return Err(format!(
            "{id}: expect = \"refusal\", but build.camscript ended without an error"
        ));
    }
    let nc = work.join(format!("{}.nc", case.name));
    if nc.exists() {
        return Err(format!(
            "{id}: expect = \"refusal\", but build.camscript wrote {}",
            nc.display()
        ));
    }
    let project = work.join(format!("{}.camproj", case.name));
    if !project.is_file() {
        return Err(format!(
            "{id}: build.camscript must save {} before the refused step",
            project.display()
        ));
    }
    let got = lines(&String::from_utf8_lossy(&out.stderr));
    if got.trim().is_empty() {
        return Err(format!("{id}: build.camscript failed with no refusal lines"));
    }
    Ok(got)
}

/// Compare the refusal lines of a rebuilt run with the committed file.
pub fn check(case: &CatalogCase, got: &str) -> Result<(), String> {
    let path = case.dir.join(FILE);
    let want =
        fs::read_to_string(&path).map_err(|e| format!("{}: read {FILE}: {e}", case.id()))?;
    if got != want {
        return Err(format!(
            "{}: refusal lines differ from committed {}\nwant:\n{want}got:\n{got}(run `cargo xtask catalog --case {}`)",
            case.id(),
            path.display(),
            case.id()
        ));
    }
    Ok(())
}

/// Write the refusal lines of a rebuild into the case.
pub fn write(case: &CatalogCase, got: &str) -> Result<(), String> {
    let path = case.dir.join(FILE);
    fs::write(&path, got).map_err(|e| format!("{}: write {FILE}: {e}", case.id()))
}
