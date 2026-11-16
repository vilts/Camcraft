//! Tests for `process_bump` (#1153): the bump is +1, one integer only, and the file is rewritten whole.

use crate::process_bump::{bumped, run};

#[test]
fn bump_adds_one_and_ends_with_a_newline() {
    assert_eq!(bumped("62\n").as_deref(), Ok("63\n"));
    assert_eq!(bumped("  9 \r\n").as_deref(), Ok("10\n"));
    assert_eq!(bumped("\u{feff}0").as_deref(), Ok("1\n"));
}

#[test]
fn bump_refuses_anything_but_one_integer() {
    for bad in ["", "abc", "62 63", "-1", "6.2"] {
        assert!(bumped(bad).is_err(), "{bad:?} must be refused");
    }
    assert!(bumped(&u64::MAX.to_string()).is_err());
}

#[test]
fn run_rewrites_the_file_and_refuses_arguments() {
    let dir = std::env::temp_dir().join(format!("camcraft-process-bump-{}", std::process::id()));
    let claude = dir.join(".claude");
    std::fs::create_dir_all(&claude).expect("temp dir");
    std::fs::write(claude.join("PROCESS_VERSION"), "41\n").expect("seed");
    assert_eq!(run(&dir, &[]), Ok(0));
    assert_eq!(std::fs::read_to_string(claude.join("PROCESS_VERSION")).expect("read"), "42\n");
    assert!(run(&dir, &["--x".into()]).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
