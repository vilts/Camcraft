//! Pins `cargo xtask catalog` CLI contract, checks, and safety refusals (#2393).
//! Provenance: CamCraft example catalog build/regression tooling tests.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn xtask_bin() -> &'static str {
    env!("CARGO_BIN_EXE_xtask")
}

#[test]
fn help_lists_catalog_command() {
    let out = Command::new(xtask_bin())
        .arg("--help")
        .output()
        .expect("run xtask --help");
    assert!(out.status.success(), "--help should succeed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("catalog"),
        "USAGE must list `cargo xtask catalog`, got:\n{stdout}"
    );
}

#[test]
fn catalog_check_verifies_seed_case() {
    let out = Command::new(xtask_bin())
        .current_dir(workspace_root())
        .args(["catalog", "--case", "2d/first-part", "--check"])
        .output()
        .expect("run xtask catalog --check");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "catalog --check failed (status {:?})\nstdout:\n{stdout}\nstderr:\n{stderr}",
        out.status.code()
    );
    assert!(
        stdout.contains("checked 2d/first-part"),
        "expected checked 2d/first-part output, got:\n{stdout}"
    );
}

#[test]
fn catalog_refuses_unsafe_branch_and_in_tree_output_and_bad_flags() {
    let root = workspace_root();
    for (args, needle) in [
        (
            &["catalog", "--screenshots", "--screenshots-branch", "main"][..],
            "main",
        ),
        (
            &[
                "catalog",
                "--screenshots",
                "--screenshots-out",
                "docs/inside",
            ][..],
            "outside",
        ),
        (&["catalog", "--with-guide"][..], "--screenshots"),
        (&["catalog", "--check", "--rebuild"][..], "--check"),
        (&["catalog", "--bogus-flag"][..], "--bogus-flag"),
    ] {
        let out = Command::new(xtask_bin())
            .current_dir(&root)
            .args(args)
            .output()
            .expect("run xtask catalog");
        assert!(!out.status.success(), "expected failure for {args:?}");
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            combined.contains(needle),
            "expected refusal naming '{needle}' for {args:?}, got:\n{combined}"
        );
    }
}

#[cfg(unix)]
#[test]
fn catalog_screenshots_resolves_new_relative_outside_directory() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "camcraft-catalog-relative-out-{}",
        std::process::id()
    )));
    fs::create_dir(&scratch.0).expect("new isolated fixture");
    let root = scratch.0.join("repo");
    fs::create_dir_all(root.join("examples/catalog/2d/first-part")).unwrap();
    fs::create_dir_all(root.join("crates/ui")).unwrap();
    let init = Command::new("git").arg("init").arg(&root).output().unwrap();
    assert!(init.status.success(), "git init: {init:?}");

    // Stand in for cargo's renderer subprocess, including its crates/ui CWD.
    // A PNG's bytes are opaque to the collection and images-only commit steps.
    let renderer = scratch.0.join("renderer");
    fs::write(
        &renderer,
        "#!/bin/sh\nset -eu\nmkdir -p \"$CAMCRAFT_CATALOG_SCREENSHOTS_OUT/2d/first-part\"\nprintf 'fixture image' > \"$CAMCRAFT_CATALOG_SCREENSHOTS_OUT/2d/first-part/first-part.png\"\nprintf '%s' \"$CAMCRAFT_CATALOG_SCREENSHOTS_OUT\" > \"$CAPTURE_RENDERER_OUT\"\n",
    )
    .unwrap();
    fs::set_permissions(&renderer, fs::Permissions::from_mode(0o755)).unwrap();
    let capture = scratch.0.join("renderer-out");
    let expected = scratch.0.join("new-shots");
    assert!(!expected.exists(), "output directory must start absent");

    let out = Command::new(xtask_bin())
        .current_dir(&root)
        .env("CARGO_MANIFEST_DIR", root.join("xtask"))
        .env("CARGO", &renderer)
        .env("CAPTURE_RENDERER_OUT", &capture)
        .args([
            "catalog",
            "--case",
            "2d/first-part",
            "--screenshots",
            "--screenshots-branch",
            "pr-screenshots",
            "--screenshots-out",
            "../new-shots",
        ])
        .output()
        .expect("run catalog screenshots with relative outside output");
    assert!(out.status.success(), "catalog screenshots failed: {out:?}");
    assert_eq!(
        PathBuf::from(fs::read_to_string(capture).unwrap()),
        expected.canonicalize().unwrap(),
        "renderer must receive the same absolute output directory created by the caller"
    );
    assert_eq!(
        fs::read(expected.join("2d/first-part/first-part.png")).unwrap(),
        b"fixture image"
    );
    assert!(
        !root.join("crates/new-shots").exists(),
        "no output inside source tree"
    );
    let image = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args([
            "show",
            "pr-screenshots:catalog/2d/first-part/first-part.png",
        ])
        .output()
        .unwrap();
    assert!(
        image.status.success(),
        "collection must find and commit the rendered image"
    );
    assert_eq!(image.stdout, b"fixture image");
}
