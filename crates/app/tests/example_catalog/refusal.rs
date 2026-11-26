// Provenance: original CamCraft catalog refusal-case assertions (#2856).
//
// A case with `expect = "refusal"` in `case.toml` has no `<case>.nc`. Its
// `refusal.txt` holds the refusal lines that the script host records (the
// run's `errors`), one line each, byte for byte. The CLI prints each one
// on stderr after the prefix `camcraft-cli: `; its log lines have no prefix.
use std::fs;
use std::path::Path;
use std::process::Command;

use camcraft_app::{project, script, world};

use super::support::CatalogCase;

const PREFIX: &str = "camcraft-cli: ";

/// `true` when the case's `case.toml` says `expect = "refusal"`.
pub(crate) fn expects_refusal(dir: &Path) -> bool {
    let Ok(text) = fs::read_to_string(dir.join("case.toml")) else {
        return false;
    };
    text.lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(key, _)| key.trim() == "expect")
        .any(|(_, value)| {
            let value = value.split('#').next().unwrap_or_default().trim();
            value.trim_matches(|c| c == '"' || c == '\'') == "refusal"
        })
}

/// The saved project opens without notes and keeps its model portable.
fn assert_opens(case: &CatalogCase, path: &Path, label: &str) {
    let w = world::new_world();
    let notes = project::load_noted(&w, path)
        .unwrap_or_else(|e| panic!("{label}: load {}: {e}", path.display()));
    assert!(notes.is_empty(), "{label}: unexpected load notes: {notes:?}");
    super::support::assert_portable_models(case, &w);
}

fn committed(case: &CatalogCase, label: &str) -> String {
    let path = case.dir.join("refusal.txt");
    // A Windows checkout with autocrlf hands the file back CRLF: the
    // generated lines end \n, so the committed file is read as LF.
    fs::read_to_string(&path)
        .map(|s| s.replace("\r\n", "\n"))
        .unwrap_or_else(|e| panic!("{label}: read {}: {e}", path.display()))
}

fn scratch(case: &CatalogCase, action: &str) -> std::path::PathBuf {
    std::env::temp_dir()
        .join(format!("camcraft-catalog-refusal-{action}-{}", std::process::id()))
        .join(&case.family)
        .join(&case.name)
}

/// Open the saved project in the script host, regenerate, and post. The
/// post must refuse with the committed lines and write no program.
pub(crate) fn regenerate(case: &CatalogCase) {
    let label = format!("{}/{}", case.family, case.name);
    let project = case.dir.join(format!("{}.camproj", case.name));
    assert_opens(case, &project, &label);
    let work = scratch(case, "regenerate");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap_or_else(|e| panic!("{label}: mkdir {}: {e}", work.display()));
    let nc = work.join(format!("{}.nc", case.name));
    let source = format!(
        "cam.load({});\ncam.generate();\ncam.post({{ to: {} }});\n",
        serde_json::json!(project.display().to_string()),
        serde_json::json!(nc.display().to_string()),
    );
    let outcome = script::run(&source);
    assert!(!outcome.ok, "{label}: expect = \"refusal\", but the post was not refused");
    assert!(!nc.exists(), "{label}: expect = \"refusal\", but the post wrote {}", nc.display());
    let got: String = outcome.errors.iter().map(|line| format!("{line}\n")).collect();
    assert_eq!(got, committed(case, &label), "{label}: refusal lines differ from refusal.txt");
    let _ = fs::remove_dir_all(&work);
}

/// Run `build.camscript` in a copy of the case. The run must end with an
/// error after it saved the project, write no program, and print the
/// committed refusal lines.
pub(crate) fn rebuild(case: &CatalogCase) {
    let label = format!("{}/{}", case.family, case.name);
    let work = scratch(case, "rebuild");
    let _ = fs::remove_dir_all(&work);
    super::support::copy_dir_all(&case.dir, &work);
    let project = work.join(format!("{}.camproj", case.name));
    let nc = work.join(format!("{}.nc", case.name));
    let _ = fs::remove_file(&project);
    let output = Command::new(env!("CARGO_BIN_EXE_camcraft-cli"))
        .args(["script", "build.camscript"])
        .current_dir(&work)
        .output()
        .unwrap_or_else(|e| panic!("{label}: run camcraft-cli: {e}"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "{label}: expect = \"refusal\", but build.camscript ended without an error\nstderr: {stderr}"
    );
    assert!(!nc.exists(), "{label}: expect = \"refusal\", but build.camscript wrote {}", nc.display());
    assert!(project.is_file(), "{label}: build.camscript must save {} before the refused step", project.display());
    assert_opens(case, &project, &label);
    // The other stderr lines are the log, with times that change each run.
    let got: String = stderr
        .lines()
        .filter_map(|line| line.strip_prefix(PREFIX))
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(got, committed(case, &label), "{label}: refusal lines differ from refusal.txt");
    let _ = fs::remove_dir_all(&work);
}
