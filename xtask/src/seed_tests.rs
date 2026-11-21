//! Pins of the worktree target seed (#2575): a fresh checkout finds a build-script dependency Fresh, and
//! never takes a local package's stale unit. Provenance: our own tests.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use crate::seed;

/// A scratch directory of its own for each test.
fn scratch(tag: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("camcraft-seed-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, text).expect("write");
}

/// Cargo in `dir` with its own CARGO_HOME and no wrapper or shared target dir; (ok, stderr).
fn cargo(dir: &Path, home: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(crate::exec::cargo())
        .args(args)
        .current_dir(dir)
        .env("CARGO_HOME", home)
        .env_remove("RUSTC_WRAPPER")
        .env_remove("CARGO_BUILD_RUSTC_WRAPPER")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_BUILD_TARGET_DIR")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .output()
        .expect("cargo runs");
    (out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=seed", "-c", "user.email=seed@example.invalid", "-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs")
        .status
        .success();
    assert!(ok, "git {args:?}");
}

/// A git dependency with a build script that writes the source the library includes from OUT_DIR, as
/// serde and the solidcore crates do; and one checkout `a` of a package that uses it, built once.
fn fixture(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = scratch(tag);
    let (home, base, dep, a) = (root.join("home"), root.join("base"), root.join("dep"), root.join("a"));
    let repo = |dir: &Path| {
        git(dir, &["init", "-q"]);
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", "fixture"]);
        let path = dir.display().to_string().replace('\\', "/");
        format!("file://{}{path}", if path.starts_with('/') { "" } else { "/" })
    };
    write(&base.join("Cargo.toml"), "[package]\nname = \"seedbase\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
    write(&base.join("src/lib.rs"), "pub const B: u32 = 3;\n");
    let base_url = repo(&base);
    let dep_manifest = format!(
        "[package]\nname = \"seeddep\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [dependencies]\nseedbase = {{ git = \"{base_url}\" }}\n"
    );
    write(&dep.join("Cargo.toml"), &dep_manifest);
    write(
        &dep.join("build.rs"),
        "fn main() {\n    let out = std::env::var(\"OUT_DIR\").unwrap();\n    \
         std::fs::write(format!(\"{out}/gen.rs\"), \"pub const N: u32 = 7 + seedbase::B;\").unwrap();\n}\n",
    );
    write(&dep.join("src/lib.rs"), "include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\n");
    let url = repo(&dep);
    let manifest = format!(
        "[package]\nname = \"seedapp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n\
         [dependencies]\nseeddep = {{ git = \"{url}\" }}\n"
    );
    write(&a.join("Cargo.toml"), &manifest);
    write(&a.join("src/lib.rs"), "pub const LOCAL: u32 = 1;\npub fn n() -> u32 { seeddep::N + LOCAL }\n");
    let (ok, err) = cargo(&a, &home, &["build"]);
    assert!(ok, "checkout a builds: {err}");
    (home, a, root)
}

/// A second checkout `b` of the same package, with `LOCAL = 2`, its source older than a's build.
fn second_checkout(root: &Path, a: &Path) -> PathBuf {
    let b = root.join("b");
    write(&b.join("Cargo.toml"), &std::fs::read_to_string(a.join("Cargo.toml")).expect("manifest"));
    std::fs::copy(a.join("Cargo.lock"), b.join("Cargo.lock")).expect("lock");
    let lib = b.join("src/lib.rs");
    write(&lib, "pub const LOCAL: u32 = 2;\npub fn n() -> u32 { seeddep::N + LOCAL }\n");
    let old = SystemTime::now() - Duration::from_secs(3600);
    std::fs::File::options().write(true).open(&lib).expect("open").set_modified(old).expect("mtime");
    // `cargo xtask` builds its own dependencies in b before the seed: b has seedbase's units already, newer.
    let mut copied = 0;
    for part in ["build", "deps", ".fingerprint"] {
        let (from, to) = (a.join("target/debug").join(part), b.join("target/debug").join(part));
        let Ok(entries) = std::fs::read_dir(&from) else {
            continue;
        };
        for item in entries {
            let item = item.expect("unit entry");
            let name = item.file_name().to_string_lossy().into_owned();
            if name == "seedbase" || name.starts_with("seedbase-") || name.starts_with("libseedbase-") {
                touched_copy(&item.path(), &to.join(name));
                copied += 1;
            }
        }
    }
    assert!(copied > 0, "the fixture refreshes an existing dependency in either Cargo layout");
    b
}

/// Copies a file or tree with the time of now on each file.
fn touched_copy(from: &Path, to: &Path) {
    if from.is_dir() {
        std::fs::create_dir_all(to).expect("mkdir");
        for item in std::fs::read_dir(from).expect("read").flatten() {
            touched_copy(&item.path(), &to.join(item.file_name()));
        }
    } else {
        std::fs::create_dir_all(to.parent().expect("parent")).expect("mkdir");
        std::fs::copy(from, to).expect("copy");
        std::fs::File::options().write(true).open(to).expect("open").set_modified(SystemTime::now()).expect("mtime");
    }
}

fn local_packages(dir: &Path, home: &Path) -> Vec<String> {
    let out = Command::new(crate::exec::cargo())
        .args(["metadata", "--format-version", "1"])
        .current_dir(dir)
        .env("CARGO_HOME", home)
        .output()
        .expect("cargo metadata");
    let json = crate::json::parse(&String::from_utf8_lossy(&out.stdout)).expect("metadata json");
    seed::local_packages(&json)
}

#[test]
fn a_seeded_checkout_finds_a_build_script_dependency_fresh() {
    let (home, a, root) = fixture("fresh");
    let b = second_checkout(&root, &a);
    let local = local_packages(&b, &home);
    assert_eq!(local, ["seedapp"]);
    let note = seed::seed(&b, std::slice::from_ref(&a), &local).expect("seed");
    let (ok, err) = cargo(&b, &home, &["build", "-v"]);
    assert!(ok, "checkout b builds: {err}");
    assert!(
        !err.contains("Compiling seeddep"),
        "fresh-worktree build-script dependency must not compile again:\n{err}"
    );
    assert!(err.contains("Fresh seeddep"), "seeddep is Fresh:\n{err}");
    // The local package is never taken from a: its older source would read as Fresh and hide LOCAL = 2.
    assert!(err.contains("Compiling seedapp"), "the local package compiles in b:\n{err}");
    assert!(note.is_some_and(|n| n.contains("seeded")), "the seed says what it did");
    assert!(b.join("target/debug").join(seed::MARKER).is_file(), "the marker is written");
    let again = seed::seed(&b, &[a], &local).expect("second seed");
    assert!(again.is_none(), "a seeded worktree is not seeded again");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_checkout_where_a_build_runs_is_passed_over() {
    let root = scratch("busy");
    let (big, small, dest) = (root.join("big"), root.join("small"), root.join("dest"));
    for (checkout, units) in [(&big, 2), (&small, 1)] {
        for n in 0..units {
            write(&checkout.join(format!("target/debug/.fingerprint/dep{n}-00/hash")), "x");
        }
        write(&checkout.join(format!("target/debug/deps/libonly_{}-00.rlib", units)), "rlib");
    }
    write(&big.join("target/debug/.cargo-lock"), "");
    let held = std::fs::File::options().write(true).open(big.join("target/debug/.cargo-lock")).expect("open");
    held.try_lock().expect("the test holds big's build lock");
    let note = seed::seed(&dest, &[big.clone(), small.clone()], &[]).expect("seed").expect("a note");
    assert!(note.contains(&small.display().to_string()), "the free checkout seeds: {note}");
    assert!(dest.join("target/debug/deps/libonly_1-00.rlib").is_file(), "small's unit is here");
    assert!(!dest.join("target/debug/deps/libonly_2-00.rlib").exists(), "nothing is read from the busy checkout");
    let dest2 = root.join("dest2");
    let note = seed::seed(&dest2, &[big], &[]).expect("seed").expect("a note");
    assert!(note.contains("a build runs"), "all sources busy: {note}");
    assert!(!dest2.join("target/debug").join(seed::MARKER).exists(), "no marker: a later gate tries again");
    drop(held);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn local_packages_are_the_ones_without_a_source() {
    let json = crate::json::parse(
        r#"{"packages":[{"name":"camcraft-app","source":null},{"name":"serde","source":"registry+x"},{"name":"own"}]}"#,
    )
    .expect("json");
    assert_eq!(seed::local_packages(&json), ["camcraft-app", "own"]);
}

#[test]
fn a_unit_entry_of_a_local_package_is_never_copied() {
    let local = ["camcraft-app".to_string(), "libc".to_string()];
    for e in [
        "camcraft-app",
        "camcraft_app",
        "libc",
        "camcraft-app-0a1b",
        "libcamcraft_app-0a1b.rlib",
        "camcraft_app-0a1b.d",
        "liblibc-9f.rlib",
        "libc-9f",
    ] {
        assert!(seed::is_local(e, &local), "{e} is local");
    }
    for e in ["serde-0a1b", "libserde-0a1b.rmeta", "solidcore-77", "libsolidcore-77.rlib"] {
        assert!(!seed::is_local(e, &local), "{e} is not local");
    }
}

#[test]
fn both_artifact_layouts_copy_dependencies_and_exclude_local_packages() {
    let root = scratch("layouts");
    for nested in [false, true] {
        let source = root.join(if nested { "nested" } else { "legacy" });
        let dest = root.join(if nested { "nested-dest" } else { "legacy-dest" });
        let files = if nested {
            [
                "build/serde/00/fingerprint/lib-serde",
                "build/serde/00/out/libserde.rlib",
                "build/camcraft-app/00/fingerprint/lib-camcraft_app",
                "build/camcraft-app/00/out/libcamcraft_app.rlib",
            ]
        } else {
            [
                ".fingerprint/serde-00/lib-serde",
                "deps/libserde-00.rlib",
                ".fingerprint/camcraft-app-00/lib-camcraft_app",
                "deps/libcamcraft_app-00.rlib",
            ]
        };
        for file in files {
            write(&source.join("target/debug").join(file), "unit");
        }
        let note = seed::seed(&dest, &[source], &["camcraft-app".into()]).expect("seed").expect("note");
        assert!(note.contains("seeded"), "the source scan finds the layout: {note}");
        for file in &files[..2] {
            assert_eq!(std::fs::read_to_string(dest.join("target/debug").join(file)).expect("dependency unit"), "unit");
        }
        for file in &files[2..] {
            assert!(!dest.join("target/debug").join(file).exists(), "local artifact is excluded: {file}");
        }
    }
    let _ = std::fs::remove_dir_all(root);
}
