//! The Grbl 1.1 dialect law (#2481), read on every `grbl.cps` golden
//! program (the recorded-stream suite and the app's conformance goldens):
//! each line fits Grbl 1.1's 80-byte line buffer (79 characters and the
//! terminator), and no word outside Grbl 1.1's documented G-code and
//! M-code set is written - no canned cycle, no M6, no tool length word
//! with H, no cutter compensation, no O program number.
//!
//! Gate map (M4.4): covers assets/posts/grbl.cps.

use std::fs;
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

/// Every `.nc` golden the suites wrote for the Grbl post.
fn goldens() -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    for dir in ["crates/post/tests/recorded/golden/grbl", "crates/app/tests/golden/nc/grbl"] {
        let Ok(rd) = fs::read_dir(repo(dir)) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "nc") {
                let text = fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                out.push((p, text));
            }
        }
    }
    out
}

/// The words of a line outside its comments, upper case.
fn words(line: &str) -> Vec<String> {
    let mut code = String::new();
    let mut depth = 0;
    for c in line.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ';' if depth == 0 => break,
            _ if depth == 0 => code.push(c),
            _ => {}
        }
    }
    code.split_whitespace().map(|w| w.to_ascii_uppercase()).collect()
}

#[test]
fn every_grbl_golden_line_fits_the_80_byte_line_buffer() {
    let all = goldens();
    assert!(!all.is_empty(), "the Grbl goldens exist");
    for (p, text) in &all {
        for (i, line) in text.lines().enumerate() {
            assert!(line.len() <= 79, "{}:{}: {} characters: {line}", p.display(), i + 1, line.len());
        }
    }
}

#[test]
fn no_grbl_golden_writes_a_word_grbl_1_1_does_not_have() {
    // Canned cycles G73 / G81-G89 and the retract modes G98 / G99, M6,
    // G43 (with H) and G41 / G42, and an O number.
    let banned_g = ["G73", "G81", "G82", "G83", "G84", "G85", "G86", "G87", "G88", "G89", "G98", "G99", "G41", "G42", "G43"];
    for (p, text) in goldens() {
        for (i, line) in text.lines().enumerate() {
            for w in words(line) {
                let bad = banned_g.contains(&w.as_str()) || w == "M6" || w == "M06" || w.starts_with('O') || w.starts_with('H');
                assert!(!bad, "{}:{}: Grbl 1.1 has no {w}: {line}", p.display(), i + 1);
            }
        }
    }
}
