//! #3298: OBJ face references count vertex RECORDS, not the unique positions
//! that the reader keeps. A file that repeats a vertex record must still
//! resolve each later index (positive and negative) to the record it names,
//! and a reference past the records, a zero, or a negative index past the
//! start must still give a clean refusal.
//!
//! Gate map: covers app::geometry.

use camcraft_app::geometry::import::{obj, ImportError};
use camcraft_app::geometry::{Point3, Triangle};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn refusal(text: &str) -> String {
    match obj::parse(text.as_bytes()) {
        Err(ImportError::Obj(msg)) => msg,
        other => panic!("expected an OBJ refusal, got {other:?}"),
    }
}

/// The example of the issue: records 4 and 5 repeat records 1 and 3.
const REPEATED: &str = "\
v -24 12 0
v -24 11 4
v -16 11 4
f 1 2 3
v -24 12 0
v -16 11 4
v -16 12 0
f 4 5 6
";

fn expected_pair() -> Vec<Triangle> {
    vec![
        Triangle::new(p(-24.0, 12.0, 0.0), p(-24.0, 11.0, 4.0), p(-16.0, 11.0, 4.0)),
        Triangle::new(p(-24.0, 12.0, 0.0), p(-16.0, 11.0, 4.0), p(-16.0, 12.0, 0.0)),
    ]
}

#[test]
fn obj_positive_indices_after_repeated_records_resolve() {
    let mesh = obj::parse(REPEATED.as_bytes()).expect("a legal OBJ must import");
    assert_eq!(mesh.facets.to_vec(), expected_pair());
}

#[test]
fn obj_negative_indices_after_repeated_records_resolve() {
    // -3 -2 -1 after six records name records 4, 5 and 6.
    let text = REPEATED.replace("f 4 5 6", "f -3 -2 -1");
    let mesh = obj::parse(text.as_bytes()).expect("relative references must import");
    assert_eq!(mesh.facets.to_vec(), expected_pair());
}

#[test]
fn obj_negative_index_counts_records_seen_so_far() {
    // The face sits between the records: -1 is record 3 here, not record 6.
    let text = "\
v 0 0 0
v 1 0 0
v 0 0 0
v 0 1 0
f -4 -3 -1
v 9 9 9
";
    let mesh = obj::parse(text.as_bytes()).expect("import");
    assert_eq!(
        mesh.facets.to_vec(),
        vec![Triangle::new(p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0))]
    );
}

#[test]
fn obj_slash_forms_after_repeated_records_resolve() {
    let text = REPEATED.replace("f 4 5 6", "f 4/1 5//2 6/3/4");
    let text = text.replace("f 1 2 3", "vt 0 0\nvt 1 0\nvt 1 1\nvn 0 0 1\nvn 0 0 1\nf 1 2 3");
    let mesh = obj::parse(text.as_bytes()).expect("slash references must import");
    assert_eq!(mesh.facets.to_vec(), expected_pair());
}

#[test]
fn obj_reference_past_the_records_is_refused() {
    // Seven is one past the six records.
    let msg = refusal(&REPEATED.replace("f 4 5 6", "f 4 5 7"));
    assert!(msg.contains("vertex reference '7' out of range"), "{msg}");
    assert!(msg.starts_with("line 8:"), "{msg}");
}

#[test]
fn obj_forward_reference_is_refused() {
    // Record 4 does not exist yet when the first face is read.
    let msg = refusal(&REPEATED.replace("f 1 2 3", "f 1 2 4"));
    assert!(msg.contains("vertex reference '4' out of range"), "{msg}");
}

#[test]
fn obj_zero_reference_is_refused() {
    let msg = refusal(&REPEATED.replace("f 4 5 6", "f 0 5 6"));
    assert!(msg.contains("vertex reference '0' out of range"), "{msg}");
}

#[test]
fn obj_negative_reference_past_the_start_is_refused() {
    let msg = refusal(&REPEATED.replace("f 4 5 6", "f -7 5 6"));
    assert!(msg.contains("vertex reference '-7' out of range"), "{msg}");
}

#[test]
fn obj_huge_and_garbage_references_are_refused() {
    for word in ["99999999999999999999", "-99999999999999999999", "x", "/1", "", "1.5"] {
        let face = if word.is_empty() { "f 4 5 6 /".to_string() } else { format!("f 4 5 {word}") };
        let text = REPEATED.replace("f 4 5 6", &face);
        match obj::parse(text.as_bytes()) {
            Err(ImportError::Obj(_)) => {}
            other => panic!("{face:?}: expected an OBJ refusal, got {other:?}"),
        }
    }
}

#[test]
fn obj_face_of_three_repeated_records_is_degenerate() {
    // Three records at one position give a zero-area triangle: refused.
    let text = "v 0 0 0\nv 0 0 0\nv 0 0 0\nf 1 2 3\n";
    let msg = refusal(text);
    assert!(msg.contains("degenerate triangle"), "{msg}");
}
