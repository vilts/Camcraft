//! Directory suites in asset selection, the catalog check and the heavy filter (#3186).
//! Provenance: Cargo package layout, Rust module paths, and our gate law (AGENTS.md "Testing").

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::{assets, catalog, plan};

/// The `<crate> heavy: <binary>` step names of a fixture's heavy plan.
fn heavy_names(f: &Fixture) -> Vec<String> {
    plan::heavy_steps(&f.0).into_iter().map(|s| s.name).collect()
}

#[test]
fn an_asset_named_in_a_child_module_selects_its_directory_suite() {
    let f = fixture();
    f.write("crates/app/tests/posting/main.rs", "//! Gate map: covers app::post.\nmod cases;\n");
    f.write(
        "crates/app/tests/posting/cases.rs",
        "#[test]\nfn posts() {\n    let _ = \"assets/posts/fanuc.cps\";\n}\n",
    );
    let changed = vec!["assets/posts/fanuc.cps".to_string()];
    let asset = assets::changed_assets(&changed).pop().expect("one asset");
    let picks = assets::suites_for(&f.0, &asset).expect("asset selection");
    assert_eq!(picks, vec![("app:posting".to_string(), "names fanuc.cps".to_string())]);
}

#[test]
fn a_catalog_change_selects_a_directory_example_catalog() {
    let f = fixture();
    let changed = vec!["examples/catalog/case.camproj".to_string()];
    let (mut whole, mut why) = (BTreeSet::new(), BTreeMap::new());
    catalog::map_catalog(&f.0, &changed, &mut whole, &mut why);
    assert!(whole.is_empty(), "no example_catalog suite: nothing selected, got {whole:?}");
    f.write("crates/app/tests/example_catalog/main.rs", "mod cases;\n");
    catalog::map_catalog(&f.0, &changed, &mut whole, &mut why);
    assert!(whole.contains("app:example_catalog"), "{whole:?}");
}

#[test]
fn a_directory_suite_without_ignored_tests_gets_no_heavy_step() {
    let f = fixture();
    f.write("crates/app/tests/plain/main.rs", "mod sim;\n#[test]\nfn top() {}\n");
    f.write("crates/app/tests/plain/sim.rs", "#[test]\nfn quick() {}\n");
    let names = heavy_names(&f);
    assert!(!names.iter().any(|n| n == "app heavy: plain"), "{names:?}");
    // The fixture's `group` suite holds an `#[ignore]` only in its child module `cases.rs`.
    assert!(names.iter().any(|n| n == "app heavy: group"), "{names:?}");
}

#[test]
fn a_directory_suite_with_a_missing_module_file_keeps_its_heavy_step() {
    let f = fixture();
    f.write("crates/app/tests/gap/main.rs", "#[cfg(feature = \"x\")]\nmod absent;\n#[test]\nfn top() {}\n");
    let names = heavy_names(&f);
    assert!(names.iter().any(|n| n == "app heavy: gap"), "unknown text counts as heavy: {names:?}");
}

#[test]
fn a_flat_suite_is_read_through_its_module_files() {
    // #3706: a `mod common;` line alone no longer makes a flat suite heavy; its module files are read.
    let f = fixture();
    f.write("crates/app/tests/shared.rs", "mod common;\n#[test]\nfn top() {}\n");
    f.write("crates/app/tests/common/mod.rs", "pub fn helper() {}\nmod deep;\n");
    f.write("crates/app/tests/common/deep.rs", "pub fn deeper() {}\n");
    f.write("crates/app/tests/marked.rs", "mod common2;\n#[test]\nfn top() {}\n");
    f.write("crates/app/tests/common2/mod.rs", "mod deep;\n");
    f.write("crates/app/tests/common2/deep.rs", "#[test]\n#[ignore = \"heavy lane\"]\nfn slow() {}\n");
    let names = heavy_names(&f);
    assert!(!names.iter().any(|n| n == "app heavy: shared"), "{names:?}");
    assert!(names.iter().any(|n| n == "app heavy: marked"), "{names:?}");
    assert!(!names.iter().any(|n| n == "app heavy: flat"), "{names:?}");
}

/// Is `app heavy: <suite>` planned for a flat suite `<suite>.rs` with `entry` text and the given
/// module files (paths under `crates/app/tests/`)?
fn heavy_with(entry: &str, files: &[(&str, &[u8])]) -> bool {
    let f = fixture();
    f.write("crates/app/tests/probe.rs", entry);
    for (path, bytes) in files {
        let path = f.0.join("crates/app/tests").join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("fixture directory");
        std::fs::write(path, bytes).expect("fixture file");
    }
    heavy_names(&f).iter().any(|n| n == "app heavy: probe")
}

const CLEAN: &[u8] = b"pub fn helper() {}\n";
const IGNORED: &[u8] = b"#[test]\n#[ignore = \"heavy lane\"]\nfn slow() {}\n";

#[test]
fn a_clean_flat_suite_with_a_clean_module_gets_no_heavy_step() {
    assert!(!heavy_with("mod a;\n", &[("a.rs", CLEAN)]));
    // A `mod` word in a comment or a string is not a declaration.
    assert!(!heavy_with("mod a; // mod b;\nconst S: &str = \"mod c;\";\n", &[("a.rs", CLEAN)]));
}

#[test]
fn include_or_path_in_a_child_module_keeps_the_heavy_step() {
    assert!(heavy_with("mod a;\n", &[("a.rs", b"include!(\"cases.rs\");\n")]));
    assert!(heavy_with("mod a;\n", &[("a.rs", b"#[path = \"x.rs\"]\nmod b;\n"), ("a/x.rs", CLEAN)]));
}

#[test]
fn a_nested_inline_module_file_is_read() {
    assert!(!heavy_with("mod a {\n    mod b;\n}\n", &[("a/b.rs", CLEAN)]));
    assert!(heavy_with("mod a {\n    mod b;\n}\n", &[("a/b.rs", IGNORED)]));
}

#[test]
fn an_unreadable_module_keeps_the_heavy_step() {
    assert!(heavy_with("mod a;\n", &[("a.rs", &[0xff, 0xfe, 0x00, 0xc3])]));
}

#[test]
fn a_mod_token_the_scan_cannot_read_keeps_the_heavy_step() {
    // Two declarations on one line: the scan reads only `a`, so `b` is unknown.
    assert!(heavy_with("mod a; mod b;\n", &[("a.rs", CLEAN), ("b.rs", IGNORED)]));
    // An attribute before the declaration on the same line.
    assert!(heavy_with("#[cfg(test)] mod b;\n", &[("b.rs", IGNORED)]));
}

#[test]
fn a_cfg_attr_path_keeps_the_heavy_step() {
    let entry = "#[cfg_attr(feature = \"x\", path = \"other.rs\")]\nmod b;\n";
    assert!(heavy_with(entry, &[("b.rs", CLEAN), ("other.rs", IGNORED)]));
}

/// A directory suite `many` with two private submodules, a shared helper module and a cfg-gated one.
fn many() -> Fixture {
    let f = fixture();
    let private = |t: &str| format!("fn helper() -> u8 {{ 1 }}\n#[test]\nfn {t}() {{\n    assert_eq!(helper(), 1);\n}}\n");
    f.write("crates/app/tests/many/main.rs", "mod alpha;\nmod beta;\nmod shared;\nmod gated;\n#[test]\nfn top() {}\n");
    f.write("crates/app/tests/many/alpha.rs", &private("a1"));
    f.write("crates/app/tests/many/beta.rs", &private("b1"));
    f.write("crates/app/tests/many/shared.rs", "pub fn h() {}\n#[test]\nfn s1() {}\n");
    f.write("crates/app/tests/many/gated.rs", "#[cfg(feature = \"x\")]\nfn helper() {}\n#[test]\nfn g1() {}\n");
    f
}

/// The quick plan of a diff whose every file has the one hunk `hunk`.
fn quick_files(f: &Fixture, files: &[&str], hunk: &str) -> Plan {
    let changed: Vec<String> = files.iter().map(|s| s.to_string()).collect();
    let hunks = |_: &str| vec![hunk.to_string()];
    let before = |file: &str| std::fs::read_to_string(f.0.join(file)).ok();
    let diff = Diff { changed: &changed, untracked: &[], hunks: &hunks, before: &before };
    match quick::plan(&f.0, &diff, false).expect("quick plan") {
        Quick::Plan(p) => p,
        other => panic!("unexpected {other:?}"),
    }
}

const WHOLE_MANY: &str = "test -j 6 -p camcraft-app --no-fail-fast --test many -- --test-threads=6";

#[test]
fn a_private_submodule_change_runs_only_that_submodule() {
    let f = many();
    let p = quick_files(&f, &["crates/app/tests/many/alpha.rs"], "@@ -1 +1 @@");
    assert_eq!(args(&p), ["test -j 6 -p camcraft-app --no-fail-fast --test many -- --test-threads=6 alpha::"]);
    assert!(p.notes.iter().any(|n| n.contains("app:many  alpha:: - submodule changed")), "{:?}", p.notes);
    // A change inside a test fn of the submodule still runs that one fn.
    let p = quick_files(&f, &["crates/app/tests/many/alpha.rs"], "@@ -4 +4 @@");
    assert_eq!(args(&p), ["test -j 6 -p camcraft-app --test many -- --test-threads=6 --exact alpha::a1"]);
}

#[test]
fn main_shared_gated_or_two_submodules_run_the_whole_suite() {
    let f = many();
    for files in [
        &["crates/app/tests/many/main.rs"][..],
        &["crates/app/tests/many/shared.rs"],
        &["crates/app/tests/many/gated.rs"],
        &["crates/app/tests/many/alpha.rs", "crates/app/tests/many/beta.rs"],
    ] {
        let p = quick_files(&f, files, "@@ -1 +1 @@");
        assert_eq!(args(&p), [WHOLE_MANY], "{files:?}");
    }
}

#[test]
fn a_submodule_that_was_shared_at_the_base_runs_the_whole_suite() {
    let f = many();
    let changed = vec!["crates/app/tests/many/alpha.rs".to_string()];
    let hunks = |_: &str| vec!["@@ -1 +1 @@".to_string()];
    let before = |_: &str| Some("pub fn helper() -> u8 { 1 }\n#[test]\nfn a1() {}\n".to_string());
    let diff = Diff { changed: &changed, untracked: &[], hunks: &hunks, before: &before };
    let Ok(Quick::Plan(p)) = quick::plan(&f.0, &diff, false) else { panic!("quick plan") };
    assert_eq!(args(&p), [WHOLE_MANY]);
}

/// The plan for an edit on line 1 of `alpha.rs` when it holds `text` (also at the merge base).
fn alpha_plan(text: &str) -> Vec<String> {
    let f = many();
    f.write("crates/app/tests/many/alpha.rs", text);
    args(&quick_files(&f, &["crates/app/tests/many/alpha.rs"], "@@ -1 +1 @@"))
}

#[test]
fn a_trait_impl_or_a_scoped_pub_in_a_submodule_runs_the_whole_suite() {
    const TEST: &str = "#[test]\nfn a1() {}\n";
    for item in [
        // Other submodules see a trait impl (#3711).
        "struct A;\nimpl std::fmt::Display for A {\n    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, \"a\") }\n}\n",
        "struct A;\nimpl<T> From<T> for A { fn from(_: T) -> A { A } }\n",
        "struct A;\n#[allow(dead_code)] unsafe impl Send for A {}\n",
        // `pub` after an attribute on the same line, and each scoped `pub`.
        "#[allow(dead_code)] pub fn helper() {}\n",
        "#[inline] #[allow(dead_code)] pub(crate) fn helper() {}\n",
        "pub(crate) fn helper() {}\n",
        "pub(super) fn helper() {}\n",
        "pub(in crate::many) fn helper() {}\n",
    ] {
        assert_eq!(alpha_plan(&format!("{item}{TEST}")), [WHOLE_MANY], "{item}");
    }
}

#[test]
fn an_inherent_impl_of_a_private_type_keeps_the_submodule_filter() {
    let text = "struct A;\nimpl A {\n    fn one(&self) -> u8 { 1 }\n}\n#[test]\nfn a1() {\n    assert_eq!(A.one(), 1);\n}\n";
    assert_eq!(
        alpha_plan(text),
        ["test -j 6 -p camcraft-app --no-fail-fast --test many -- --test-threads=6 alpha::"]
    );
}

#[test]
fn a_submodule_step_runs_only_that_submodule_in_real_cargo() {
    // The fixture suite `many` holds five tests: top, alpha::a1, beta::b1, shared::s1, gated::g1.
    let f = many();
    let p = quick_files(&f, &["crates/app/tests/many/alpha.rs"], "@@ -1 +1 @@");
    assert_eq!(p.steps.len(), 1, "{:?}", args(&p));
    let stdout = run_step(&f.0, &p.steps[0], 1);
    assert!(stdout.contains("alpha::a1"), "{stdout}");
    assert!(stdout.contains("4 filtered out"), "the other submodules must not run: {stdout}");
}
