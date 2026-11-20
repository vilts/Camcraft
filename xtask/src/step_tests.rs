//! Pins the one-step-for-each-test-binary plans of the debug suites (#1941): no test target is dropped,
//! a step names one target, and `--only` / `--failed` can pick one such step.
//! Provenance: our own build law (AGENTS.md "Three gate tiers"); reads only this repository's test layout.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::plan::{self, ALL_CRATES, BATCH_CRATES, Plan, Step};
use crate::{scan, select};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_default()
}

/// The `-p <crate>` selectors (`--lib`, `--bins`, `--test x`, `--doc`) of the `<d> suites: ...` steps.
fn targets(p: &Plan, d: &str) -> BTreeSet<String> {
    let prefix = format!("{d} suites: ");
    p.steps
        .iter()
        .filter(|s| s.name.starts_with(&prefix))
        .map(|s| {
            let end = s.args.iter().position(|a| a == "--").unwrap_or(s.args.len());
            let sel = s.args[..end].iter().skip_while(|a| !a.starts_with("--lib") && *a != "--bins" && *a != "--test" && *a != "--doc");
            sel.cloned().collect::<Vec<_>>().join(" ")
        })
        .collect()
}

fn one_target(s: &Step) -> bool {
    let n = s.args.iter().filter(|a| matches!(a.as_str(), "--lib" | "--bins" | "--doc" | "--test")).count();
    n == 1
}

#[test]
fn release_runs_every_target_of_every_crate_one_step_each() {
    let root = root();
    let p = plan::release(&root);
    for d in ALL_CRATES {
        let mut want: BTreeSet<String> = ["--lib", "--bins", "--doc"].iter().map(|s| s.to_string()).collect();
        want.extend(scan::binaries(&root, d).into_iter().map(|b| format!("--test {b}")));
        assert_eq!(targets(&p, d), want, "release targets of {d}");
    }
    assert!(p.steps.iter().filter(|s| s.name.contains(" suites: ")).all(one_target));
}

#[test]
fn batch_runs_lib_and_each_non_slow_binary_one_step_each() {
    let root = root();
    let p = plan::batch(&root);
    for d in BATCH_CRATES.iter().chain(&["ui"]) {
        let mut want: BTreeSet<String> = BTreeSet::new();
        if *d == "ui" {
            want.extend(["--bins", "--doc"].iter().map(|s| s.to_string()));
        }
        want.insert("--lib".into());
        want.extend(
            scan::binaries(&root, d).into_iter().filter(|b| *d == "ui" || !plan::is_slow(d, b)).map(|b| format!("--test {b}")),
        );
        assert_eq!(targets(&p, d), want, "batch targets of {d}");
    }
    assert!(p.steps.iter().filter(|s| s.name.contains(" suites: ")).all(one_target));
    // The ui steps keep the crates/ui working directory; the prebuild steps come first.
    assert!(p.steps.iter().filter(|s| s.name.starts_with("ui suites: ")).all(|s| s.in_ui));
    assert_eq!(p.steps[0].name, "debug build: geometry (--no-run)");
}

#[test]
fn only_and_failed_select_one_binary_step() {
    let root = root();
    let p = plan::batch(&root);
    let one = select::apply(p.clone(), &["app suites: lib".into()], None).expect("only");
    assert_eq!(one.steps.len(), 1);
    assert_eq!(one.steps[0].name, "app suites: lib");
    let stems = vec!["app_suites_lib".to_string()];
    let again = select::apply(p, &[], Some(&stems)).expect("failed");
    assert_eq!(again.steps.len(), 1);
}

#[test]
fn batch_prebuilds_each_crate_without_mixing_feature_sets() {
    let root = root();
    let p = plan::batch(&root);
    let builds: Vec<&Step> = p.steps.iter().take_while(|s| crate::lanes::is_build(s)).collect();
    let want: Vec<String> = BATCH_CRATES.iter().chain(&["ui"]).map(|d| plan::pkg(d)).collect();
    assert_eq!(builds.len(), want.len(), "each batch crate gets its own prebuild step so features match its test steps: {builds:?}");
    for (s, pkg) in builds.iter().zip(&want) {
        let pkgs: Vec<&str> = s.args.windows(2).filter(|w| w[0] == "-p").map(|w| w[1].as_str()).collect();
        assert_eq!(pkgs, [pkg.as_str()], "{}: prebuild must not mix packages/features", s.name);
        assert_eq!(s.in_ui, pkg == "camcraft-ui", "{}", s.name);
    }
    assert!(p.steps[builds.len()..].iter().all(|s| !crate::lanes::is_build(s)), "all prebuilds must run before suites");
}

#[test]
fn batch_prebuild_leaves_suite_steps_fresh_with_truck_dev_dep() {
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
    }
    let fx = std::env::temp_dir().join(format!("xtask-batch-fresh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&fx);
    let _cleanup = Cleanup(fx.clone());
    let write = |rel: &str, body: &str| {
        let p = fx.join(rel);
        std::fs::create_dir_all(p.parent().expect("parent")).expect("dir");
        std::fs::write(p, body).expect("write");
    };
    let members: Vec<String> = ALL_CRATES.iter().map(|d| format!("\"crates/{d}\"")).collect();
    write("Cargo.toml", &format!("[workspace]\nmembers = [{}]\nresolver = \"3\"\n", members.join(", ")));
    write("crates/kernel/Cargo.toml", "[package]\nname = \"camcraft-kernel\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[features]\ndefault = []\ntruck = []\n[dev-dependencies]\ncamcraft-kernel = { path = \".\", features = [\"truck\"] }\n");
    write("crates/kernel/src/lib.rs", "pub fn k() -> bool { cfg!(feature = \"truck\") }\n");
    write("crates/geometry/Cargo.toml", "[package]\nname = \"camcraft-geometry\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\ncamcraft-kernel = { path = \"../kernel\" }\n");
    write("crates/geometry/src/lib.rs", "pub fn g() -> bool { camcraft_kernel::k() }\n");
    for d in ["cl", "tools", "post", "sim", "motion"] {
        write(&format!("crates/{d}/Cargo.toml"), &format!("[package]\nname = \"camcraft-{d}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"));
        write(&format!("crates/{d}/src/lib.rs"), "");
    }
    write("crates/app/Cargo.toml", "[package]\nname = \"camcraft-app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\ncamcraft-geometry = { path = \"../geometry\" }\n");
    write("crates/app/src/lib.rs", "pub fn a() -> bool { camcraft_geometry::g() }\n");
    write("crates/app/tests/smoke.rs", "#[test]\nfn smoke_ok() { assert!(!camcraft_app::a()); }\n");
    write("crates/ui/Cargo.toml", "[package]\nname = \"camcraft-ui\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\ncamcraft-app = { path = \"../app\" }\n");
    write("crates/ui/src/lib.rs", "");

    let p = plan::batch(&fx);
    for build in p.steps.iter().filter(|s| crate::lanes::is_build(s)) {
        let dir = if build.in_ui { fx.join("crates/ui") } else { fx.clone() };
        let st = std::process::Command::new(crate::exec::cargo())
            .args(&build.args)
            .current_dir(&dir)
            .env_remove("CARGO_TARGET_DIR")
            .stdout(std::process::Stdio::null()) // the JSON messages of the build (#1951)
            .status()
            .expect("run batch prebuild");
        assert!(st.success(), "prebuild {} failed", build.name);
    }
    let smoke = p.steps.iter().find(|s| s.name == "app suites: smoke").expect("app smoke step");
    let split = smoke.args.iter().position(|a| a == "--").unwrap_or(smoke.args.len());
    let mut args = smoke.args[..split].to_vec();
    args.push("--message-format=json".into());
    args.extend_from_slice(&smoke.args[split..]);
    let out = std::process::Command::new(crate::exec::cargo())
        .args(&args)
        .current_dir(&fx)
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("run app smoke");
    assert!(out.status.success(), "smoke step failed: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stale: Vec<&str> = stdout
        .lines()
        .filter(|l| l.contains("\"reason\":\"compiler-artifact\"") && l.contains("\"fresh\":false"))
        .collect();
    assert!(stale.is_empty(), "batch suite step recompiled after prebuild:\n{}", stale.join("\n"));
}
