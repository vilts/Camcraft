//! Pins of #2380 for the folder scan: an added file is listed, a removed
//! file is not and gives no error, a malformed file is listed and hides no
//! other file, a hidden file is no entry, and a folder that does not read
//! lists nothing and says why.
//! Gate map (#2380): covers camcraft_post::data_scan, camcraft_post::data_dir.

use camcraft_post::data_scan::scan_folder;
use std::path::{Path, PathBuf};

const POSTS: &[(&str, &str)] = &[("Fanuc", "fanuc.cps"), ("Haas", "haas.cps"), ("Mazak", "mazak.cps"), ("Heidenhain", "heidenhain.cps")];

fn assets(kind: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets").join(kind)
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-2380-post-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch folder");
    dir
}

fn copy(kind: &str, file: &str, to: &Path, as_name: &str) {
    std::fs::copy(assets(kind).join(file), to.join(as_name)).expect("copy a shipped file");
}

/// Posts: three table files and a non-post file; add two, remove one,
/// scan again - the list follows the folder.
#[test]
fn the_post_list_follows_its_folder() {
    let dir = scratch("cps");
    for f in ["fanuc.cps", "haas.cps", "mazak.cps"] {
        copy("posts", f, &dir, f);
    }
    std::fs::write(dir.join("notes.txt"), "not a post").expect("write");
    std::fs::create_dir(dir.join("a_folder.cps")).expect("a folder named like a file");
    let at_start = scan_folder(&dir, "cps", POSTS, &["3axis-vmc.mch"]);
    assert_eq!(at_start.present, ["Fanuc", "Haas", "Mazak"]);
    assert!(at_start.extra.is_empty(), "{:?}", at_start.extra);

    copy("posts", "fanuc.cps", &dir, "my-shop.cps");
    copy("posts", "fanuc.cps", &dir, "Hidden.CPS");
    std::fs::write(dir.join("broken.cps"), "function (").expect("write");
    std::fs::remove_file(dir.join("haas.cps")).expect("remove");
    let refreshed = scan_folder(&dir, "cps", POSTS, &["hidden.cps"]);
    assert_eq!(refreshed.present, ["Fanuc", "Mazak"]);
    let extra: Vec<&str> = refreshed.extra.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(extra, ["broken", "my-shop"], "the malformed script is listed and hides nothing; the hidden file is no entry");
    assert!(refreshed.extra.iter().all(|(_, p)| p.starts_with(&dir)));
    assert!(refreshed.problem.is_none() && refreshed.skipped.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A folder that does not read lists nothing and says why; it never panics.
#[test]
fn a_missing_folder_lists_nothing_and_says_why() {
    let gone = std::env::temp_dir().join(format!("camcraft-2380-gone-{}", std::process::id()));
    let scan = scan_folder(&gone, "cps", POSTS, &[]);
    assert!(scan.present.is_empty() && scan.extra.is_empty());
    assert!(scan.problem.is_some_and(|p| p.contains("camcraft-2380-gone")));
}
