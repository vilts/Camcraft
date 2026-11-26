//! Solidcraft `.sc` design import (#1456): the kernel reads the design's
//! cached final body (`camcraft_kernel::solidcraft`) and meshes it with the
//! machining mesh at the importer's chord cap; this module turns that mesh
//! into geometry facets and preserves every kernel refusal reason.
//! Provenance: our own glue over camcraft-kernel's public API (its reader
//! follows Solidcraft's published source, tangency-labs, Apache-2.0); no
//! HSMWorks material consulted.

use crate::geometry::Mesh;

#[derive(Debug, thiserror::Error)]
pub enum SolidcraftError {
    #[error("model exceeds the import limit")]
    TooLarge,
    #[error("{0}")]
    Refused(String),
}

/// SHA-256 of `data` as lowercase hex (#2463): the digest a linked model
/// records of its source file, so a changed file is told from an unchanged
/// one. The kernel's FIPS 180-4 implementation; native only, as the kernel.
#[cfg(not(target_arch = "wasm32"))]
pub use camcraft_kernel::solidcraft::hex_digest as sha256_hex;

/// The same SHA-256 fed in pieces (#2689): a source file is hashed as it is
/// read, from the bytes the import parses.
#[cfg(not(target_arch = "wasm32"))]
pub use camcraft_kernel::solidcraft::Sha256;

/// The final body of a Solidcraft design as a watertight mesh, in mm,
/// with what the import did (one note each). At most `budget` facets.
#[cfg(not(target_arch = "wasm32"))]
pub fn parse(data: &[u8], budget: usize, chord_cap: f64) -> Result<(Mesh, Vec<String>), SolidcraftError> {
    use camcraft_kernel::machining_mesh::machining_mesh;
    use camcraft_kernel::solidcore_impl::SolidcoreKernel;
    use camcraft_kernel::solidcraft::import_solidcraft;

    let stop = camcraft_kernel::CancelToken::new();
    let _watch = crate::kernel_import::Watchdog::start(stop.clone(), "Solidcraft import");
    let design = import_solidcraft(&mut SolidcoreKernel::default(), data, &stop)
        .map_err(|e| SolidcraftError::Refused(e.to_string()))?;
    let mesh = machining_mesh(&design.solid, chord_cap, &stop)
        .map_err(|e| SolidcraftError::Refused(e.to_string()))?;
    if mesh.triangles.len() > budget {
        return Err(SolidcraftError::TooLarge);
    }
    let mut facets = Vec::new();
    crate::kernel_import::push_facets(&mesh, &mut facets).map_err(SolidcraftError::Refused)?;
    let mut notes: Vec<String> = mesh.report.notes.iter().map(|n| format!("Solidcraft: {n}")).collect();
    if !design.design.is_empty() {
        notes.insert(0, format!("Solidcraft design '{}': the cached final body only (no parameters or feature history)", design.design));
    }
    Ok((Mesh::from_facets(facets), notes))
}

/// The browser build keeps solidcore out of its wasm: a `.sc` is refused
/// by name there.
#[cfg(target_arch = "wasm32")]
pub fn parse(_data: &[u8], _budget: usize, _chord_cap: f64) -> Result<(Mesh, Vec<String>), SolidcraftError> {
    Err(SolidcraftError::Refused("Solidcraft designs open in the desktop app only, not in the browser build".into()))
}
