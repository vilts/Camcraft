//! Solidcraft import is a geometry API, independent of app document wiring.
//! Provenance: our own API contract over the public Solidcraft reader.
//! Gate map (M4.4): covers camcraft_geometry::solidcraft.

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn domain_parser_preserves_the_kernel_refusal_reason() {
    use camcraft_geometry::solidcraft::{parse, SolidcraftError};

    let error = parse(b"not a ZIP container", 1_000, 0.005).expect_err("damaged design must be refused");
    let SolidcraftError::Refused(reason) = error else {
        panic!("damaged design must retain the reader's diagnostic");
    };
    assert!(reason.contains("import_solidcraft") || reason.contains("ZIP") || reason.contains("zip"), "{reason}");
}
