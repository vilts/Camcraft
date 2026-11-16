//! STEP closed-shell law (#1573): a shell the file declares closed must
//! mesh closed, and a face the kernel returns no mesh for is refused by
//! name - never imported as a model with a wall missing.
//!
//! Provenance: ISO 10303-42 `closed_shell` (and `manifold_solid_brep`,
//! whose outer shell is closed by the schema); the seams between faces are
//! judged in `step_seams` - only a real gap is refused, a face wound
//! against its neighbours is flipped and a T-junction is closed.
//! Our own code; nothing from any CAM vendor.
//!
//! Why: truck skips a face whose boundary it cannot place (no polygon) and
//! trims wrongly a face whose boundary it places wrongly, with no error
//! either way. The mesh then has holes, and the stock simulation and every
//! verdict judge the program against a part with walls missing.

use std::collections::HashSet;

use ruststep::ast::{DataSection, Name, Parameter};

use super::ImportError;
use crate::geometry::{Point3, Triangle};

/// Vertex weld for the seam check, in the file's length unit: truck's own
/// `TOLERANCE`, the distance it treats as one point.
pub(super) const WELD: f64 = 1e-6;

/// How far apart (in chord tolerances) two faces' boundaries may lie and
/// still be one seam: each side's polyline stands within one chord
/// tolerance of the shared curve, so the two within two of each other.
pub(super) const GAP_CHORDS: f64 = 2.0;

/// Shared edges a closed shell may keep running the same way in both its
/// faces after the flips: none. A healthy closed mesh has none, and a face
/// left wound against a neighbour turns inside and outside over for every
/// check that reads its winding (#1576 review).
pub(super) const UNRESOLVED_EDGES: usize = 0;

/// Flipped faces named in a note; the rest are counted.
const NAMED: usize = 8;

/// The shell being judged.
pub(super) struct Shell {
    pub id: u64,
    pub faces: usize,
    /// The file declares it closed ([`closed_shells`]).
    pub closed: bool,
    /// Millimetres per file unit, for the message.
    pub mm_per_unit: f64,
}

/// The ids of every shell the file declares closed: each `CLOSED_SHELL`,
/// and each shell a `MANIFOLD_SOLID_BREP` names as its outer.
pub(super) fn closed_shells(section: &DataSection) -> HashSet<u64> {
    let mut closed = HashSet::new();
    for (id, recs) in super::step_units::by_id(section) {
        for r in recs {
            if r.name == "CLOSED_SHELL" {
                closed.insert(id);
            } else if r.name == "MANIFOLD_SOLID_BREP"
                && let Some(Parameter::Ref(Name::Entity(outer))) = super::step_units::args(r).get(1)
            {
                closed.insert(*outer);
            }
        }
    }
    closed
}

/// Judge one shell's kept mesh, at chord tolerance `tol` (file units),
/// whose face `i` owns facets `ends[i-1]..ends[i]`; `names` gives a face's
/// entity id. In a closed shell, faces wound against their neighbours are
/// flipped in place (an open shell's winding is left as the kernel made it)
/// and said in the returned notes, by name, for the import result.
/// A closed shell with a real gap (a boundary edge farther than
/// [`GAP_CHORDS`] tolerances from any other face) is not refused here: its
/// hole is returned, named ([`Judged::open`], #1610), and the caller
/// refuses it or lands the model incomplete. Refused: a closed shell with a
/// shared edge still running the same way in both its faces after the
/// flips (more than [`UNRESOLVED_EDGES`]); and in a shell with no gap a
/// face the kernel gave no mesh (`dropped`, already named).
pub(super) fn judge(
    tris: &mut [Triangle],
    ends: Option<&[usize]>,
    tol: f64,
    shell: &Shell,
    names: impl Fn(usize) -> Option<u64>,
    dropped: Option<String>,
) -> Result<Judged, ImportError> {
    let id = shell.id;
    let mut notes = Vec::new();
    let face = |f: usize| match names(f) {
        Some(n) => format!("face #{n} ({} of {})", f + 1, shell.faces),
        None => format!("face {} of {}", f + 1, shell.faces),
    };
    if shell.closed {
        let whole = [tris.len()];
        let gap = (GAP_CHORDS * tol).max(WELD);
        let seams = super::step_seams::close(tris, ends.unwrap_or(&whole), WELD, gap);
        if !seams.flipped.is_empty() {
            let mut said: Vec<String> = seams.flipped.iter().take(NAMED).map(|&f| face(f)).collect();
            if seams.flipped.len() > NAMED {
                said.push(format!("{} more", seams.flipped.len() - NAMED));
            }
            let note = format!(
                "shell #{id}: {} face(s) wound against their neighbours were flipped: {}",
                seams.flipped.len(),
                said.join(", ")
            );
            tracing::warn!("step: {note}");
            notes.push(note);
        }
        if seams.contradictions > 0 && seams.unresolved_edges == 0 {
            let note = format!(
                "shell #{id}: {} face pair(s) met only across T-junctions still disagree on winding after the flips; \
                 every shared edge agrees",
                seams.contradictions
            );
            tracing::warn!("step: {note}");
            notes.push(note);
        }
        if seams.reversed_edges > 0 || seams.t_junction_edges > 0 {
            tracing::info!(
                "step: shell #{id}: {} seam edges reversed, {} T-junction edges closed within {gap:e}",
                seams.reversed_edges,
                seams.t_junction_edges
            );
        }
        if seams.gaps > 0 {
            let k = shell.mm_per_unit;
            let mm = |p: Point3| Point3::new(p.x * k, p.y * k, p.z * k);
            tracing::error!("step: closed shell #{id} meshes open ({} boundary edges with a gap)", seams.gaps);
            let first = seams.first_gap.map(|(a, b)| (mm(a), mm(b)));
            let gap = Gap { edges: seams.gaps, tol_mm: gap * k, widest_mm: seams.widest * k, face: seams.gap_face.map(face), first };
            return Ok(Judged { notes, open: Some(hole(shell, &gap, dropped)) });
        }
        if seams.unresolved_edges > UNRESOLVED_EDGES {
            let between = seams.unresolved_pair.map(|(f, g)| format!(", most between {} and {}", face(f), face(g))).unwrap_or_default();
            tracing::error!("step: closed shell #{id}: {} shared edges still reversed after the flips", seams.unresolved_edges);
            return Err(ImportError::Step(format!(
                "closed shell #{id} ({} faces) is wound inconsistently: {} shared edges still run the same way in both faces \
                 after the flips{between} - its inside and outside cannot be told apart there; re-export the model, or import an STL",
                shell.faces, seams.unresolved_edges
            )));
        }
    }
    if let Some(face) = dropped {
        tracing::error!("step: {face} of shell #{id} got no mesh from the kernel");
        return Err(dropped_refusal(&face, id));
    }
    Ok(Judged { notes, open: None })
}

/// What [`judge`] found of a shell it did not refuse.
pub(super) struct Judged {
    /// What it changed on the way, one line each.
    pub notes: Vec<String>,
    /// A closed shell's real gap, named (#1610): where the mesh has walls
    /// the file has not.
    pub open: Option<String>,
}

/// The refusal for a mesh with a hole, where the caller cannot land the
/// model incomplete (a fixture, a tool body, a mesh-only import).
#[must_use]
pub(super) fn open_refusal(hole: &str) -> ImportError {
    ImportError::Step(format!("{hole} - the model would import with walls missing; re-export the model, or import an STL"))
}

/// A closed shell's real gap, in mm.
struct Gap {
    /// Boundary edges farther than `tol_mm` from any other face.
    edges: usize,
    tol_mm: f64,
    /// The widest measured distance to another face's boundary.
    widest_mm: f64,
    /// The face owning the most of them, named.
    face: Option<String>,
    first: Option<(Point3, Point3)>,
}

/// A closed shell's real gap, said: where it is and how wide. `dropped`
/// names a face the kernel returned no mesh for, if any.
fn hole(shell: &Shell, gap: &Gap, dropped: Option<String>) -> String {
    let on = gap.face.as_ref().map(|f| format!(", most on {f}")).unwrap_or_default();
    let widest = if gap.widest_mm > 0.0 { format!(", up to {:.3} mm", gap.widest_mm) } else { String::new() };
    let at = gap
        .first
        .map(|(a, b)| format!("; the first from ({:.4}, {:.4}, {:.4}) to ({:.4}, {:.4}, {:.4}) mm", a.x, a.y, a.z, b.x, b.y, b.z))
        .unwrap_or_default();
    let cause = dropped.map(|f| format!("; {f} got no mesh from the kernel")).unwrap_or_default();
    format!(
        "closed shell #{} ({} faces) meshes open: {} boundary edges lie more than {:.2e} mm from any other face{on}{widest}{at}{cause}",
        shell.id, shell.faces, gap.edges, gap.tol_mm
    )
}

/// The refusal for a face the kernel returned no mesh for, in a shell the
/// file does not declare closed.
fn dropped_refusal(face: &str, shell: u64) -> ImportError {
    ImportError::Step(format!(
        "{face} in shell #{shell} could not be tessellated: the kernel could not place its boundary on its surface \
         and returned no mesh for it - re-export the model, or import an STL"
    ))
}
