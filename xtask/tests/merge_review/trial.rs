//! Provenance: #3459's text-clean merge with a missing struct field; real Git and Cargo, only gh is a fixture.
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Repo(PathBuf);
impl Repo {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "camcraft-merge-trial-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let r = Self(dir);
        r.git(&["init", "-b", "main"]);
        r.git(&["config", "user.name", "Trial test"]);
        r.git(&["config", "user.email", "trial@localhost"]);
        r.write(
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/tiny\", \"crates/unrelated\"]\nresolver = \"2\"\n",
        );
        r.write(
            "crates/unrelated/Cargo.toml",
            "[package]\nname = \"unrelated\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        r.write(
            "crates/unrelated/src/lib.rs",
            "compile_error!(\"must not check an untouched package\");\n",
        );
        r.write(
            "crates/tiny/Cargo.toml",
            "[package]\nname = \"tiny\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        r.write(
            "crates/tiny/src/lib.rs",
            "pub struct Op { pub depth: u32 }\npub fn old() -> Op { Op { depth: 1 } }\n",
        );
        r.write(".gitignore", "target/\n.claude/\n");
        r.commit("base");
        r.git(&["remote", "add", "origin", r.0.to_str().unwrap()]);
        r.git(&["switch", "-c", "field"]);
        r.write(
            "crates/tiny/src/lib.rs",
            "pub struct Op { pub depth: u32, pub sharp_corners: bool }\npub fn old() -> Op { Op { depth: 1, sharp_corners: false } }\n",
        );
        r.commit("add field and update existing constructor");
        r.git(&["update-ref", "refs/pull/1/head", "HEAD"]);
        r.git(&["switch", "-c", "constructor", "main"]);
        r.write(
            "crates/tiny/tests/new.rs",
            "use tiny::Op;\n#[test]\nfn construct() { let _op = Op { depth: 2 }; }\n",
        );
        r.commit("add constructor in a separate file");
        r.git(&["update-ref", "refs/pull/2/head", "HEAD"]);
        r.git(&["switch", "main"]);
        r.git(&[
            "fetch",
            "--quiet",
            "origin",
            "main",
            "+refs/pull/1/head:refs/merge-plan/1",
            "+refs/pull/2/head:refs/merge-plan/2",
        ]);
        r
    }
    fn write(&self, path: &str, text: &str) {
        let p = self.0.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    fn git(&self, args: &[&str]) -> String {
        let o = Command::new("git").current_dir(&self.0).args(args).output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8(o.stdout).unwrap().trim().to_string()
    }
    fn commit(&self, message: &str) {
        self.git(&["add", "."]);
        self.git(&["commit", "-m", message]);
    }
    fn run(&self, check: bool) -> Output {
        self.run_prs(check, &["1", "2"])
    }
    fn run_prs(&self, check: bool, prs: &[&str]) -> Output {
        let bin = self.0.join(".claude/bin");
        fs::create_dir_all(&bin).unwrap();
        let gh = bin.join("gh");
        fs::write(&gh, "#!/bin/sh\ncase \"$1 $2\" in\n 'pr list') printf '1\\tfield\\tmain\\tmerge-ready\\tcrates/tiny/src/lib.rs\\tMERGEABLE\\tAdd field\\n2\\tconstructor\\tmain\\tmerge-ready\\tcrates/tiny/tests/new.rs\\tMERGEABLE\\tNew constructor\\n3\\tfollow\\tconstructor\\tmerge-ready\\tdocs/follow.md\\tMERGEABLE\\tFollow constructor\\n';;\n 'api '*) exit 0;;\n *) exit 1;;\nesac\n").unwrap();
        fs::set_permissions(gh, fs::Permissions::from_mode(0o755)).unwrap();
        let mut paths = vec![bin];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_xtask"));
        cmd.current_dir(&self.0)
            .arg("merge-plan")
            .args(prs)
            .args(["--repo", "test/trial", "--json"])
            .env("CARGO_MANIFEST_DIR", self.0.join("xtask"))
            .env("PATH", std::env::join_paths(paths).unwrap());
        if check {
            cmd.arg("--check");
        }
        cmd.output().unwrap()
    }
    fn assert_preserved(&self, before: &str) {
        assert_eq!(self.git(&["status", "--porcelain"]), before);
        assert_eq!(self.git(&["branch", "--show-current"]), "main");
        assert_eq!(self.git(&["worktree", "list", "--porcelain"]).matches("worktree ").count(), 1);
        let trial = self.0.join(".claude/worktrees");
        if trial.exists() {
            assert_eq!(fs::read_dir(trial).unwrap().count(), 0);
        }
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn text_clean_missing_field_is_excluded_before_real_merge() {
    let r = Repo::new();
    let text = r.run(false);
    assert!(text.status.success(), "{}", String::from_utf8_lossy(&text.stderr));
    assert!(String::from_utf8_lossy(&text.stdout).contains("\"wave1\":[{\"number\":2"));
    // Preserve an unrelated caller edit and untracked file, not merely a clean checkout.
    r.write(
        "crates/tiny/src/lib.rs",
        "pub struct Op { pub depth: u32 }\npub fn old() -> Op { Op { depth: 7 } }\n",
    );
    r.write("notes.txt", "caller-owned untracked file\n");
    let before = r.git(&["status", "--porcelain"]);
    let output = r.run(true);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
    assert!(
        stderr.contains("E0063") && stderr.contains("sharp_corners"),
        "the trial must find the actual compile error: {stderr}"
    );
    assert!(
        stderr.contains("#2") && stderr.contains("crates/tiny/tests/new.rs"),
        "the diagnostic must name the owning PR: {stderr}"
    );
    assert!(!output.status.success(), "a rejected PR must give a failing exit status");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.split("\"wave2\"").next().unwrap().contains("\"number\":2"),
        "the broken PR must leave wave 1: {stdout}"
    );
    assert!(stdout.contains("\"excluded\""), "excluded PRs must be in the JSON result: {stdout}");
    r.assert_preserved(&before);
    let mut dirs = vec![r.0.join("target")];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::metadata(&path).unwrap();
            if meta.is_dir() {
                dirs.push(path);
            } else {
                assert_eq!(
                    meta.permissions().mode() & 0o111,
                    0,
                    "check must not link a test executable: {path:?}"
                );
            }
        }
    }
}

#[test]
fn individual_heads_compile_and_leave_no_trial_worktree() {
    let r = Repo::new();
    let before = r.git(&["status", "--porcelain"]);
    let head = r.git(&["rev-parse", "HEAD"]);
    for pr in ["1", "2"] {
        let out = r.run_prs(true, &[pr]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        println!("head #{pr}: {}\n{stderr}", String::from_utf8_lossy(&out.stdout));
        assert!(out.status.success(), "{stderr}");
        assert!(String::from_utf8_lossy(&out.stdout).contains("\"checked\":true,\"excluded\":[]"));
        assert!(stderr.contains("cargo check --tests -p tiny"));
        assert!(!stderr.contains("cargo check --tests -p unrelated"));
        r.assert_preserved(&before);
        assert_eq!(r.git(&["rev-parse", "HEAD"]), head);
    }
}

#[test]
fn a_stacked_child_of_an_excluded_owner_is_not_proposed() {
    let r = Repo::new();
    r.git(&["switch", "-c", "follow", "constructor"]);
    r.write("docs/follow.md", "Uses the new constructor.\n");
    r.commit("follow the constructor");
    r.git(&["update-ref", "refs/pull/3/head", "HEAD"]);
    r.git(&["switch", "main"]);
    let out = r.run_prs(true, &["1", "2", "3"]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8_lossy(&out.stdout);
    let proposed = text.split("\"excluded\"").next().unwrap();
    assert!(!proposed.contains("\"number\":2") && !proposed.contains("\"number\":3"), "{text}");
    assert!(text.contains("parent excluded by trial check"), "{text}");
    r.assert_preserved("");
}

#[test]
fn metadata_failure_cleans_the_trial_and_does_not_emit_a_plan() {
    let r = Repo::new();
    r.git(&["switch", "field"]);
    r.write("Cargo.toml", "not valid TOML [\n");
    r.commit("invalid trial manifest");
    r.git(&["update-ref", "refs/pull/1/head", "HEAD"]);
    r.git(&["switch", "main"]);
    let before = r.git(&["status", "--porcelain"]);
    let head = r.git(&["rev-parse", "HEAD"]);
    let out = r.run_prs(true, &["1"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("metadata"));
    r.assert_preserved(&before);
    assert_eq!(r.git(&["rev-parse", "HEAD"]), head);
}
