//! #1573 review: a STEP face the importer flipped to agree with its
//! neighbours is said by name on the status line, not only in the log; an
//! import that changed nothing says nothing more.
//! Provenance: our own tests over our own shell; no vendor material.
//!
//! Gate map (#1573): covers ui::state (finish_import), app::commands::import_model (notes).

use super::*;

fn box_step() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/box.step")
}

#[test]
fn a_flipped_face_is_named_on_the_status_line() {
    let text = std::fs::read_to_string(box_step()).expect("fixture");
    let (face, bound) = ("#32 = FACE_SURFACE('', (#33), #90, .T.);", "#33 = FACE_BOUND('', #34, .T.);");
    assert_eq!((text.matches(face).count(), text.matches(bound).count()), (1, 1), "fixture layout changed");
    let bad = text.replace(face, "#32 = FACE_SURFACE('', (#33), #90, .F.);").replace(bound, "#33 = FACE_BOUND('', #34, .F.);");
    let dir = std::env::temp_dir().join(format!("camcraft-ui-import-notes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let (bad_path, good_path) = (dir.join("reversed.step"), dir.join("plain.step"));
    std::fs::write(&bad_path, bad).expect("write");
    std::fs::copy(box_step(), &good_path).expect("copy");

    let mut h = shell_harness();
    let st = &mut h.state_mut().state;
    st.pending.push_back(CommandKind::ImportFile(bad_path));
    st.settle();
    assert!(st.status.starts_with("imported reversed"), "{}", st.status);
    assert!(st.status.contains("1 face(s) wound against their neighbours were flipped: face #32 ("), "{}", st.status);
    st.pending.push_back(CommandKind::ImportFile(good_path));
    st.settle();
    assert!(st.status.starts_with("imported plain"), "{}", st.status);
    assert!(!st.status.contains("flipped"), "nothing changed, nothing said: {}", st.status);
    let _ = std::fs::remove_dir_all(&dir);
}
