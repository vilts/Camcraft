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

/// Request cooperative cancellation after 300 seconds. The planner has no
/// cancellation hook, so this is not a deadline for completion.
#[cfg(not(target_arch = "wasm32"))]
const TIME_BOUND: std::time::Duration = std::time::Duration::from_secs(300);

/// The final body of a Solidcraft design as a watertight mesh, in mm,
/// with what the import did (one note each). At most `budget` facets.
#[cfg(not(target_arch = "wasm32"))]
pub fn parse(data: &[u8], budget: usize, chord_cap: f64) -> Result<(Mesh, Vec<String>), SolidcraftError> {
    use crate::geometry::{Point3, Triangle};
    use camcraft_kernel::machining_mesh::machining_mesh;
    use camcraft_kernel::solidcore_impl::SolidcoreKernel;
    use camcraft_kernel::solidcraft::import_solidcraft;

    let stop = camcraft_kernel::CancelToken::new();
    let _watch = Watchdog::start(stop.clone());
    let design = import_solidcraft(&mut SolidcoreKernel::default(), data, &stop)
        .map_err(|e| SolidcraftError::Refused(e.to_string()))?;
    let mesh = machining_mesh(&design.solid, chord_cap, &stop)
        .map_err(|e| SolidcraftError::Refused(e.to_string()))?;
    if mesh.triangles.len() > budget {
        return Err(SolidcraftError::TooLarge);
    }
    let at = |i: u32| -> Result<Point3, SolidcraftError> {
        let p = mesh
            .positions
            .get(i as usize)
            .ok_or_else(|| SolidcraftError::Refused(format!("the body's mesh names vertex {i}, which it does not have")))?;
        Ok(Point3::new(p[0], p[1], p[2]))
    };
    let mut facets = Vec::with_capacity(mesh.triangles.len());
    for [a, b, c] in &mesh.triangles {
        facets.push(Triangle::new(at(*a)?, at(*b)?, at(*c)?));
    }
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

/// Cancels the import's token after [`TIME_BOUND`]; ends at once when the
/// import ends first (dropped), so no thread outlives the import by more
/// than a wake-up.
#[cfg(not(target_arch = "wasm32"))]
struct Watchdog(Option<std::sync::mpsc::Sender<()>>);

#[cfg(not(target_arch = "wasm32"))]
impl Watchdog {
    fn start(stop: camcraft_kernel::CancelToken) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let spawned = std::thread::Builder::new().name("sc-import-watchdog".into()).spawn(move || {
            if let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(TIME_BOUND) {
                tracing::warn!("Solidcraft import passed its {TIME_BOUND:?} bound; cancelling");
                stop.cancel();
            }
        });
        match spawned {
            Ok(_) => Watchdog(Some(tx)),
            Err(e) => {
                // No watchdog: the reader's own size bounds still hold.
                tracing::warn!("Solidcraft import runs without its time bound: {e}");
                Watchdog(None)
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for Watchdog {
    fn drop(&mut self) {
        // Dropping the sender wakes the watchdog with `Disconnected`.
        self.0.take();
    }
}
