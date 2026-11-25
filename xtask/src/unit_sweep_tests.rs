//! Tests of the end-of-run sweep of superseded test-harness units (#3169).
//! Provenance: our own build tooling (#3169).

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::unit_sweep::{after_run_with, first_live, judge, lines, norm, opted_out, plain_dir, scan, superseded, sweep, target_forms, Report, OPT_OUT};

fn tmp(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("xtask-unit-sweep-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    root
}

/// One unit `target/<profile>/build/<pkg>/<hash>/` with the target JSON `json` written at `at`
/// and a 1000-byte output file.
fn unit(root: &Path, profile: &str, pkg: &str, hash: &str, json: &str, at: SystemTime) -> PathBuf {
    let dir = root.join("target").join(profile).join("build").join(pkg).join(hash);
    assert!(fs::create_dir_all(dir.join("fingerprint")).is_ok());
    assert!(fs::create_dir_all(dir.join("out")).is_ok());
    assert!(fs::write(dir.join("out").join("t.exe"), vec![0u8; 1000]).is_ok());
    assert!(fs::write(dir.join("fingerprint").join(format!("dep-{json}")), b"x").is_ok());
    let f = dir.join("fingerprint").join(format!("{json}.json"));
    assert!(fs::write(&f, b"{}").is_ok());
    let file = File::options().write(true).open(&f).expect("open the fingerprint");
    assert!(file.set_modified(at).is_ok());
    dir
}

fn none() -> Option<String> {
    None
}

const T: &str = "test-integration-test-ops";

#[test]
fn an_older_variant_of_a_target_built_in_this_run_is_removed_and_the_newest_survives() {
    let root = tmp("newest");
    let start = SystemTime::now();
    let old = start - Duration::from_secs(3600);
    let new = start + Duration::from_secs(5);
    let a_old = unit(&root, "debug", "camcraft-app", "aaaa", T, old);
    let a_older = unit(&root, "debug", "camcraft-app", "aaab", T, old - Duration::from_secs(60));
    let a_new = unit(&root, "debug", "camcraft-app", "bbbb", T, new);
    let b_old = unit(&root, "debug", "camcraft-app", "cccc", "test-integration-test-cmds", old);
    let b_new = unit(&root, "debug", "camcraft-app", "dddd", "test-integration-test-cmds", new);
    let r = sweep(&root, start, none);
    assert_eq!(r.skipped, None);
    assert_eq!(r.removed, 3, "{r:?}");
    assert!(r.bytes >= 3000, "the report counts the bytes removed: {r:?}");
    assert!(!a_old.exists() && !a_older.exists() && !b_old.exists(), "the superseded variants are removed");
    assert!(a_new.exists() && b_new.exists(), "the newest variant of each target survives");
    assert!(lines(&r)[0].contains("removed 3 superseded test unit dir(s)"), "{:?}", lines(&r));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn two_runs_on_two_trees_leave_one_unit_for_each_target() {
    let root = tmp("bounded");
    let targets = ["test-integration-test-a", "test-integration-test-b", "test-lib-camcraft_app", "test-bin-camcraft-cli"];
    let run1 = SystemTime::now() - Duration::from_secs(7200);
    for (i, t) in targets.iter().enumerate() {
        unit(&root, "debug", "camcraft-app", &format!("1{i}"), t, run1 + Duration::from_secs(1));
    }
    assert!(sweep(&root, run1, none).removed == 0, "the first run has one variant of each target");
    let run2 = SystemTime::now();
    for (i, t) in targets.iter().enumerate() {
        unit(&root, "debug", "camcraft-app", &format!("2{i}"), t, run2 + Duration::from_secs(1));
    }
    assert_eq!(scan(&root.join("target")).len(), 8, "the second tree gave each target a new unit");
    let r = sweep(&root, run2, none);
    assert_eq!(r.removed, 4, "{r:?}");
    assert_eq!(scan(&root.join("target")).len(), targets.len(), "one unit for each target is left");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn no_variant_written_in_this_run_means_nothing_is_removed() {
    let root = tmp("untouched");
    let start = SystemTime::now();
    let a = unit(&root, "debug", "camcraft-motion", "aaaa", T, start - Duration::from_secs(10));
    let b = unit(&root, "debug", "camcraft-motion", "bbbb", T, start - Duration::from_secs(20));
    assert_eq!(sweep(&root, start, none).removed, 0);
    assert!(a.exists() && b.exists(), "a target that this run did not build keeps its variants");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_non_test_unit_is_never_removed() {
    let root = tmp("nontest");
    let start = SystemTime::now();
    let old = start - Duration::from_secs(3600);
    let new = start + Duration::from_secs(5);
    let mut kept = Vec::new();
    for (i, json) in ["lib-camcraft_app", "bin-camcraft-cli", "build-script-build-script-build", "run-build-script-build-script-build"].iter().enumerate() {
        kept.push(unit(&root, "debug", "camcraft-app", &format!("o{i}"), json, old));
        kept.push(unit(&root, "debug", "camcraft-app", &format!("n{i}"), json, new));
    }
    assert!(scan(&root.join("target")).is_empty(), "no lib, bin or build-script unit is a harness unit");
    assert_eq!(sweep(&root, start, none).removed, 0);
    assert!(kept.iter().all(|d| d.exists()));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_held_cargo_lock_or_a_live_process_removes_nothing() {
    let root = tmp("locked");
    let start = SystemTime::now();
    let old = unit(&root, "debug", "camcraft-app", "aaaa", T, start - Duration::from_secs(3600));
    unit(&root, "debug", "camcraft-app", "bbbb", T, start + Duration::from_secs(5));
    let lock = File::create(root.join("target/debug/.cargo-lock")).expect("lock file");
    assert!(lock.try_lock().is_ok(), "the test holds cargo's lock");
    let r = sweep(&root, start, none);
    assert!(r.skipped.as_deref().is_some_and(|w| w.contains("lock")), "{r:?}");
    assert!(old.exists(), "a held lock removes nothing");
    assert!(lines(&r)[0].contains("not run"));
    drop(lock);
    let r = sweep(&root, start, || Some("pid 42".into()));
    assert!(r.skipped.as_deref().is_some_and(|w| w.contains("pid 42")), "{r:?}");
    assert!(old.exists(), "a live process of this tree removes nothing");
    assert_eq!(sweep(&root, start, none).removed, 1, "with no lock and no process the sweep runs");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn superseded_never_lists_the_newest_variant() {
    let root = tmp("pure");
    let start = SystemTime::now();
    unit(&root, "release", "camcraft-sim", "aaaa", T, start + Duration::from_secs(1));
    unit(&root, "release", "camcraft-sim", "bbbb", T, start + Duration::from_secs(2));
    unit(&root, "debug", "camcraft-sim", "cccc", T, start - Duration::from_secs(9));
    let units = scan(&root.join("target"));
    assert_eq!(units.len(), 3);
    assert!(superseded(&units, start).is_empty(), "both release variants are from this run; debug is another profile");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_process_check_finds_another_process_of_this_tree_only() {
    let target = if cfg!(windows) { r"C:\W\Tree\target" } else { "/w/tree/target" };
    let needles = [norm(target)];
    let other = if cfg!(windows) { r"c:\w\tree\target\debug\xtask.exe gate quick" } else { "/w/tree/target/debug/xtask gate quick" };
    let listing = format!("  1 /usr/bin/cargo xtask gate quick\n 77 {other}\n 99 {other}\n");
    assert_eq!(first_live(&listing, &needles, 77), Some("pid 99".into()), "own pid is skipped");
    assert_eq!(first_live(&listing, &needles, 99), Some("pid 77".into()));
    assert_eq!(first_live(" 1 cargo build\n", &needles, 5), None);
    let verbatim = if cfg!(windows) { r"\\?\C:\W\Tree\target\" } else { "/w/tree/target/" };
    assert_eq!(norm(verbatim), norm(target), "a verbatim prefix and a trailing separator are one form");
}

#[test]
fn the_process_check_knows_every_form_of_the_root() {
    let base = tmp("forms");
    let real = base.join("tree");
    assert!(fs::create_dir_all(real.join("sub")).is_ok());
    let dotted = real.join("sub").join("..");
    let forms = target_forms(&dotted).expect("the forms resolve");
    let clean = norm(&real.join("target").to_string_lossy());
    let canon = norm(&fs::canonicalize(&real).expect("canonical").join("target").to_string_lossy());
    assert!(forms.contains(&clean), "the clean form is a needle: {forms:?}");
    assert!(forms.contains(&canon), "the canonical form is a needle: {forms:?}");
    assert!(forms.contains(&norm(&dotted.join("target").to_string_lossy())), "the raw form is a needle");
    let listing = format!("5 {} --test x", real.join("target").join("debug").join("x").display());
    assert_eq!(judge(Some(&listing), Ok(forms.clone()), 1), Some("pid 5".into()));
    assert!(target_forms(&base.join("missing")).is_err(), "a root that cannot be resolved is an error");
    let _ = fs::remove_dir_all(&base);
}

#[test]
#[allow(clippy::permissions_set_readonly_false)] // the test undoes its own read-only flag
fn when_unsure_the_sweep_removes_nothing() {
    let root = tmp("unsure");
    let start = SystemTime::now();
    let old = unit(&root, "debug", "camcraft-app", "aaaa", T, start - Duration::from_secs(3600));
    unit(&root, "debug", "camcraft-app", "bbbb", T, start + Duration::from_secs(5));
    // An unreadable process list, and a root whose forms cannot be resolved, count as live.
    assert!(judge(None, Ok(vec!["x".into()]), 1).is_some_and(|w| w.contains("could not be read")));
    assert!(judge(Some(""), Err("no".into()), 1).is_some_and(|w| w.contains("cannot be resolved")));
    let r = sweep(&root, start, || judge(None, Ok(vec![]), 1));
    assert!(r.skipped.is_some() && old.exists(), "an unreadable process list removes nothing: {r:?}");
    // A lock that cannot be probed (here a read-only lock file: it cannot be opened for writing).
    let lock = root.join("target/debug/.cargo-lock");
    assert!(fs::write(&lock, b"").is_ok());
    let mut perm = fs::metadata(&lock).expect("lock metadata").permissions();
    perm.set_readonly(true);
    assert!(fs::set_permissions(&lock, perm.clone()).is_ok());
    if File::options().write(true).open(&lock).is_err() {
        // (a root user on Unix can still open it; then this case cannot be made here)
        let r = sweep(&root, start, none);
        assert!(r.skipped.as_deref().is_some_and(|w| w.contains("cannot be probed")) && old.exists(), "{r:?}");
    }
    perm.set_readonly(false);
    assert!(fs::set_permissions(&lock, perm).is_ok());
    assert!(fs::remove_file(&lock).is_ok());
    // The env turns it off, or moves the target directory.
    let elapsed = SystemTime::now().duration_since(start).unwrap_or_default();
    let live = || None;
    for (k, v) in [(OPT_OUT, "off"), (OPT_OUT, "0"), ("CARGO_TARGET_DIR", "x"), ("CARGO_BUILD_TARGET_DIR", "x"), ("CARGO_BUILD_BUILD_DIR", "x")] {
        let env = |name: &str| (name == k).then(|| v.to_string());
        let r = after_run_with(&root, elapsed, &env, &live);
        assert!(r.skipped.as_deref().is_some_and(|w| w.contains(k)) && old.exists(), "{k}={v}: {r:?}");
    }
    // A start time that cannot be computed is the epoch: every unit is from "this run".
    let r = after_run_with(&root, Duration::MAX, &|_| None, &live);
    assert!(r.skipped.is_none() && r.removed == 0 && old.exists(), "{r:?}");
    // With no doubt left, the same tree is swept.
    let r = after_run_with(&root, elapsed + Duration::from_secs(1), &|_| None, &live);
    assert_eq!(r.removed, 1, "{r:?}");
    let _ = fs::remove_dir_all(&root);
}

/// A directory link at `link` to `to`: a symlink on Unix, a junction on Windows (no privilege needed).
fn dir_link(to: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(to, link).is_ok()
    }
    #[cfg(windows)]
    {
        let win = |p: &Path| p.to_string_lossy().replace('/', "\\");
        std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(win(link))
            .arg(win(to))
            .output()
            .is_ok_and(|o| o.status.success())
    }
}

#[test]
fn the_sweep_never_reads_or_deletes_through_a_link() {
    let base = tmp("links");
    let root = base.join("tree");
    let outside = base.join("outside");
    let start = SystemTime::now();
    // A real newer variant, and an old variant of the same target that is a link to a unit outside target/:
    // if the sweep followed the link, it would remove the far unit.
    unit(&root, "debug", "camcraft-app", "bbbb", T, start + Duration::from_secs(5));
    let far = unit(&outside, "debug", "camcraft-app", "aaaa", T, start - Duration::from_secs(3600));
    let linked = root.join("target").join("debug").join("build").join("camcraft-app").join("aaaa");
    assert!(dir_link(&far, &linked), "the test makes a directory link");
    assert!(!plain_dir(&linked), "a link is not a plain directory");
    assert!(scan(&root.join("target")).iter().all(|u| !u.dir.starts_with(&linked)), "the scan does not enter a link");
    let r = sweep(&root, start, none);
    assert_eq!(r.removed, 0, "{r:?}");
    assert!(far.exists(), "nothing outside target/ is removed");
    // A linked profile directory stops the whole sweep.
    let root2 = base.join("tree2");
    assert!(fs::create_dir_all(root2.join("target")).is_ok());
    assert!(dir_link(&root.join("target/debug"), &root2.join("target/debug")));
    let r = sweep(&root2, start, none);
    assert!(r.skipped.as_deref().is_some_and(|w| w.contains("link")), "{r:?}");
    for l in [linked, root2.join("target/debug")] {
        let _ = fs::remove_file(&l).or_else(|_| fs::remove_dir(&l)); // the link only, never its target
    }
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn a_link_at_the_package_level_is_skipped() {
    let base = tmp("pkglink");
    let root = base.join("tree");
    let start = SystemTime::now();
    // A real package with one old unit, and a second package that is a link to it: the scan must not
    // enter the link, the sweep must remove nothing through it, and the link itself must stay (#3173).
    let real_pkg = unit(&root, "debug", "camcraft-app", "aaaa", T, start - Duration::from_secs(3600));
    unit(&root, "debug", "camcraft-app", "bbbb", T, start + Duration::from_secs(5));
    let build = root.join("target").join("debug").join("build");
    // A link name distinct from the real package names: on a case-insensitive
    // volume (Windows, macOS) a case-variant name folds to the same directory
    // and cannot be created next to it (#3173). Every platform links a second
    // package name, `other-app`.
    let linked_pkg = build.join("other-app");
    assert!(dir_link(real_pkg.parent().expect("the package dir"), &linked_pkg), "the test makes the package link");
    assert!(!plain_dir(&linked_pkg), "a package link is not a plain directory");
    let units = scan(&root.join("target"));
    assert!(units.iter().all(|u| !u.dir.starts_with(&linked_pkg)), "the scan does not enter a package link: {units:?}");
    let r = sweep(&root, start, none);
    assert!(r.skipped.is_none(), "{r:?}");
    assert_eq!(r.removed, 1, "only the real old variant is removed: {r:?}");
    assert!(fs::read_dir(&build).is_ok_and(|d| d.count() == 2), "the package link itself stays");
    let _ = fs::remove_file(&linked_pkg).or_else(|_| fs::remove_dir(&linked_pkg)); // the link only
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn a_path_that_differs_only_by_case_is_one_form_on_a_case_insensitive_volume() {
    let a = norm("/w/tree/target");
    let b = norm("/W/Tree/target");
    // Windows and the default macOS volume fold case; a case-sensitive Unix volume does not.
    if cfg!(windows) || cfg!(target_os = "macos") {
        assert_eq!(a, b, "case folding (#3173)");
    } else {
        assert_ne!(a, b);
    }
}

#[test]
fn the_sweep_can_be_turned_off_and_reports_failed_deletes_as_warnings() {
    assert!(opted_out(Some("off")) && opted_out(Some("OFF")) && opted_out(Some("0")));
    assert!(!opted_out(None) && !opted_out(Some("on")));
    let r = Report { removed: 1, bytes: 2_500_000_000, failed: vec!["x: denied".into()], skipped: None };
    let l = lines(&r);
    assert!(l[0].contains("2.5 GB"), "{l:?}");
    assert!(l[1].contains("warning: could not remove x: denied"), "{l:?}");
}

#[test]
fn the_gate_summary_runs_the_sweep() {
    let src = include_str!("exec.rs");
    let body = src.split("pub fn summary(").nth(1).unwrap_or_default();
    assert!(body.contains("crate::unit_sweep::after_run(&self.root"), "the summary must run the sweep (#3169)");
}
