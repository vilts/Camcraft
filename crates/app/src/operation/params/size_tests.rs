//! Pins #1849: the digest writer of `OpParams` stays split along its
//! operation families, every file under the AGENTS.md 400-line law.

/// Each file of the `params` module (the root and its children) holds
/// at most this many lines (AGENTS.md "Code shape": ~400 LOC).
const LIMIT: usize = 400;

#[test]
fn params_files_stay_under_the_line_law() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/operation");
    let mut files = vec![dir.join("params.rs")];
    let children = std::fs::read_dir(dir.join("params")).expect("params/ is readable");
    for entry in children {
        let path = entry.expect("params/ entry is readable").path();
        if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
    for path in files {
        let text = std::fs::read_to_string(&path).expect("params source is readable");
        let n = text.lines().count();
        assert!(n <= LIMIT, "{} has {n} lines, the law is {LIMIT}", path.display());
    }
}
