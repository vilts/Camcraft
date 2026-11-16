//! Where the discovered theme files live, resolved at RUN time (M13.3).
//!
//! The switcher's file rows used to come from a compile-time path into
//! the source tree (`CARGO_MANIFEST_DIR/../../assets/themes`): a shipped
//! `camcraft-ui.exe` only found themes on the machine that built it, and
//! the portable zip's `assets/themes/` beside the exe was never read.
//! The lookup is now a candidate walk, first existing directory wins:
//!
//! 1. `CAMCRAFT_THEMES` - an absolute directory the user names;
//! 2. `<exe dir>/assets/themes` - the zip layout;
//! 3. `<app>/Contents/Resources/assets/themes` - a macOS app bundle;
//! 4. `<exe dir>/../../assets/themes` - a dev run from `target/<profile>/`;
//! 5. `<this crate>/../../assets/themes` - the old compile-time path, a
//!    dev fallback LAST (the headless suites run from `target/*/deps/`).
//!
//! The walk is pure: the inputs (env value, exe path, manifest dir) and
//! the directory probe are parameters, so the order is testable without
//! the real process. Nothing found is an `Err` naming the fact - the
//! built-in themes stand and the status line says so once; never a panic,
//! never a silently empty picker.
//!
//! Provenance: our own resolver, specced from the ROADMAP's M13.3 release
//! note (the zip ships `assets/themes` beside the exe) and the `std::env`
//! documentation for `current_exe` / `var_os`.

use std::path::{Path, PathBuf};

/// The override: an absolute directory of `camcraft-theme/1` files.
pub const ENV_VAR: &str = "CAMCRAFT_THEMES";

/// The status-line sentence when no candidate exists.
pub const NOT_FOUND: &str = "no theme folder found beside the exe - the built-in themes stand";

/// The theme-directory candidates in precedence order (see the module
/// doc). A relative `env` is dropped: the override names an absolute
/// directory, and a relative one would mean "wherever the shell happened
/// to be", which is not a place a user can predict.
#[must_use]
pub fn candidates(env: Option<&Path>, exe: Option<&Path>, manifest_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(5);
    if let Some(dir) = env.filter(|p| p.is_absolute() && !p.as_os_str().is_empty()) {
        out.push(dir.to_path_buf());
    }
    if let Some(exe_dir) = exe.and_then(Path::parent) {
        out.push(exe_dir.join("assets").join("themes"));
        if exe_dir.file_name().is_some_and(|name| name == std::ffi::OsStr::new("MacOS"))
            && let Some(contents) = exe_dir.parent()
        {
            out.push(contents.join("Resources").join("assets").join("themes"));
        }
        if let Some(repo) = exe_dir.parent().and_then(Path::parent) {
            out.push(repo.join("assets").join("themes"));
        }
    }
    if let Some(repo) = manifest_dir.and_then(Path::parent).and_then(Path::parent) {
        out.push(repo.join("assets").join("themes"));
    }
    out
}

/// The first candidate `is_dir` accepts, or the sentence for none. An
/// override that is set but not a directory is named in that sentence:
/// a typo in `CAMCRAFT_THEMES` must not read as "no themes shipped".
pub fn resolve(
    env: Option<&Path>,
    exe: Option<&Path>,
    manifest_dir: Option<&Path>,
    is_dir: impl Fn(&Path) -> bool,
) -> Result<PathBuf, String> {
    let found = candidates(env, exe, manifest_dir).into_iter().find(|p| is_dir(p));
    match found {
        Some(dir) => Ok(dir),
        None => match env {
            Some(set) => Err(format!(
                "{ENV_VAR}={} is not a directory; {NOT_FOUND}",
                set.display()
            )),
            None => Err(NOT_FOUND.to_owned()),
        },
    }
}

/// The status line the app starts with. A browser build has no exe and
/// no theme folder to look beside (#31): its built-in themes are all it
/// ships, so a missing folder is not news there and it starts "ready".
#[must_use]
pub fn startup_status(found: &Result<PathBuf, String>, in_browser: bool) -> String {
    match found {
        Err(why) if !in_browser => why.clone(),
        _ => String::from("ready"),
    }
}

/// [`resolve`] wired to the running process: the real environment, the
/// real exe path, this crate's manifest as the dev fallback, and the
/// filesystem as the probe.
pub fn from_process() -> Result<PathBuf, String> {
    let env = std::env::var_os(ENV_VAR).map(PathBuf::from);
    let exe = std::env::current_exe().ok();
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    resolve(env.as_deref(), exe.as_deref(), Some(manifest), Path::is_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An absolute root on either platform: `/x` is not absolute on
    /// Windows (no drive), and the override law drops a relative path.
    #[cfg(windows)]
    const ROOT: &str = "C:";
    #[cfg(not(windows))]
    const ROOT: &str = "";

    fn p(s: &str) -> PathBuf {
        PathBuf::from(format!("{ROOT}{s}"))
    }

    /// A probe that accepts exactly the listed directories.
    fn dirs(accept: &[&str]) -> impl Fn(&Path) -> bool {
        let set: Vec<PathBuf> = accept.iter().map(|s| p(s)).collect();
        move |q: &Path| set.iter().any(|d| d == q)
    }

    const EXE: &str = "/opt/cc/target/release/camcraft-ui.exe";
    const MANIFEST: &str = "/src/camcraft/crates/ui";

    #[test]
    fn the_order_is_env_exe_repo_manifest() {
        let got = candidates(Some(&p("/themes")), Some(&p(EXE)), Some(&p(MANIFEST)));
        assert_eq!(
            got,
            vec![
                p("/themes"),
                p("/opt/cc/target/release/assets/themes"),
                p("/opt/cc/assets/themes"),
                p("/src/camcraft/assets/themes"),
            ]
        );
    }

    #[test]
    fn env_wins_when_it_is_a_directory() {
        let all = dirs(&["/themes", "/opt/cc/target/release/assets/themes", "/opt/cc/assets/themes"]);
        let got = resolve(Some(&p("/themes")), Some(&p(EXE)), Some(&p(MANIFEST)), all);
        assert_eq!(got, Ok(p("/themes")));
    }

    #[test]
    fn a_relative_env_is_dropped_not_probed() {
        let got = candidates(Some(Path::new("themes")), None, None);
        assert!(got.is_empty(), "{got:?}");
    }

    #[test]
    fn exe_beside_layout_wins_over_the_repo_layout() {
        let both = dirs(&["/opt/cc/target/release/assets/themes", "/opt/cc/assets/themes"]);
        let got = resolve(None, Some(&p(EXE)), Some(&p(MANIFEST)), both);
        assert_eq!(got, Ok(p("/opt/cc/target/release/assets/themes")));
    }

    #[test]
    fn macos_bundle_resources_are_found_beside_the_executable() {
        let exe = p("/Applications/CamCraft.app/Contents/MacOS/camcraft-ui");
        let themes = "/Applications/CamCraft.app/Contents/Resources/assets/themes";
        let got = resolve(None, Some(&exe), None, dirs(&[themes]));
        assert_eq!(got, Ok(p(themes)));
    }

    #[test]
    fn the_repo_layout_is_found_from_a_target_release_exe() {
        let repo_only = dirs(&["/opt/cc/assets/themes"]);
        let got = resolve(None, Some(&p(EXE)), Some(&p(MANIFEST)), repo_only);
        assert_eq!(got, Ok(p("/opt/cc/assets/themes")));
    }

    #[test]
    fn the_manifest_path_is_the_last_resort() {
        let manifest_only = dirs(&["/src/camcraft/assets/themes"]);
        let got = resolve(None, Some(&p(EXE)), Some(&p(MANIFEST)), manifest_only);
        assert_eq!(got, Ok(p("/src/camcraft/assets/themes")));
    }

    #[test]
    fn nothing_found_names_the_fact() {
        let none = dirs(&[]);
        let got = resolve(None, Some(&p(EXE)), Some(&p(MANIFEST)), none);
        assert_eq!(got, Err(NOT_FOUND.to_owned()));
        // No exe path at all (the OS refused `current_exe`): still a
        // sentence, never a panic.
        let got = resolve(None, None, None, dirs(&[]));
        assert_eq!(got, Err(NOT_FOUND.to_owned()));
    }

    #[test]
    fn a_bad_env_override_is_named_in_the_refusal() {
        let got = resolve(Some(&p("/nope")), Some(&p(EXE)), None, dirs(&[]));
        let why = got.expect_err("a missing override must not resolve");
        assert!(why.starts_with("CAMCRAFT_THEMES="), "{why}");
        assert!(why.contains("nope is not a directory"), "{why}");
        assert!(why.contains(NOT_FOUND), "{why}");
    }

    /// The real wiring: the headless suites run from `target/*/deps/`,
    /// where only the manifest fallback reaches the repo's folder.
    #[test]
    fn the_dev_run_finds_the_shipped_folder() {
        let dir = from_process().unwrap_or_else(|why| panic!("dev run must find assets/themes: {why}"));
        assert!(dir.join("teal-night.json").is_file(), "{}", dir.display());
    }

    #[test]
    fn the_browser_build_never_says_a_theme_folder_is_missing_beside_an_exe() {
        let missing: Result<PathBuf, String> = Err(NOT_FOUND.to_owned());
        assert_eq!(startup_status(&missing, false), NOT_FOUND, "the desktop app says it once");
        assert_eq!(startup_status(&missing, true), "ready", "a browser has no exe to look beside");
        assert_eq!(startup_status(&Ok(p("/themes")), false), "ready");
        assert_eq!(startup_status(&Ok(p("/themes")), true), "ready");
    }
}
