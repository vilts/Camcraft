//! Pins of #2282: the shipped post scripts and machine files are read
//! from `assets/` on disk at run time, not compiled into the executable.
//! Gate map (#2307): covers app::posting, app::posting_files.

use camcraft_app::posting;

/// Each registry post is the file on disk that `builtin_post_path` names.
#[test]
fn every_shipped_post_is_the_file_on_disk() {
    for (name, _) in posting::BUILTINS {
        let path = posting::builtin_post_path(name).unwrap_or_else(|| panic!("{name}: no path"));
        assert!(path.parent().is_some_and(|d| d.ends_with("assets/posts")), "{}", path.display());
        let disk = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(posting::builtin_post_text(name), Ok(disk.as_str()), "{name}");
    }
}

/// Each library machine reads, and the default machine reads.
#[test]
fn every_shipped_machine_reads_from_disk() {
    let all = posting::shipped_machines().expect("the machine library reads");
    assert_eq!(all.len(), posting::MACHINES.len());
    for (name, text) in all {
        assert!(!text.is_empty(), "{name}");
    }
    assert!(posting::default_mch_text().is_ok_and(|t| !t.is_empty()));
}

/// A name outside the registry is a refusal by name, not a panic.
#[test]
fn an_unknown_name_is_refused_by_name() {
    let post = posting::builtin_post_text("No Such Post").expect_err("not shipped");
    assert!(post.contains("No Such Post"), "{post}");
    let machine = posting::builtin_machine_text("No Such Machine").expect_err("not shipped");
    assert!(machine.contains("No Such Machine"), "{machine}");
    assert_eq!(posting::builtin_post_path("No Such Post"), None);
}
