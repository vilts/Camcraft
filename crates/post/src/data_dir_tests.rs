//! Pins of the run-time data directory lookup (#2282).

use super::*;

#[cfg(windows)]
const ROOT: &str = "C:";
#[cfg(not(windows))]
const ROOT: &str = "";

fn p(s: &str) -> PathBuf {
    PathBuf::from(format!("{ROOT}{s}"))
}

fn dirs(accept: &[&str]) -> impl Fn(&Path) -> bool {
    let set: Vec<PathBuf> = accept.iter().map(|s| p(s)).collect();
    move |q: &Path| set.iter().any(|d| d == q)
}

const EXE: &str = "/opt/cc/target/release/camcraft-ui.exe";
const FINGERPRINT: &str = "/opt/cc/target/release/.fingerprint";
const MANIFEST: &str = "/src/camcraft/crates/post";

/// A fresh, empty scratch directory for one test.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-data-dir-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn the_order_is_exe_dev_manifest_in_a_cargo_tree() {
    let got = candidates("posts", Some(&p(EXE)), Some(&p(MANIFEST)), &dirs(&[FINGERPRINT]));
    assert_eq!(
        got,
        vec![p("/opt/cc/target/release/assets/posts"), p("/opt/cc/assets/posts"), p("/src/camcraft/assets/posts")]
    );
    let test_bin = p("/opt/cc/target/debug/deps/posting-1234.exe");
    let got = candidates("posts", Some(&test_bin), Some(&p(MANIFEST)), &dirs(&["/opt/cc/target/debug/.fingerprint"]));
    assert_eq!(got.last(), Some(&p("/src/camcraft/assets/posts")), "a test binary in deps/ finds the source tree");
}

/// #2316: the new cargo build-dir layout (the post-merge runner's cargo)
/// puts a test exe in `target/<profile>/build/<pkg>/<hash>/out` and has
/// no `target/<profile>/.fingerprint`. The test exes and the uplifted
/// binaries still find the repository assets; nothing else is let in.
#[test]
fn the_new_cargo_build_dir_layout_finds_the_repository_assets() {
    let tree = dirs(&["/opt/cc/target/debug/build/camcraft-post", "/opt/cc/assets/posts"]);
    for exe in [
        "/opt/cc/target/debug/build/camcraft-app/022e328e1d2fcaa4/out/posting-022e328e1d2fcaa4",
        "/opt/cc/target/debug/build/camcraft-post/4d4a6b2fbedf41a7/out/camcraft_post-4d4a6b2fbedf41a7",
        "/opt/cc/target/debug/camcraft-cli",
    ] {
        let exe = p(exe);
        let got = candidates("posts", Some(&exe), Some(&p(MANIFEST)), &tree);
        assert_eq!(got[1..], [p("/opt/cc/assets/posts"), p("/src/camcraft/assets/posts")], "{}", exe.display());
        assert_eq!(resolve("posts", Some(&exe), Some(&p(MANIFEST)), &tree), Ok(p("/opt/cc/assets/posts")));
    }
    // An exe in an `out` folder that is not a cargo unit folder stays an
    // installed exe: only the folder beside it.
    let stray = p("/opt/app/build/x/y/out/camcraft-ui");
    let elsewhere = dirs(&["/opt/assets/posts", "/src/camcraft/assets/posts"]);
    assert_eq!(candidates("posts", Some(&stray), Some(&p(MANIFEST)), &elsewhere).len(), 1);
    let why = resolve("posts", Some(&stray), Some(&p(MANIFEST)), elsewhere).expect_err("not a cargo tree");
    assert!(why.starts_with("no posts folder found"), "{why}");
}

/// Review of #2293: an installed exe never falls back to the build
/// machine's checkout or to `<exe>/../../assets` (`/usr/assets`, `C:/assets`).
#[test]
fn an_installed_exe_without_assets_is_refused_not_redirected() {
    for exe in ["/usr/lib/camcraft/camcraft-ui", "/Program Files/CamCraft/camcraft-ui.exe"] {
        let exe = p(exe);
        assert_eq!(candidates("posts", Some(&exe), Some(&p(MANIFEST)), &|_| false).len(), 1, "{}", exe.display());
        let elsewhere = dirs(&["/usr/assets/posts", "/assets/posts", "/src/camcraft/assets/posts"]);
        let why = resolve("posts", Some(&exe), Some(&p(MANIFEST)), elsewhere).expect_err("no assets beside the exe");
        assert!(why.starts_with("no posts folder found"), "{why}");
        assert!(!why.contains("src"), "{why}");
    }
}

#[test]
fn beside_the_exe_wins_over_the_source_tree() {
    let both = dirs(&["/opt/cc/target/release/assets/machines", "/src/camcraft/assets/machines", FINGERPRINT]);
    let got = resolve("machines", Some(&p(EXE)), Some(&p(MANIFEST)), both);
    assert_eq!(got, Ok(p("/opt/cc/target/release/assets/machines")));
}

#[test]
fn the_macos_bundle_resources_are_found() {
    let exe = p("/Applications/CamCraft.app/Contents/MacOS/camcraft-ui");
    let want = "/Applications/CamCraft.app/Contents/Resources/assets/posts";
    assert_eq!(resolve("posts", Some(&exe), None, dirs(&[want])), Ok(p(want)));
}

/// Done means (#2282): a test pins the error for a missing directory.
#[test]
fn a_missing_directory_is_a_named_error_not_a_crash() {
    let why = resolve("posts", Some(&p(EXE)), None, dirs(&[])).expect_err("nothing exists");
    assert!(why.starts_with("no posts folder found"), "{why}");
    assert!(why.contains("assets/posts directory beside the executable"), "{why}");
    let first = candidates("posts", Some(&p(EXE)), None, &|_| false)[0].display().to_string();
    assert!(why.contains(&first), "{why}");
    let files = DataFiles::new(Err(why.clone()));
    assert_eq!(files.read("fanuc.cps").map(|_| ()), Err(why), "a read names the missing directory");
    let none = resolve("machines", None, None, dirs(&[])).expect_err("no exe path");
    assert!(none.contains("nowhere"), "{none}");
}

#[test]
fn a_missing_file_names_the_file_and_the_directory() {
    let dir = scratch("missing-file");
    let files = DataFiles::new(Ok(dir.clone()));
    let why = files.read("gone.cps").expect_err("no such file");
    assert!(why.starts_with("gone.cps in "), "{why}");
    assert!(why.contains(&dir.display().to_string()), "{why}");
    let _ = std::fs::remove_dir_all(dir);
}

/// Done means (#2282): an edited file on disk changes what is loaded,
/// and the read is cached (one read per file, never per frame).
#[test]
fn an_edited_file_on_disk_is_what_loads_and_the_read_is_cached() {
    let dir = scratch("edited");
    std::fs::write(dir.join("mine.cps"), "// edited by the user\n").expect("write");
    let files = DataFiles::new(Ok(dir.clone()));
    assert_eq!(&*files.read("mine.cps").expect("reads"), "// edited by the user\n");
    std::fs::write(dir.join("mine.cps"), "// changed again\n").expect("rewrite");
    assert_eq!(&*files.read("mine.cps").expect("cached"), "// edited by the user\n", "a second read comes from the cache");
    assert_eq!(&*DataFiles::new(Ok(dir.clone())).read("mine.cps").expect("fresh"), "// changed again\n");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_file_that_is_not_text_is_refused_by_name() {
    let dir = scratch("binary");
    std::fs::write(dir.join("bad.mch"), [0xff_u8, 0xfe, 0x00]).expect("write");
    let why = DataFiles::new(Ok(dir.clone())).read("bad.mch").expect_err("not UTF-8");
    assert!(why.contains("bad.mch in") && why.contains("not UTF-8"), "{why}");
    let _ = std::fs::remove_dir_all(dir);
}

/// The dev run and the tests find the repository `assets` directory.
#[test]
fn the_dev_run_finds_the_repository_assets() {
    for (kind, file) in [("posts", "fanuc.cps"), ("machines", "vmc.mch")] {
        let files = DataFiles::from_process(kind);
        let dir = files.dir().unwrap_or_else(|why| panic!("{why}"));
        assert!(dir.join(file).is_file(), "{}", dir.display());
        assert!(!files.read(file).expect("reads").is_empty());
    }
}

/// The 4 MiB hostile-input cap: a file of exactly `MAX_BYTES` reads, one
/// byte more is refused by name (zero bytes are valid UTF-8, so only the
/// cap can refuse it).
#[test]
fn a_file_over_the_size_cap_is_refused_by_name() {
    let dir = scratch("cap");
    let at = std::fs::File::create(dir.join("at.cps")).expect("create");
    at.set_len(MAX_BYTES).expect("size");
    let over = std::fs::File::create(dir.join("over.cps")).expect("create");
    over.set_len(MAX_BYTES + 1).expect("size");
    drop((at, over));
    assert_eq!(read_file(&dir, "at.cps").map(|t| t.len() as u64), Ok(MAX_BYTES));
    let why = read_file(&dir, "over.cps").expect_err("over the cap");
    assert!(why.starts_with("over.cps in ") && why.contains("byte limit - refused"), "{why}");
    let _ = std::fs::remove_dir_all(dir);
}
