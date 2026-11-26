//! STEP model import (#917): the kernel reads every solid and every sheet of
//! the file (`camcraft_kernel::solidcore_step::import_step_model`) and
//! meshes each solid with the machining mesh (watertight, outward,
//! deviation measured) and each sheet with its open boundary allowed, at the
//! importer's chord cap; a model over the facet budget is meshed again at a
//! coarser chord, and so is a model whose edges lie off their surfaces past
//! the chord in a file that states a tolerance (at twice it, at most
//! 0.02 mm, #2976); the chord it was made at is handed out (#864, #1054).
//! Every kernel refusal keeps its reason.
//! Provenance: our own glue over camcraft-kernel's public API (solidcore-step
//! reads ISO 10303-21/-42 from the public standards, Apache-2.0); no
//! HSMWorks material consulted.

use crate::geometry::Mesh;

/// Why a STEP file gave no mesh.
#[derive(Debug, thiserror::Error)]
pub enum StepError {
    /// The model does not fit the facet budget at any chord tried.
    #[error("model exceeds the import limit")]
    TooLarge,
    /// The file states no length unit and the caller stated none (#1874):
    /// the model is never read in a guessed unit.
    #[error("the STEP file states no length unit")]
    MissingUnits,
    /// The reader, the kernel or the mesher refused the file; the reason
    /// names what failed.
    #[error("{0}")]
    Refused(String),
}

/// A STEP model meshed in millimetres.
#[derive(Debug)]
pub struct StepMesh {
    /// Every solid of the file, each a watertight outward mesh, with one
    /// facet range for each solid ([`Mesh::solid_ranges`]). A file with a
    /// sheet (a surface model of open shells), or a solid meshed with faces
    /// left out, gives one mesh with no ranges, as one soup.
    pub mesh: Mesh,
    /// The chord tolerance the mesh was made at (mm): the chord cap, or
    /// coarser when the cap did not fit the facet budget or the model's
    /// edge gaps (#2976).
    pub chord_mm: f64,
    /// The largest certified distance (mm) from a facet to its surface,
    /// over every solid and sheet: the kernel's measured bound, never more
    /// than `chord_mm` (a body over its chord is refused).
    pub deviation_mm: f64,
    /// What the reader and the mesher noted, one line each.
    pub notes: Vec<String>,
    /// Only from [`parse_open`]: each solid that meshes with faces left
    /// out, every such face named with the mesher's reason (#1610). The
    /// mesh then has walls missing there: a caller lands it incomplete or
    /// refuses it, never as a whole model.
    pub open: Option<String>,
    /// Whether the chord was made coarser because the model's edges lie off
    /// their surfaces (#2976); its note is then the first of `notes`.
    pub loosened: bool,
    /// True only when file tolerance, without a budget retry, set the chord.
    pub file_tolerance: bool,
    /// Millimetres per length unit of each context that states one, in the
    /// order met (#1874). Never empty: a file that states no unit is
    /// refused, or read in the caller's unit, which is then listed here.
    pub length_units_mm: Vec<f64>,
    /// The caller's millimetres per unit, when the file states no unit and
    /// the model was read in it (#1874). `None`: the file states its unit.
    pub assumed_mm: Option<f64>,
}

/// How many times a model over the facet budget is meshed again, coarser,
/// before it is refused as too large.
pub const COARSEN_TRIES: u32 = 6;

/// The most one coarsening multiplies the chord by.
const COARSEN_MAX_STEP: f64 = 16.0;

/// The advice a mesh refusal ends with (the wording of the earlier STEP
/// importer's tessellation refusals).
pub const ADVICE: &str = "re-export the model, or import an STL";

/// The solids and sheets of a STEP file as one mesh in mm, at most `budget`
/// facets, at the chord `chord_cap` (mm) or coarser when that does not fit.
/// Every solid meshes whole or the file is refused ([`StepMesh::open`] is
/// always `None`).
#[cfg(not(target_arch = "wasm32"))]
pub fn parse(data: &[u8], budget: usize, chord_cap: f64) -> Result<StepMesh, StepError> {
    read(data, budget, chord_cap, false, None)
}

/// [`parse`], but a solid whose faces the mesher leaves out is meshed with
/// those faces missing and named in [`StepMesh::open`] (#1610): for the
/// model import, which lands it incomplete. Every other refusal stays.
#[cfg(not(target_arch = "wasm32"))]
pub fn parse_open(data: &[u8], budget: usize, chord_cap: f64) -> Result<StepMesh, StepError> {
    read(data, budget, chord_cap, true, None)
}

/// [`parse_open`], and the caller's unit, `assume_mm` millimetres per unit,
/// for a file that states no length unit (#1874). A file that states its
/// own unit keeps it. With `None`, a file that states no unit is
/// [`StepError::MissingUnits`]: the model is never read in a guessed unit.
#[cfg(not(target_arch = "wasm32"))]
pub fn parse_open_assuming(data: &[u8], budget: usize, chord_cap: f64, assume_mm: Option<f64>) -> Result<StepMesh, StepError> {
    read(data, budget, chord_cap, true, assume_mm)
}

#[cfg(not(target_arch = "wasm32"))]
fn read(data: &[u8], budget: usize, chord_cap: f64, keep_open: bool, assume_mm: Option<f64>) -> Result<StepMesh, StepError> {
    use camcraft_kernel::machining_mesh::{MachiningMesh, MeshRefusal, machining_mesh};
    use camcraft_kernel::machining_mesh_open::{partial_mesh, sheet_mesh};
    use camcraft_kernel::solidcore_impl::SolidcoreKernel;
    use camcraft_kernel::solidcore_step::import_step_model_assuming;

    let stop = camcraft_kernel::CancelToken::new();
    let _watch = crate::kernel_import::Watchdog::start(stop.clone(), "STEP import");
    let import = import_step_model_assuming(&mut SolidcoreKernel::default(), data, &stop, assume_mm)
        .map_err(|e| StepError::Refused(locate(data, e.to_string())))?;
    // #1874: a file that states no length unit is never read in a guessed
    // one; the caller states the unit, or the file is refused.
    if import.length_units_mm.is_empty() {
        return Err(StepError::MissingUnits);
    }
    let refused = |label: &str, e: MeshRefusal| StepError::Refused(format!("{label} could not be tessellated: {e} - {ADVICE}"));
    // A face whose edges lie off its surface by more than the chord cannot
    // be meshed to the chord. A file that states a tolerance is then meshed
    // once at the coarser tolerance its edge gaps allow (#2976): the chord
    // handed out is coarser, so the model is reduced fidelity (#864).
    let loosen = |chord: f64, e: &MeshRefusal| match e {
        MeshRefusal::FaceCapped { .. } | MeshRefusal::EdgeCapped { .. } | MeshRefusal::Deviation { .. } => {
            import.edge_gap.loose_chord_mm(chord)
        }
        _ => None,
    };
    let mut chord = chord_cap;
    let mut loosened = false;
    let mut budget_coarsened = false;
    'tries: for _ in 0..=COARSEN_TRIES {
        let mut meshes: Vec<(String, MachiningMesh)> = Vec::new();
        let mut holes: Vec<String> = Vec::new();
        let mut total = 0usize;
        for solid in &import.solids {
            let label = solid_label(solid.item, &solid.name);
            let mesh = match machining_mesh(solid, chord, &stop) {
                Ok(mesh) => mesh,
                Err(MeshRefusal::FaceLeftOut { .. }) if keep_open => {
                    let open = partial_mesh(solid, chord, &stop).map_err(|e| refused(&label, e))?;
                    holes.push(hole(&label, solid.record.faces.len(), &open.left_out));
                    open.mesh
                }
                Err(e) => match loosen(chord, &e) {
                    Some(loose) => {
                        (chord, loosened) = (loose, true);
                        continue 'tries;
                    }
                    None => return Err(refused(&label, e)),
                },
            };
            total = total.saturating_add(mesh.triangles.len());
            meshes.push((label, mesh));
            if total > budget {
                break;
            }
        }
        for sheet in &import.sheets {
            if total > budget {
                break;
            }
            let label = format!("sheet #{}", sheet.item);
            let mesh = match sheet_mesh(&sheet.record, chord, &stop) {
                Ok(mesh) => mesh,
                Err(e) => match loosen(chord, &e) {
                    Some(loose) => {
                        (chord, loosened) = (loose, true);
                        continue 'tries;
                    }
                    None => return Err(refused(&label, e)),
                },
            };
            total = total.saturating_add(mesh.triangles.len());
            meshes.push((label, mesh));
        }
        if total <= budget {
            let gap_note = loosened.then(|| import.edge_gap.loose_note(chord));
            return finish(&import, &meshes, chord, &holes, gap_note, loosened && !budget_coarsened);
        }
        // Facets fall about as the chord rises; `total` may be a lower bound
        // (the count stops at the budget), so the chord at least doubles.
        let ratio = total as f64 / budget.max(1) as f64;
        budget_coarsened = true;
        chord *= ratio.clamp(2.0, COARSEN_MAX_STEP);
        tracing::warn!("STEP model over its {budget}-facet budget; meshing again at {chord:.4} mm");
    }
    Err(StepError::TooLarge)
}

/// The meshes as one [`Mesh`], with every note.
#[cfg(not(target_arch = "wasm32"))]
fn finish(
    import: &camcraft_kernel::solidcore_step::StepModel,
    meshes: &[(String, camcraft_kernel::machining_mesh::MachiningMesh)],
    chord: f64,
    holes: &[String],
    gap_note: Option<String>,
    file_tolerance: bool,
) -> Result<StepMesh, StepError> {
    // One facet range for each solid: solids that share a wall stay apart
    // for the slicer and the winding check (#917). A sheet or a partial
    // mesh is no solid, so a file with one gets no ranges.
    let mut facets = Vec::new();
    let mut ranges = Vec::with_capacity(meshes.len());
    for (_, mesh) in meshes {
        let start = facets.len();
        crate::kernel_import::push_facets(mesh, &mut facets).map_err(StepError::Refused)?;
        ranges.push(start..facets.len());
    }
    let loosened = gap_note.is_some();
    let mut notes: Vec<String> = gap_note.into_iter().collect();
    notes.extend(import.notes.iter().map(|n| format!("STEP: {n}")));
    for solid in &import.solids {
        let label = solid_label(solid.item, &solid.name);
        notes.extend(solid.notes.iter().map(|n| format!("STEP {label}: {n}")));
    }
    for sheet in &import.sheets {
        notes.extend(sheet.notes.iter().map(|n| format!("STEP sheet #{}: {n}", sheet.item)));
    }
    for (label, mesh) in meshes {
        notes.extend(mesh.report.notes.iter().map(|n| format!("STEP {label}: {n}")));
    }
    // The earlier importer's wording for solids wound inward (#1617).
    let inward: Vec<usize> = meshes.iter().filter(|(_, m)| m.report.turned_outward).map(|(_, m)| m.triangles.len()).collect();
    if !inward.is_empty() {
        let turned: usize = inward.iter().sum();
        notes.push(format!(
            "{} closed solid(s) wound inward (negative volume) were turned outward: {turned} facet(s) reversed",
            inward.len()
        ));
    }
    let mesh = if import.sheets.is_empty() && holes.is_empty() {
        Mesh::from_solids(facets, ranges).map_err(|e| StepError::Refused(e.to_string()))?
    } else {
        Mesh::from_facets(facets)
    };
    let deviation_mm = meshes.iter().map(|(_, m)| m.report.deviation.largest()).fold(0.0, f64::max);
    let open = (!holes.is_empty()).then(|| holes.join("; "));
    Ok(StepMesh {
        mesh,
        chord_mm: chord,
        deviation_mm,
        notes,
        open,
        loosened,
        file_tolerance,
        length_units_mm: import.length_units_mm.clone(),
        assumed_mm: import.assumed_mm,
    })
}

/// `closed shell of solid #12 (10 faces) meshes open: face 4 of 10 got no
/// mesh from the kernel (reason)`, every left-out face with its reason.
#[cfg(not(target_arch = "wasm32"))]
fn hole(label: &str, faces: usize, left_out: &[camcraft_kernel::machining_mesh::LeftOut]) -> String {
    let named: Vec<String> = left_out
        .iter()
        .map(|l| format!("face {} of {faces} got no mesh from the kernel ({})", l.face.saturating_add(1), l.reason))
        .collect();
    format!("closed shell of {label} ({faces} faces) meshes open: {}", named.join(", "))
}

/// The browser build keeps solidcore out of its wasm: a STEP file is
/// refused by name there.
#[cfg(target_arch = "wasm32")]
pub fn parse(_data: &[u8], _budget: usize, _chord_cap: f64) -> Result<StepMesh, StepError> {
    Err(StepError::Refused("STEP models open in the desktop app only, not in the browser build".into()))
}

/// [`parse`] in the browser build: refused by name.
#[cfg(target_arch = "wasm32")]
pub fn parse_open(data: &[u8], budget: usize, chord_cap: f64) -> Result<StepMesh, StepError> {
    parse(data, budget, chord_cap)
}

/// [`parse`] in the browser build: refused by name.
#[cfg(target_arch = "wasm32")]
pub fn parse_open_assuming(data: &[u8], budget: usize, chord_cap: f64, _assume_mm: Option<f64>) -> Result<StepMesh, StepError> {
    parse(data, budget, chord_cap)
}

/// A syntax refusal ("... at line 9: ...") with the entity that holds the
/// line named after it, and the string or comment that never closes (#912:
/// the user finds the bad record by its id and its first line). Any other
/// refusal (a solid the kernel could not read) ends with [`ADVICE`].
#[cfg(not(target_arch = "wasm32"))]
fn locate(data: &[u8], reason: String) -> String {
    let Some(line) = reason
        .split_once("syntax error at line ")
        .and_then(|(_, rest)| rest.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse::<usize>().ok())
    else {
        return format!("{reason} - {ADVICE}");
    };
    let Ok(text) = std::str::from_utf8(data) else {
        return reason;
    };
    let at = scan(text, line);
    let mut out = reason;
    if let Some((id, first)) = at.entity {
        out.push_str(&format!(" (in entity #{id}, which starts at line {first})"));
    }
    if let Some((what, first)) = at.open {
        out.push_str(&format!("; the {what} opened at line {first} never closes"));
    }
    out
}

/// Where a line of a STEP file stands: the last entity begun on or before
/// it, and a string or comment still open at its end.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct Spot {
    entity: Option<(u64, usize)>,
    open: Option<(&'static str, usize)>,
}

/// [`Spot`] at the end of `line` (counted from 1): a scan that skips
/// strings (`'...'`, `''` inside) and comments (`/* ... */`).
#[cfg(not(target_arch = "wasm32"))]
fn scan(text: &str, line: usize) -> Spot {
    let mut spot = Spot::default();
    let (mut at, mut fresh) = (1usize, true);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match spot.open {
            Some(("string", _)) => {
                if c == '\'' && chars.next_if_eq(&'\'').is_none() {
                    spot.open = None;
                }
            }
            Some(_) => {
                if c == '*' && chars.next_if_eq(&'/').is_some() {
                    spot.open = None;
                }
            }
            None if c == '\'' => spot.open = Some(("string", at)),
            None if c == '/' && chars.next_if_eq(&'*').is_some() => spot.open = Some(("comment", at)),
            None if c == '#' && fresh => {
                let mut id = String::new();
                while let Some(d) = chars.next_if(char::is_ascii_digit) {
                    id.push(d);
                }
                while chars.next_if(|w| *w == ' ' || *w == '\t').is_some() {}
                if let (Some('='), Ok(id)) = (chars.peek(), id.parse()) {
                    spot.entity = Some((id, at));
                }
            }
            None => {}
        }
        if c == '\n' {
            if at >= line {
                break;
            }
            at += 1;
            fresh = true;
        } else if !c.is_whitespace() {
            fresh = false;
        }
    }
    spot
}

/// `solid #12 'bracket'`, or `solid #12` for a solid without a name.
#[cfg(not(target_arch = "wasm32"))]
fn solid_label(item: u64, name: &str) -> String {
    if name.is_empty() {
        format!("solid #{item}")
    } else {
        format!("solid #{item} '{name}'")
    }
}
