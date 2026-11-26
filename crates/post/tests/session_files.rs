//! Domain-owned session files, from our own storage contract (#3160).
//! Gate map (#3160): covers camcraft_post::session_files.

use camcraft_post::session_files;

#[test]
fn pure_session_readers_are_available_without_the_app() {
    let text = "function onOpen() {}";
    session_files::add_post("domain-3160.cps", text).expect("valid domain post");
    session_files::add_post("domain-3160.cps", text).expect("identical text is idempotent");
    assert_eq!(
        session_files::read_post("domain-3160.cps").expect("session reader"),
        text
    );
    let why = session_files::add_post("domain-3160.cps", "function onOpen() { } // other")
        .expect_err("existing name is immutable");
    assert!(why.contains("domain-3160.cps"));
    assert_eq!(
        session_files::read_post("domain-3160.cps").expect("unchanged reader"),
        text
    );
}
