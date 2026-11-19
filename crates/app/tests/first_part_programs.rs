//! Provenance: our own shipped examples, processed through our public CLI.
//! Gate map: covers app::cli, app::project, app::posting, app::operation.
//!
//! Regenerate each program with `camcraft-cli process examples/<name>.camproj
//! -o examples/<name>.nc` after reviewing an intentional output change.

use std::path::Path;

fn assert_example_program(name: &str) {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = repo.join(format!("examples/{name}.camproj"));
    let golden = repo.join(format!("examples/{name}.nc"));
    let output = std::env::temp_dir().join(format!(
        "camcraft-{name}-program-pin-{}.nc",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    let status = camcraft_app::cli::run(&[
        "process".into(),
        project.display().to_string(),
        "-o".into(),
        output.display().to_string(),
    ]);
    assert_eq!(status, 0, "{name} must generate, post and verify");
    let actual = std::fs::read_to_string(&output).expect("the CLI posted a program");
    let expected = std::fs::read_to_string(&golden)
        .unwrap_or_else(|e| panic!("{} must ship beside its project: {e}", golden.display()));
    assert!(actual == expected, "{} is stale; compare {} and regenerate it", golden.display(), output.display());
    std::fs::remove_file(output).expect("remove the generated program");
}

#[test]
fn first_part_posts_the_shipped_program() {
    assert_example_program("first-part");
}

#[test]
fn first_router_part_posts_the_shipped_program() {
    assert_example_program("first-router-part");
}
