//! #1617: a closed STEP solid wound inward is turned outward at import,
//! and the status line says so; a plain import says nothing more.
//! Provenance: our own tests over our own box fixture; no vendor material.
//!
//! Gate map (#1617): covers ui::state (finish_import), app::commands::import_model (notes).

use super::*;

fn box_step() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/box.step")
}

/// `box.step` with every face and bound sense reversed: closed, gap-free,
/// consistently wound, inward.
fn inward(text: &str) -> String {
    text.lines()
        .map(|l| {
            if l.contains("FACE_SURFACE(") || l.contains("FACE_BOUND(") {
                l.replace(".T.)", ".X.)").replace(".F.)", ".T.)").replace(".X.)", ".F.)")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn an_inward_solid_is_turned_and_said_on_the_status_line() {
    let text = std::fs::read_to_string(box_step()).expect("fixture");
    let dir = std::env::temp_dir().join(format!("camcraft-ui-import-outward-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let (bad_path, good_path) = (dir.join("inward.step"), dir.join("plain.step"));
    std::fs::write(&bad_path, inward(&text)).expect("write");
    std::fs::copy(box_step(), &good_path).expect("copy");

    let mut h = shell_harness();
    let st = &mut h.state_mut().state;
    st.pending.push_back(CommandKind::ImportFile(bad_path));
    st.settle();
    assert!(st.status.starts_with("imported inward"), "{}", st.status);
    assert!(st.status.contains("1 closed solid(s) wound inward (negative volume) were turned outward"), "{}", st.status);
    st.pending.push_back(CommandKind::ImportFile(good_path));
    st.settle();
    assert!(st.status.starts_with("imported plain"), "{}", st.status);
    assert!(!st.status.contains("turned outward"), "nothing changed, nothing said: {}", st.status);
    let _ = std::fs::remove_dir_all(&dir);
}
