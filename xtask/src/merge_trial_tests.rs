//! Provenance: #3459; package selection and diagnostic ownership for the merge trial.
use super::*;
use crate::merge_plan::Mergeable;

const META: &str = r#"{"workspace_members":["a","b","c"],"packages":[
 {"id":"a","name":"named-a","manifest_path":"/trial/crates/a/Cargo.toml"},
 {"id":"b","name":"named-b","manifest_path":"/trial/crates/b/Cargo.toml"},
 {"id":"c","name":"xtask","manifest_path":"/trial/xtask/Cargo.toml"},
 {"id":"dep","name":"external","manifest_path":"/cache/external/Cargo.toml"}]}"#;
fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}
fn pr(n: u32, paths: &[&str]) -> Pr {
    Pr {
        number: n,
        title: format!("PR {n}"),
        head: format!("head{n}"),
        paths: strings(paths),
        mergeable: Mergeable::Clean,
        merge_bases: 0,
    }
}
#[test]
fn selection_uses_metadata_names_and_only_touched_members() {
    assert_eq!(
        packages(
            Path::new("/trial"),
            META,
            &strings(&["crates/a/tests/nested/case.rs", "xtask/src/main.rs", "crates/a/src/lib.rs"])
        )
        .unwrap(),
        strings(&["named-a", "xtask"])
    );
    assert!(
        packages(
            Path::new("/trial"),
            META,
            &strings(&["docs/guide.md", ".claude/skills/merge-ready/SKILL.md"])
        )
        .unwrap()
        .is_empty()
    );
}
#[test]
fn shared_inputs_select_all_workspace_members_not_external_dependencies() {
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        ".cargo/config.toml",
        "rust-toolchain.toml",
        "assets/posts/example.cps",
        "examples/input.cnc",
    ] {
        assert_eq!(
            packages(Path::new("/trial"), META, &strings(&[path])).unwrap(),
            strings(&["named-a", "named-b", "xtask"]),
            "{path}"
        );
    }
}
#[test]
fn malformed_metadata_is_an_error_not_an_empty_selection() {
    for metadata in ["not json", "{}", "{\"workspace_members\":[]}"] {
        assert!(packages(Path::new("/trial"), metadata, &strings(&["crates/a/src/lib.rs"])).is_err());
    }
}
#[test]
fn diagnostics_find_owners_in_absolute_relative_and_child_spans() {
    let prs = [pr(1, &["crates/a/src/lib.rs"]), pr(2, &["crates/b/tests/case.rs"])];
    let text = r#"{"reason":"compiler-message","message":{"level":"warning","spans":[{"file_name":"crates/a/src/lib.rs"}]}}
{"reason":"compiler-message","message":{"level":"error","rendered":"missing field","spans":[{"file_name":"/trial/crates/b/tests/case.rs"}],"children":[{"spans":[{"file_name":"crates/a/src/lib.rs"}]}]}}
{"reason":"build-finished","success":false}"#;
    let errors = diagnostics(Path::new("/trial"), text, &prs);
    assert_eq!(errors.keys().copied().collect::<Vec<_>>(), vec![1, 2]);
    assert!(errors[&2][0].contains("crates/b/tests/case.rs"));
}
#[test]
fn unowned_errors_exclude_every_candidate_instead_of_guessing() {
    let prs = [pr(1, &["a.rs"]), pr(2, &["b.rs"])];
    let text = r#"{"reason":"compiler-message","message":{"level":"error","rendered":"dependency failure","spans":[{"file_name":"/cache/dependency/src/lib.rs"}]}}"#;
    assert_eq!(
        diagnostics(Path::new("/trial"), text, &prs).keys().copied().collect::<Vec<_>>(),
        vec![1, 2]
    );
}
#[test]
fn check_flag_cannot_silently_run_delta_mode_instead() {
    let args = strings(&["--check", "--delta", "1", "--reviewed", "abcd"]);
    assert!(
        crate::merge_plan_git::parse(&args)
            .unwrap_err()
            .contains("cannot be used with --delta")
    );
    assert!(crate::merge_plan_git::parse(&strings(&["--check", "1"])).unwrap().check);
}
