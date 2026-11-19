//! Pins the known-fail skip of the release heavy lanes (the #1950 known-fail decision, RECOMMENDED.md).
//! Provenance: our own build law (AGENTS.md "Three gate tiers"); libtest's `--exact` / `--skip` flags.

use std::path::Path;

use crate::known_fail::{self, KnownFail};
use crate::{exec, plan, scan};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn entries() -> Vec<KnownFail> {
    crate::known_fail_scan::discover(&root()).expect("valid source markers")
}

#[test]
fn source_marker_adds_a_known_fail_without_a_skip_list() {
    let root = std::env::temp_dir().join(format!("known-fail-marker-{}", std::process::id()));
    let source = root.join("crates/motion/src/lib.rs");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "#[test]\n#[ignore = \"known-fail #123 | lib | example | expected failure\"]\nfn example() {\n// known-fail-assert\nassert!(false, \"expected failure\");\n}\n").unwrap();
    let p = plan::release(&root);
    let valid = std::fs::read_to_string(&source).unwrap();
    let entries = crate::known_fail_scan::discover(&root).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].issue, 123);
    std::fs::write(&source, valid.replace(" | lib | ", " | missing | ")).unwrap();
    assert!(known_fail::check_issues(&root, true).unwrap_err().contains("unknown target"));
    std::fs::write(&source, &valid).unwrap();
    std::fs::write(source.with_file_name("duplicate.rs"), &valid).unwrap();
    assert!(known_fail::check_issues(&root, true).unwrap_err().contains("duplicate known-fail marker"));
    std::fs::remove_dir_all(&root).unwrap();
    assert!(p.notes.iter().any(|n| n.starts_with("SKIPPED (known fail, #123):") && n.contains("example")), "{:?}", p.notes);
    assert!(p.steps.iter().any(|s| s.name.contains("known fail #123")));
}

#[test]
fn malformed_known_fail_markers_are_refused() {
    let valid = "#[test]\n#[ignore = \"known-fail #123 | lib | example | expected failure\"]\nfn example() {\n// known-fail-assert\nassert!(false, \"expected failure\");\n}\n";
    let parse = |s: &str| crate::known_fail_scan::parse(s, "motion", "crates/motion/src/lib.rs");
    let entries = parse(valid).unwrap();
    assert_eq!(entries[0].line, 5);
    for bad in [
        valid.replace("#123", "#0"), valid.replace("#123", "#x"),
        valid.replace(" | expected failure", " | "),
        valid.replace(" | example | ", " | wrong | "),
        valid.replace(" | lib | ", " | "),
        valid.replace("known-fail #123", "known-fail 123"),
        valid.replace("#[test]\n", ""),
        valid.replace("// known-fail-assert\n", ""),
        valid.replace("// known-fail-assert", "// known-fail-assert\n// known-fail-assert"),
        valid.replace("assert!(false,", "other!(false,"),
    ] {
        assert!(parse(&bad).is_err(), "{bad}");
    }
}

#[test]
fn closed_known_fail_issue_refuses_the_gate() {
    let entries = crate::known_fail_scan::parse(
        "#[test]\n#[ignore = \"known-fail #123 | lib | example | expected failure\"]\nfn example() {\n// known-fail-assert\nassert!(false, \"expected failure\");\n}\n",
        "motion", "crates/motion/src/lib.rs",
    ).unwrap();
    let err = known_fail::check_states(&entries, false, |_| Some("CLOSED".into())).unwrap_err();
    assert!(err.contains("#123 is closed"), "{err}");
    assert!(known_fail::check_states(&entries, false, |_| Some("OPEN".into())).is_ok());
    let dry = known_fail::check_states(&entries, true, |_| panic!("dry run must not read GitHub")).unwrap();
    assert_eq!(dry, ["known fail: #123 state not read (dry run)"]);
}

#[test]
fn each_known_fail_is_skipped_in_its_lane_and_checked_alone() {
    assert!(entries().len() >= 3, "the three #1950 failures must use source markers");
    let steps = plan::heavy_steps(&root());
    for k in &entries() {
        let lane = steps.iter().find(|s| s.name == format!("{} heavy: {}", k.dir, k.target)).expect("lane");
        assert!(lane.args.windows(2).any(|w| w[0] == "--skip" && w[1] == k.test), "{}: {:?}", k.test, lane.args);
        assert!(lane.args.contains(&"--exact".to_string()) && lane.expect_fail.is_none());
        let check: Vec<_> = steps.iter().filter(|s| s.name == known_fail::step_name(k)).collect();
        assert_eq!(check.len(), 1, "{}", k.test);
        assert_eq!(check[0].expect_fail, Some(k.clone()));
        assert!(check[0].args.ends_with(&["--".into(), plan::T.into(), "--exact".into(), k.test.clone(), "--ignored".into()]));
        assert!(!check[0].args.iter().any(|a| a == "--skip"));
    }
    // Only the known-fail checks expect a failure, and only the lanes that hold one skip anything.
    assert_eq!(steps.iter().filter(|s| s.expect_fail.is_some()).count(), entries().len());
    let skipping = steps.iter().filter(|s| s.args.iter().any(|a| a == "--skip")).count();
    let lanes: std::collections::BTreeSet<_> = entries().into_iter().map(|k| (k.dir.clone(), k.target.clone())).collect();
    assert_eq!(skipping, lanes.len());
}

#[test]
fn each_known_fail_is_an_ignored_test_that_names_its_issue() {
    for k in &entries() {
        let name = k.test.rsplit("::").next().unwrap_or(&k.test);
        let dir = root().join("crates").join(&k.dir);
        let files = if k.target == "lib" {
            scan::rs_files_recursive(&dir.join("src"))
        } else {
            vec![dir.join("tests").join(format!("{}.rs", k.target))]
        };
        let hit = files.iter().map(|f| scan::read(f)).find_map(|text| {
            let (before, _) = text.split_once(&format!("fn {name}()"))?;
            Some(before.rsplit("#[test]").next().unwrap_or_default().to_string())
        });
        let attrs = hit.unwrap_or_else(|| panic!("{} not found in {}/{}", k.test, k.dir, k.target));
        assert!(attrs.contains("#[ignore = \""), "{} must stay #[ignore]d", k.test);
        assert!(attrs.contains(&format!("#{}", k.issue)), "{}: ignore reason must name #{}", k.test, k.issue);
    }
}

#[test]
fn a_known_fail_step_is_ok_only_when_its_test_fails() {
    let k = KnownFail { dir: "motion".into(), target: "lib".into(), test: "a::x".into(), issue: 1, file: "crates/m/src/a.rs".into(), line: 92, reason: "hops ".into() };
    // The red run's panic block (Windows slashes), as the release log holds it.
    let right = "thread 'a::x' (7) panicked at crates\\m\\src\\a.rs:92:5:\nhops 10 over 8\n\nfailures:\n";
    let mut red = "failed: unittests src/lib.rs [tests: a::w, a::x]".to_string();
    assert!(exec::expect_fail_note(101, &mut red, right, &k), "{red}");
    assert!(red.starts_with("known fail, red as expected"));
    let mut green = String::new();
    assert!(!exec::expect_fail_note(0, &mut green, "", &k));
    assert!(green.contains("NOW PASSES"));
    let mut broken = "compile error".to_string();
    assert!(!exec::expect_fail_note(101, &mut broken, right, &k));
    // A crash of the test binary (no `test ... FAILED` line) is not the expected failure.
    for code in [101, -1] {
        let mut crash = format!("exit {code}");
        assert!(!exec::expect_fail_note(code, &mut crash, right, &k), "{crash}");
    }
    let mut no_list = "failed: unittests src/lib.rs".to_string();
    assert!(!exec::expect_fail_note(101, &mut no_list, right, &k));
    // Another test failed, or a name that only contains the expected one.
    let mut other = "failed: unittests src/lib.rs [tests: a::y]".to_string();
    assert!(!exec::expect_fail_note(101, &mut other, right, &k));
    let mut longer = "failed: unittests src/lib.rs [tests: a::x2]".to_string();
    assert!(!exec::expect_fail_note(101, &mut longer, right, &k));
    // The right test, failed for another reason: another assert, or a panic in other code.
    let named = "failed: unittests src/lib.rs [tests: a::x]";
    let other_assert = "thread 'a::x' (7) panicked at crates/m/src/a.rs:88:5:\nrest 0.3000\n";
    let other_file = "thread 'a::x' (7) panicked at crates/m/src/planner.rs:12:9:\nindex out of bounds: hops 3\n";
    let other_thread = "thread 'a::y' (8) panicked at crates/m/src/a.rs:92:5:\nhops 10 over 8\n";
    // The refusal assert in the same file holds "hops " inside its message, not at a line start.
    let refusal = "thread 'a::x' (7) panicked at crates/m/src/a.rs:86:5:\nSome(\"would reach 9 passes a level by retract hops (over 8)\")\n";
    let same_reason_other_line = "thread 'a::x' (7) panicked at crates/m/src/a.rs:88:5:\nhops 10 over 8\n";
    for log in [same_reason_other_line, other_assert, other_file, other_thread, refusal, ""] {
        let mut note = named.to_string();
        assert!(!exec::expect_fail_note(101, &mut note, log, &k), "{log}");
        assert!(note.starts_with("KNOWN FAIL, DIFFERENT REASON"), "{note}");
    }
    // The reason must sit in this test's own block, not in a later thread's block.
    let later = "thread 'a::x' (7) panicked at crates/m/src/a.rs:88:5:\nrest 0.3\nthread 'a::y' (8) panicked at crates/m/src/a.rs:92:5:\nhops 10 over 8\n";
    let mut note = named.to_string();
    assert!(!exec::expect_fail_note(101, &mut note, later, &k));
}

#[test]
fn the_release_plan_prints_one_skip_line_per_known_fail() {
    let p = plan::release_with(&root(), false);
    for k in &entries() {
        assert!(p.notes.contains(&known_fail::note(k)), "{}", k.test);
        assert!(known_fail::note(k).starts_with(&format!("SKIPPED (known fail, #{})", k.issue)));
    }
}
