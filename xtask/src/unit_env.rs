//! The caller environment that enters cargo's unit hash, pinned for every cargo call of a gate (#3150),
//! and the count of unit directories that a gate run adds.
//! Provenance: our own build tooling (#900, #3150); the Cargo book, "Environment variables" (CARGO_INCREMENTAL,
//! CARGO_PROFILE_<name>_<key>) and "Build cache" (a unit's hash covers its profile and its rustflags).
//!
//! Why: cargo names each unit directory (`target/<profile>/build/<package>/<hash>/`) by a hash of its
//! profile, among other inputs. `CARGO_INCREMENTAL` and the `CARGO_PROFILE_*` variables change the
//! profile, so each agent that runs a gate with other values gives every workspace unit a new
//! directory. Cargo never removes the old one: one `camcraft-app` variant is about 600 units of ~190 MB.

use std::fs;
use std::path::Path;
use std::process::Command;

/// The variables that each gate cargo call gets, whatever the caller set. Incremental is off: sccache
/// cannot cache an incremental unit, and incremental data adds to the size of each unit.
pub const PINNED: [(&str, &str); 1] = [("CARGO_INCREMENTAL", "0")];

/// True for a caller variable that the gate removes: it changes the profile, and with it the unit hash.
pub fn is_removed(name: &str) -> bool {
    name.starts_with("CARGO_PROFILE_") || name == "CARGO_BUILD_INCREMENTAL"
}

/// Caller variables that the gate keeps but reports: they also enter the unit hash.
pub const NOTED: [&str; 3] = ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS"];

/// The variables of `vars` that the gate removes from its cargo calls, sorted.
pub fn removed(vars: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = vars.into_iter().filter(|k| is_removed(k)).collect();
    out.sort();
    out
}

/// Sets the pinned variables on `cmd` and removes the profile variables of the caller.
pub fn apply(cmd: &mut Command) {
    for name in removed(std::env::vars_os().filter_map(|(k, _)| k.into_string().ok())) {
        cmd.env_remove(name);
    }
    cmd.envs(PINNED);
}

/// The lines that a gate prints at its start: the pin, and each caller variable that it removes or notes.
pub fn notes(vars: &[(String, String)]) -> Vec<String> {
    let mut out = vec![format!(
        "unit env: {} for every cargo step (#3150: one fixed profile, so a second run reuses the units)",
        PINNED.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" ")
    )];
    let gone = removed(vars.iter().map(|(k, _)| k.clone()));
    if !gone.is_empty() {
        out.push(format!("unit env: removed {} (it changes the profile and gives each unit a new directory)", gone.join(", ")));
    }
    for (k, _) in vars.iter().filter(|(k, v)| NOTED.contains(&k.as_str()) && !v.is_empty()) {
        out.push(format!("unit env: note: {k} is set; each other value gives each unit a new directory"));
    }
    out
}

/// The unit directories (`build/<package>/<hash>/` with a `fingerprint/`) under `target/debug` and
/// `target/release` of `root`; a missing directory counts 0.
pub fn units(root: &Path) -> usize {
    ["debug", "release"]
        .iter()
        .map(|p| root.join("target").join(p).join("build"))
        .flat_map(|b| fs::read_dir(b).into_iter().flatten())
        .filter_map(Result::ok)
        .flat_map(|package| fs::read_dir(package.path()).into_iter().flatten())
        .filter_map(Result::ok)
        .filter(|unit| unit.path().join("fingerprint").is_dir())
        .count()
}

/// The summary line of the unit count: how many unit directories the run added.
pub fn added_line(before: usize, after: usize) -> String {
    let added = after.saturating_sub(before);
    if added == 0 {
        format!("  unit directories: {after}, none added (the run reused every unit)")
    } else {
        format!("  unit directories: {after}, {added} added (a second run on the same tree must add none, #3150)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_variables_are_removed_and_others_kept() {
        let vars = ["CARGO_PROFILE_DEV_DEBUG", "PATH", "CARGO_BUILD_INCREMENTAL", "CARGO_PROFILE_TEST_OPT_LEVEL", "RUSTFLAGS", "CARGO_BUILD_JOBS"];
        assert_eq!(
            removed(vars.iter().map(|s| s.to_string())),
            ["CARGO_BUILD_INCREMENTAL", "CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_OPT_LEVEL"]
        );
        assert!(!is_removed("CARGO_INCREMENTAL"), "the gate sets CARGO_INCREMENTAL itself");
    }

    #[test]
    fn every_gate_cargo_call_gets_the_same_unit_env() {
        // Two callers with different profile env must give the same command env (#3150).
        let env_of = |cmd: &Command| {
            let mut e: Vec<(String, Option<String>)> = cmd
                .get_envs()
                .map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned())))
                .filter(|(k, _)| k == "CARGO_INCREMENTAL" || is_removed(k))
                .collect();
            e.sort();
            e
        };
        let mut a = Command::new("cargo");
        apply(&mut a);
        let mut b = Command::new("cargo");
        b.env("CARGO_INCREMENTAL", "1");
        apply(&mut b);
        assert_eq!(env_of(&a), env_of(&b));
        assert!(env_of(&a).contains(&("CARGO_INCREMENTAL".into(), Some("0".into()))));
    }

    #[test]
    fn direct_cargo_steps_apply_the_unit_env() {
        // The one place where a gate step starts cargo applies the pin (#3150).
        let src = include_str!("direct.rs");
        let body = src.split("pub fn cargo(").nth(1).unwrap_or_default();
        let body = body.split("\n}\n").next().unwrap_or_default();
        assert!(body.contains("crate::unit_env::apply(&mut cmd)"), "direct::cargo must apply the unit env");
    }

    #[test]
    fn batch_steps_share_the_flags_that_enter_the_unit_hash() {
        // A step with its own profile, features or config would build each crate again under a new hash.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let plan = crate::plan::batch(&root);
        assert!(plan.steps.len() > 10, "the batch plan has its steps");
        let hashed = ["--config", "--features", "--all-features", "--no-default-features", "--profile", "--release", "-Z"];
        for s in &plan.steps {
            let bad: Vec<&String> = s.args.iter().filter(|a| hashed.iter().any(|h| a == h || a.starts_with(&format!("{h}=")))).collect();
            assert!(bad.is_empty(), "step {} sets {bad:?}: it changes the unit hash", s.name);
        }
    }

    #[test]
    fn notes_name_the_pin_and_each_removed_or_noted_variable() {
        let vars = vec![
            ("CARGO_PROFILE_DEV_DEBUG".to_string(), "true".to_string()),
            ("RUSTFLAGS".to_string(), "-Ctarget-cpu=native".to_string()),
            ("CARGO_ENCODED_RUSTFLAGS".to_string(), String::new()),
        ];
        let n = notes(&vars);
        assert!(n[0].contains("CARGO_INCREMENTAL=0"));
        assert!(n.iter().any(|l| l.contains("removed CARGO_PROFILE_DEV_DEBUG")));
        assert!(n.iter().any(|l| l.contains("note: RUSTFLAGS")));
        assert!(!n.iter().any(|l| l.contains("CARGO_ENCODED_RUSTFLAGS")), "an empty value is no flag");
        assert_eq!(notes(&[]).len(), 1);
    }

    #[test]
    fn units_count_nested_unit_directories_only() {
        let root = std::env::temp_dir().join(format!("xtask-unit-env-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (profile, unit) in [("debug", "camcraft-app/aaaa"), ("debug", "camcraft-app/bbbb"), ("release", "camcraft-cl/cccc")] {
            let d = root.join("target").join(profile).join("build").join(unit).join("fingerprint");
            assert!(fs::create_dir_all(&d).is_ok());
        }
        // An old-layout build-script directory has no fingerprint/ under its children: not a unit.
        assert!(fs::create_dir_all(root.join("target/debug/build/ahash-1234/out")).is_ok());
        assert_eq!(units(&root), 3);
        assert_eq!(units(&root.join("missing")), 0);
        let _ = fs::remove_dir_all(&root);
        assert!(added_line(3, 3).contains("none added"));
        assert!(added_line(3, 603).contains("600 added"));
    }
}
