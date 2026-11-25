//! Pins the file-level heavy reach (#3560): a narrower reach only where the file is proven to be
//! test-only or binary-only code, and the crate reach for every other file.
//! Provenance: our own build law (AGENTS.md "Heavy lanes at review"); the Rust reference module rules.

use std::path::{Path, PathBuf};

use crate::heavy;
use crate::heavy_files::{Seen, seen_by};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn names(changed: &[&str]) -> Vec<String> {
    let changed: Vec<String> = changed.iter().map(|s| (*s).to_string()).collect();
    heavy::plan(&root(), &changed).expect("heavy plan").steps.into_iter().map(|s| s.name).collect()
}

/// A scratch crate `crates/demo` with the given files (path under the crate, text).
fn scratch(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let r = std::env::temp_dir().join(format!("xtask-heavy-files-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&r);
    for (p, text) in files {
        let f = r.join("crates/demo").join(p);
        std::fs::create_dir_all(f.parent().expect("parent")).expect("mkdir");
        std::fs::write(f, text).expect("write");
    }
    r
}

const MANIFEST: (&str, &str) = ("Cargo.toml", "[package]\nname = \"camcraft-demo\"\n");

#[test]
fn a_cfg_test_module_reaches_only_the_unit_tests_of_its_crate() {
    // Top-level and nested `#[cfg(test)] mod x;` (lib.rs and drilling/mod.rs of motion).
    for f in ["crates/motion/src/ring_zig_tests.rs", "crates/motion/src/drilling/tests_cross.rs"] {
        let n = names(&[f]);
        assert_eq!(n[..2], ["heavy build (release --no-run)", "motion heavy: lib"], "{f}");
        // The known-fail checks of the motion unit tests (#2567) stay with the lib target.
        assert!(n[2..].iter().all(|s| s.starts_with("motion heavy: known fail #")), "{f}: {n:?}");
    }
}

#[test]
fn library_code_keeps_the_crate_reach() {
    // A normal module of motion still reaches every motion and app heavy target.
    let wide = names(&["crates/motion/src/drilling/boss.rs"]);
    let lib_rs = names(&["crates/motion/src/lib.rs"]);
    let mod_rs = names(&["crates/motion/src/drilling/mod.rs"]);
    assert_eq!(wide, lib_rs);
    assert_eq!(wide, mod_rs);
    assert!(wide.contains(&"app heavy: lib".to_string()) && wide.contains(&"app heavy: doc".to_string()), "{wide:?}");
    assert!(wide.iter().any(|n| n.starts_with("motion heavy: ") && n != "motion heavy: lib"), "{wide:?}");
    // A test-only file and a library file together: the library file decides.
    assert_eq!(names(&["crates/motion/src/ring_zig_tests.rs", "crates/motion/src/drilling/boss.rs"]), wide);
}

#[test]
fn a_binary_source_reaches_the_binaries_and_integration_tests_of_its_crate_only() {
    let n = names(&["crates/app/src/bin/camcraft-cli.rs"]);
    assert!(n.iter().skip(1).all(|s| s.starts_with("app heavy: ")), "{n:?}");
    assert!(!n.contains(&"app heavy: lib".to_string()) && !n.contains(&"app heavy: doc".to_string()), "{n:?}");
    // Each integration binary of app that has an ignored test stays: it can run the CLI.
    let full = names(&["crates/app/src/lib.rs"]);
    let app_tests: Vec<&String> = full.iter().filter(|s| s.starts_with("app heavy: ") && !s.ends_with(": lib") && !s.ends_with(": doc")).collect();
    assert!(app_tests.iter().all(|s| n.contains(s)), "{n:?}");
}

#[test]
fn every_doubt_answers_library() {
    let r = scratch(
        "doubt",
        &[
            MANIFEST,
            ("src/lib.rs", "#[cfg(test)]\nmod t;\n#[cfg(test)]\nmod both;\n#[cfg(not(test))]\nmod both;\nmod plain;\n#[cfg(test)]\n#[path = \"p.rs\"]\nmod p;\nmod inline {\n    #[cfg(test)]\n    mod deep;\n}\n#[cfg(any(test, feature = \"x\"))]\nmod feat;\n"),
            ("src/t.rs", ""),
            ("src/both.rs", ""),
            ("src/plain.rs", "#[cfg(test)]\nmod sub;\n"),
            ("src/plain/sub.rs", ""),
            ("src/p.rs", ""),
            ("src/inline/deep.rs", ""),
            ("src/feat.rs", ""),
            ("src/undeclared.rs", ""),
        ],
    );
    assert!(matches!(seen_by(&r, "demo", "t.rs"), Seen::UnitTestsOnly(_)));
    assert!(matches!(seen_by(&r, "demo", "plain/sub.rs"), Seen::UnitTestsOnly(_)), "a cfg(test) leaf under a normal module");
    for rel in ["lib.rs", "both.rs", "plain.rs", "p.rs", "inline/deep.rs", "feat.rs", "undeclared.rs", "build.rs", "data.toml"] {
        assert_eq!(seen_by(&r, "demo", rel), Seen::Library, "{rel}");
    }
    let _ = std::fs::remove_dir_all(&r);
}

#[test]
fn an_include_or_a_target_path_keeps_the_crate_reach() {
    let lib = "#[cfg(test)]\nmod t;\n";
    let r = scratch("include", &[MANIFEST, ("src/lib.rs", lib), ("src/t.rs", ""), ("src/other.rs", "include!(\"t.rs\");\n")]);
    assert_eq!(seen_by(&r, "demo", "t.rs"), Seen::Library, "include! of the file");
    let _ = std::fs::remove_dir_all(&r);
    let split = ("src/other.rs", "#[path =\n    \"../src/t.rs\"]\nmod q;\n");
    let r = scratch("split", &[MANIFEST, ("src/lib.rs", lib), ("src/t.rs", ""), split]);
    assert_eq!(seen_by(&r, "demo", "t.rs"), Seen::Library, "#[path] over two lines");
    let _ = std::fs::remove_dir_all(&r);
    let r = scratch("longer", &[MANIFEST, ("src/lib.rs", lib), ("src/t.rs", ""), ("src/other.rs", "#[path = \"air_t.rs\"]\nmod q;\n")]);
    assert!(matches!(seen_by(&r, "demo", "t.rs"), Seen::UnitTestsOnly(_)), "air_t.rs does not name t.rs");
    let _ = std::fs::remove_dir_all(&r);
    let custom = ("Cargo.toml", "[package]\nname = \"camcraft-demo\"\n[lib]\npath = \"src/lib.rs\"\n");
    let r = scratch("path", &[custom, ("src/lib.rs", lib), ("src/t.rs", ""), ("src/main.rs", "")]);
    assert_eq!(seen_by(&r, "demo", "t.rs"), Seen::Library, "[lib] path");
    assert_eq!(seen_by(&r, "demo", "main.rs"), Seen::Library, "[lib] path");
    let _ = std::fs::remove_dir_all(&r);
    let r = scratch("nomanifest", &[("src/lib.rs", lib), ("src/t.rs", "")]);
    assert_eq!(seen_by(&r, "demo", "t.rs"), Seen::Library, "no manifest");
    let _ = std::fs::remove_dir_all(&r);
}

#[test]
fn the_dry_run_names_each_skipped_target_and_the_counts() {
    let changed = vec!["crates/motion/src/ring_zig_tests.rs".to_string()];
    let p = heavy::plan(&root(), &changed).expect("plan");
    assert!(p.notes.iter().any(|l| l.contains("motion heavy: lib - crates/motion/src/ring_zig_tests.rs changed: module ring_zig_tests is #[cfg(test)]")), "{:#?}", p.notes);
    assert!(p.notes.iter().any(|l| l == "  not run: app heavy: lib - the diff does not reach it"), "{:#?}", p.notes);
    let all = crate::plan::heavy_steps(&root()).len() - 1;
    let reached = p.steps.len() - 1;
    let want = format!("  heavy reach: {reached} step(s) reached, {} skipped (not run)", all - reached);
    assert!(p.notes.contains(&want), "{want} in {:#?}", p.notes);
}

#[test]
fn a_second_compile_root_that_names_the_file_keeps_the_crate_reach() {
    let lib = ("src/lib.rs", "#[cfg(test)]\nmod t;\npub mod a;\n");
    let base = [MANIFEST, lib, ("src/t.rs", ""), ("src/a/mod.rs", "#[cfg(test)]\nmod u;\n"), ("src/a/u.rs", "")];
    let probes: [(&str, (&str, &str), &str); 4] = [
        ("mainmod", ("src/main.rs", "mod t;\nfn main() {}\n"), "t.rs"),
        ("testspath", ("tests/x.rs", "#[path = \"../src/t.rs\"]\nmod t;\n"), "t.rs"),
        ("binpath", ("src/bin/x.rs", "#[path = \"../t.rs\"]\nmod t;\nfn main() {}\n"), "t.rs"),
        // main.rs builds module `a` again, so a/u.rs is in the unit tests of the binary too.
        ("mainancestor", ("src/main.rs", "mod a;\nfn main() {}\n"), "a/u.rs"),
    ];
    for (tag, extra, rel) in probes {
        let mut files = base.to_vec();
        files.push(extra);
        let r = scratch(tag, &files);
        assert_eq!(seen_by(&r, "demo", rel), Seen::Library, "{tag}");
        let _ = std::fs::remove_dir_all(&r);
    }
    // A `mod t;` of a test binary loads tests/t.rs, not src/t.rs: the narrow answer stays.
    let mut files = base.to_vec();
    files.push(("tests/x.rs", "mod t;\n"));
    let r = scratch("testsmod", &files);
    assert!(matches!(seen_by(&r, "demo", "t.rs"), Seen::UnitTestsOnly(_)));
    assert!(matches!(seen_by(&r, "demo", "a/u.rs"), Seen::UnitTestsOnly(_)));
    let _ = std::fs::remove_dir_all(&r);
}
