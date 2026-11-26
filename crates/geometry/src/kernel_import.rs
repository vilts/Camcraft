//! What the Solidcraft and the STEP import share (#1456, #917): the time
//! bound (a thread that cancels the import's token after [`TIME_BOUND`])
//! and the machining mesh turned into geometry facets.
//! Provenance: our own glue over camcraft-kernel's public API and the
//! standard library's channels; no HSMWorks material consulted.

use camcraft_kernel::machining_mesh::MachiningMesh;

use crate::geometry::{Point3, Triangle};

/// Appends the triangles of `mesh` to `out`. A triangle that names a vertex
/// the mesh does not have is refused by name, never read out of bounds.
pub(crate) fn push_facets(mesh: &MachiningMesh, out: &mut Vec<Triangle>) -> Result<(), String> {
    let at = |i: u32| -> Result<Point3, String> {
        let p = mesh
            .positions
            .get(i as usize)
            .ok_or_else(|| format!("the body's mesh names vertex {i}, which it does not have"))?;
        Ok(Point3::new(p[0], p[1], p[2]))
    };
    out.try_reserve(mesh.triangles.len()).map_err(|e| format!("no memory for the mesh: {e}"))?;
    for [a, b, c] in &mesh.triangles {
        out.push(Triangle::new(at(*a)?, at(*b)?, at(*c)?));
    }
    Ok(())
}

/// Request cooperative cancellation after 300 seconds. Parts of the kernel
/// have no cancellation hook, so this is not a deadline for completion.
pub(crate) const TIME_BOUND: std::time::Duration = std::time::Duration::from_secs(300);

/// Cancels the import's token after [`TIME_BOUND`]; ends at once when the
/// import ends first (dropped), so no thread outlives the import by more
/// than a wake-up.
pub(crate) struct Watchdog(Option<std::sync::mpsc::Sender<()>>);

impl Watchdog {
    /// `what` names the import in the log ("Solidcraft import").
    pub(crate) fn start(stop: camcraft_kernel::CancelToken, what: &'static str) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let spawned = std::thread::Builder::new().name("import-watchdog".into()).spawn(move || {
            if let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(TIME_BOUND) {
                tracing::warn!("{what} passed its {TIME_BOUND:?} bound; cancelling");
                stop.cancel();
            }
        });
        match spawned {
            Ok(_) => Watchdog(Some(tx)),
            Err(e) => {
                // No watchdog: the reader's own size bounds still hold.
                tracing::warn!("{what} runs without its time bound: {e}");
                Watchdog(None)
            }
        }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        // Dropping the sender wakes the watchdog with `Disconnected`.
        self.0.take();
    }
}
