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
    run_catalog_screenshots(false);
}

#[cfg(unix)]
#[test]
fn catalog_screenshots_pdf_stages_blank_and_operation_images() {
    run_catalog_screenshots(true);
}

#[cfg(unix)]
fn run_catalog_screenshots(pdf: bool) {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "camcraft-catalog-relative-out-{}-{pdf}",
        std::process::id()
    )));
    fs::create_dir(&scratch.0).expect("new isolated fixture");
    let root = scratch.0.join("repo");
    fs::create_dir_all(root.join("examples/catalog/2d/first-part")).unwrap();
    fs::create_dir_all(root.join("crates/ui")).unwrap();
    fs::write(
        root.join("examples/catalog/2d/first-part/README.md"),
        "# First part\n",
    )
    .unwrap();
    let init = Command::new("git").arg("init").arg(&root).output().unwrap();
    assert!(init.status.success(), "git init: {init:?}");
    let remote = scratch.0.join("images.git");
    assert!(Command::new("git")
        .args(["init", "--bare"]).arg(&remote).status().unwrap().success());
    assert!(Command::new("git")
        .arg("-C").arg(&root).args(["remote", "add", "origin"])
        .arg(&remote).status().unwrap().success());

    // Stand in for cargo's renderer subprocess, including its crates/ui CWD.
    // A PNG's bytes are opaque to the collection and images-only commit steps.
    let renderer = scratch.0.join("renderer");
    fs::write(
        &renderer,
        "#!/bin/sh\nset -eu\nmkdir -p \"$CAMCRAFT_CATALOG_SCREENSHOTS_OUT/2d/first-part\"\nprintf 'fixture image' > \"$CAMCRAFT_CATALOG_SCREENSHOTS_OUT/2d/first-part/first-part.png\"\nprintf '%s' \"$CAMCRAFT_CATALOG_SCREENSHOTS_OUT\" > \"$CAPTURE_RENDERER_OUT\"\n",
    )
    .unwrap();
    fs::set_permissions(&renderer, fs::Permissions::from_mode(0o755)).unwrap();
    let script = fs::read_to_string(&renderer).unwrap();
    fs::write(&renderer, format!("{script}\nfor name in 0-blank 1-Face1 2-2D-Pocket1 3-Drill1; do\n  printf '%s' \"$name\" > \"$CAMCRAFT_CATALOG_SCREENSHOTS_OUT/2d/first-part/$name.png\"\ndone\n")).unwrap();
    let capture = scratch.0.join("renderer-out");
    let expected = scratch.0.join("new-shots");
    assert!(!expected.exists(), "output directory must start absent");

    let tools = scratch.0.join("tools");
    fs::create_dir(&tools).unwrap();
    let pandoc = tools.join("pandoc");
    fs::write(
        &pandoc,
        r#"#!/bin/sh
set -eu
cp "$1" "$CAPTURE_PDF_STAGE/catalog-case.md"
cp ./*.png "$CAPTURE_PDF_STAGE/"
for arg do last="$arg"; done
printf '<html>fixture</html>' > "$last"
"#,
    )
    .unwrap();
    let chrome = tools.join("chrome");
    fs::write(
        &chrome,
        r#"#!/bin/sh
set -eu
for arg do
  case "$arg" in --print-to-pdf=*) printf 'fixture pdf' > "${arg#--print-to-pdf=}" ;; esac
done
"#,
    )
    .unwrap();
    for tool in [&pandoc, &chrome] {
        fs::set_permissions(tool, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let pdf_stage = scratch.0.join("pdf-stage");
    fs::create_dir(&pdf_stage).unwrap();
    let paths = std::env::join_paths(std::iter::once(tools).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    // Keep the command first; the optional media flags follow it.
    let mut args = vec!["catalog".into()];
    if pdf {
        args.extend([
            "--pdf".into(),
            "--pdf-out".into(),
            scratch.0.join("pdf").into_os_string(),
        ]);
    }
    let out = Command::new(xtask_bin())
        .args(args)
        .current_dir(&root)
        .env("PATH", paths)
        .env("CAMCRAFT_CHROME", &chrome)
        .env("CAPTURE_PDF_STAGE", &pdf_stage)
        .env("CARGO_MANIFEST_DIR", root.join("xtask"))
        .env("CARGO", &renderer)
        .env("CAPTURE_RENDERER_OUT", &capture)
        .args([
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
    for name in ["0-blank", "1-Face1", "2-2D-Pocket1", "3-Drill1"] {
        let image = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["show", &format!("pr-screenshots:catalog/2d/first-part/{name}.png")])
            .output()
            .unwrap();
        assert!(image.status.success(), "missing operation image {name}");
        assert_eq!(image.stdout, name.as_bytes());
    }
    if pdf {
        let md = fs::read_to_string(pdf_stage.join("catalog-case.md")).unwrap();
        let mut previous = 0;
        for (name, bytes) in [
            ("0-blank", "0-blank"),
            ("1-Face1", "1-Face1"),
            ("2-2D-Pocket1", "2-2D-Pocket1"),
            ("3-Drill1", "3-Drill1"),
            ("first-part", "fixture image"),
        ] {
            let reference = format!("(<{name}.png>)");
            let position = md
                .find(&reference)
                .unwrap_or_else(|| panic!("missing PDF image {reference}: {md}"));
            assert!(
                position >= previous,
                "PDF images must have stable filename order"
            );
            previous = position;
            assert_eq!(
                fs::read(pdf_stage.join(format!("{name}.png"))).unwrap(),
                bytes.as_bytes()
            );
        }
        assert_eq!(
            fs::read(scratch.0.join("pdf/2d/first-part.pdf")).unwrap(),
            b"fixture pdf"
        );
    }
    let published = Command::new("git")
        .arg("-C").arg(&remote)
        .args(["ls-tree", "-r", "--name-only", "pr-screenshots"])
        .output().unwrap();
    assert!(published.status.success(), "images branch must be published: {published:?}");
    let paths = String::from_utf8(published.stdout).unwrap();
    assert_eq!(paths.lines().count(), 5);
    assert!(paths.lines().all(|p| p.starts_with("catalog/2d/first-part/") && p.ends_with(".png")));
}
