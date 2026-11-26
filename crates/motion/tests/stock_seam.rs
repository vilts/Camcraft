//! M7.3 (4/6, #587): camcraft-motion reads and cuts stock only through
//! the `Stock` / `Columns` seam, never the column engine's `Dexel`
//! directly - so retiring that engine (6/6) is one change behind the
//! seam, not a hunt through the generators. Tests may still build a
//! bare grid (as a fixture, or the baseline the seam is pinned to).
//!
//! Provenance: our own source-scan law, the same shape as
//! `origin_approach`'s; no vendor material.
//!
//! Gate map (M7.3): covers camcraft_motion::machining3d,
//! camcraft_motion::multiaxis, camcraft_motion::quality,
//! camcraft_motion::machining2d::rest_prior,
//! camcraft_motion::machining2d::rest_prior_adaptive.

#[test]
fn motion_names_no_dexel_outside_tests() {
    fn walk(dir: &std::path::Path, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n != std::ffi::OsStr::new("tests")) {
                    walk(&p, out);
                }
            } else if p.extension().is_some_and(|x| x == "rs") && p.file_name().is_some_and(|n| n != std::ffi::OsStr::new("tests.rs")) {
                let text = std::fs::read_to_string(&p).unwrap_or_default();
                for (i, line) in text.lines().enumerate() {
                    // Code only: a comment may name the engine's history.
                    let code = line.split("//").next().unwrap_or("");
                    if code.contains("Dexel") {
                        out.push(format!("{}:{}: {}", p.display(), i + 1, line.trim()));
                    }
                }
            }
        }
    }
    let mut found = Vec::new();
    walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut found);
    assert!(found.is_empty(), "read the stock through Stock / Columns, not Dexel:\n{}", found.join("\n"));
}
