//! Pins of #2380: the machine list follows its folder - at the start, and
//! again after a refresh. An added file has an entry, a removed file has
//! none and gives no error, a malformed file is listed with its refusals,
//! and a name that has no file is still refused by name. The scan itself
//! is pinned in `crates/post/tests/folder_scan.rs`.
//! Gate map (#2380): covers camcraft_app::posting, camcraft_app::machine::library.

use camcraft_app::machine::library::{self, MachineSource};
use camcraft_app::posting::{self, scan_folder};
use std::path::{Path, PathBuf};

fn assets(kind: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets").join(kind)
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-2380-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch folder");
    dir
}

fn copy(kind: &str, file: &str, to: &Path, as_name: &str) {
    std::fs::copy(assets(kind).join(file), to.join(as_name)).expect("copy a shipped file");
}

/// Machines: a folder with three library files and two others; add one,
/// remove a different one, rescan - the list follows, with no error.
#[test]
fn the_machine_list_follows_its_folder() {
    let dir = scratch("mch");
    for f in ["vmc.mch", "tilthead.mch", "lathe.mch"] {
        copy("machines", f, &dir, f);
    }
    copy("machines", "vmc.mch", &dir, "mine.mch");
    std::fs::write(dir.join("broken.mch"), "{ this is not a machine").expect("write");
    std::fs::create_dir(dir.join("a_folder.mch")).expect("a folder named like a file");

    let at_start = scan_folder(&dir, "mch", posting::MACHINES, &[posting::DEFAULT_MCH_FILE]);
    assert_eq!(at_start.present, ["VMC 3-axis", "Tilt-head 5-axis", "Lathe"]);
    let extra: Vec<&str> = at_start.extra.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(extra, ["broken", "mine"], "other files by stem; the folder is skipped");
    let names = |s: &posting::FolderScan| library::shipped_entries(s).iter().map(|e| e.name.clone()).collect::<Vec<_>>();
    let listed = names(&at_start);
    assert!(listed.iter().any(|n| n == "Lathe"), "{listed:?}");
    let broken = library::shipped_entries(&at_start)
        .into_iter()
        .find(|e| matches!(&e.source, MachineSource::File(p) if p.ends_with("broken.mch")))
        .expect("the malformed file is listed, not skipped");
    assert!(!broken.is_valid(), "and it carries its refusals");

    // Add one file, remove a different one, read the folder again.
    copy("machines", "tormach-770mx.mch", &dir, "tormach-770mx.mch");
    copy("machines", "vmc.mch", &dir, "added.mch");
    copy("machines", "vmc.mch", &dir, "3axis-vmc.mch");
    std::fs::remove_file(dir.join("lathe.mch")).expect("remove");
    let refreshed = scan_folder(&dir, "mch", posting::MACHINES, &[posting::DEFAULT_MCH_FILE]);
    assert_eq!(refreshed.present, ["VMC 3-axis", "Tilt-head 5-axis", "Tormach 770MX"]);
    assert!(refreshed.problem.is_none());
    let listed = names(&refreshed);
    assert!(!listed.iter().any(|n| n == "Lathe"), "the removed file has no entry: {listed:?}");
    assert!(listed.iter().any(|n| n == "Tormach 770MX"), "{listed:?}");
    assert!(refreshed.extra.iter().any(|(n, _)| n == "added"), "{:?}", refreshed.extra);
    assert!(!refreshed.extra.iter().any(|(n, _)| n == "3axis-vmc"), "the default machine file is not a list entry");
    assert!(library::shipped_entries(&refreshed).iter().all(|e| e.refusals.iter().all(|r| !r.rule.contains("cannot be read") && !r.rule.contains("os error"))), "no file-not-found entry");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The process lists read the real folders, a refresh reads them again, and
/// a name without a file is still refused by name.
#[test]
fn the_process_lists_read_the_shipped_folders() {
    posting::rescan();
    assert_eq!(posting::machine_scan().present.len(), posting::MACHINES.len());
    assert_eq!(posting::post_scan().present.len(), posting::BUILTINS.len());
    let why = posting::builtin_machine_text("Gone Machine").expect_err("not shipped");
    assert!(why.contains("Gone Machine"), "{why}");
    let why = posting::builtin_post_text("Gone Post").expect_err("not shipped");
    assert!(why.contains("Gone Post"), "{why}");
}
