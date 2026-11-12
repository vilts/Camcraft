//! STEP import on the solidcore side of the seam (M12.1 stage 4, #916):
//! file bytes -> solids in millimetres + the file's length units +
//! diagnostics, with a size bound and a time bound. BENCH ONLY: the app
//! still imports STEP through truck-stepio; this is what the dual-importer
//! bench (tests/step_agreement.rs) measures against it.
//! Provenance: calls the public API of solidcore-step (the ISO 10303-21
//! exchange structure and ISO 10303-42 geometry and topology reader of the
//! solidcore workspace, Apache-2.0, written from those public standards),
//! solidcore-convert (`brep::to_kernel_body`) and solidcore (the Apache-2.0
//! port of NVIDIA's SMLib: cancel token, area integrator), as their rustdoc
//! documents them, at the commit pinned in Cargo.toml. No reader internals
//! are reimplemented here and no HSMWorks material was consulted.

use std::panic::{AssertUnwindSafe, catch_unwind};

use solidcore::kernel::CancelToken;
use solidcore::mass::integrator::properties::brep_area;
use solidcore::topology::Brep;
use solidcore_convert::brep::to_kernel_body;
use solidcore_step::p21::{self, Exchange, Progress};
use solidcore_step::{topology, units};

use crate::solidcore_impl::{SolidcoreKernel, VOLUME_ACCURACY, checked, failed};
use crate::solidcore_import::{HEAL_GAP_LIMIT, vertex_gaps};
use crate::{Kernel, KernelError, Measure};

const NAME: &str = "solidcore";

/// Largest STEP file the seam hands the reader (64 MiB, the native reader's
/// bound). The reader's own cap is 512 MiB; it holds the text and every
/// parsed entity in memory, so the seam refuses earlier, by name.
pub const MAX_STEP_BYTES: usize = 64 << 20;

/// One solid of a STEP file, in millimetres, healed and checked.
#[derive(Debug)]
pub struct StepSolid {
    /// The file's entity id of the solid item (`#id`).
    pub item: u64,
    /// The item's own name, else its product's (may be empty).
    pub name: String,
    /// The kernel solid: closed, manifold, positive volume.
    pub brep: Brep,
    /// The reader's B-rep record the kernel solid was built from (mm): what
    /// [`crate::machining_mesh`] meshes, face for face (#940).
    pub record: solidcore_record::brep::Brep,
    /// Exact volume, and the port's "tight" bounding box (mm). FINDING at
    /// the pinned commit (#939): the box takes the whole cylinder of a
    /// trimmed cylinder face, so it can be too large. Do not size or place
    /// anything from it until that is fixed.
    pub measure: Measure,
    /// Surface area (mm^2), the port's estimate at [`VOLUME_ACCURACY`].
    pub area: f64,
    /// Largest vertex-to-edge-end distance left in the solid (mm), always
    /// at or under [`HEAL_GAP_LIMIT`].
    pub vertex_gap: f64,
    /// What the reader repaired or noted on this solid.
    pub notes: Vec<String>,
}

/// A solid item of the file that did not become a [`StepSolid`], and why.
#[derive(Debug, Clone, PartialEq)]
pub struct StepRefusal {
    /// The file's entity id of the item (`#id`).
    pub item: u64,
    /// Which step refused it (`"read"`, `"build"`, `"check"`, `"measure"`).
    pub stage: &'static str,
    /// The reason, as the refusing step words it.
    pub reason: String,
}

/// Everything [`read_step`] found in one file.
#[derive(Debug, Default)]
pub struct StepRead {
    /// The solids that passed every check.
    pub solids: Vec<StepSolid>,
    /// The solid items that did not.
    pub refused: Vec<StepRefusal>,
    /// Millimetres per length unit of each representation context that
    /// states one, distinct values in the order met (`[25.4]` for an inch
    /// file). Empty: no context states a unit and the reader took
    /// millimetres. Every solid above is already in millimetres.
    pub length_units_mm: Vec<f64>,
    /// File-level diagnostics (duplicate entity ids, mixed units).
    pub notes: Vec<String>,
}

/// The time bound's watcher of the Part 21 parse.
struct Watch<'a>(&'a CancelToken);

impl Progress for Watch<'_> {
    fn cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
    fn progress(&self, _stage: &'static str, _done: usize, _total: usize) {}
}

fn timed_out(stage: &'static str) -> KernelError {
    KernelError::TimedOut {
        op: "read_step",
        stage,
    }
}

fn refused_input(detail: String) -> KernelError {
    KernelError::InvalidInput {
        op: "read_step",
        detail,
    }
}

/// Reads every solid of a STEP file and says what happened to each one.
///
/// Bounds. Size: `bytes` over [`MAX_STEP_BYTES`] is refused before parsing;
/// the reader caps entity, record, parameter, face, loop and solid counts
/// itself and allocates nothing from a count read out of the file. Time:
/// `stop` is the time bound - whoever calls cancels it (from a watchdog
/// thread or a timer) and the import ends with [`KernelError::TimedOut`].
/// It is polled by the parser, between the stages and solids here, and
/// inside the kernel's own loops (heal, integration). The reader's topology
/// pass has no cancellation hook at the pinned commit: it runs to its end,
/// bounded by the caps above.
///
/// A panic inside the reader or the kernel is caught and reported as a
/// failure; the kernel state is then replaced by a fresh one, as it is
/// after a time-out (a cancelled or unwound call may leave caches half
/// built).
pub fn read_step(
    k: &mut SolidcoreKernel,
    bytes: &[u8],
    stop: &CancelToken,
) -> Result<StepRead, KernelError> {
    if bytes.is_empty() {
        return Err(refused_input("empty file".into()));
    }
    if bytes.len() > MAX_STEP_BYTES {
        return Err(refused_input(format!(
            "{} bytes is past the {MAX_STEP_BYTES}-byte limit",
            bytes.len()
        )));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|e| refused_input(format!("not valid UTF-8 text ({e})")))?;
    if stop.is_cancelled() {
        return Err(timed_out("before parsing"));
    }
    let before = k.k.cancel.replace(stop.clone());
    let guarded = catch_unwind(AssertUnwindSafe(|| read_guarded(k, text, stop)));
    k.k.cancel = before;
    let out = match guarded {
        Ok(r) => r,
        Err(_) => Err(KernelError::Failed {
            kernel: NAME,
            op: "read_step",
            detail: "the reader or the kernel panicked on this file".into(),
        }),
    };
    if matches!(
        out,
        Err(KernelError::TimedOut { .. } | KernelError::Failed { .. })
    ) {
        k.k = solidcore::Kernel::default();
    }
    out
}

fn read_guarded(
    k: &mut SolidcoreKernel,
    text: &str,
    stop: &CancelToken,
) -> Result<StepRead, KernelError> {
    let ex = p21::parse_with(text, Some(&Watch(stop))).map_err(|e| match e {
        p21::Error::Cancelled => timed_out("parsing"),
        other => refused_input(other.to_string()),
    })?;
    let mut out = StepRead {
        length_units_mm: length_units(&ex).map_err(refused_input)?,
        ..StepRead::default()
    };
    if !ex.duplicates.is_empty() {
        out.notes.push(format!(
            "{} entity id(s) defined more than once; the first definition is kept",
            ex.duplicates.len()
        ));
    }
    if out.length_units_mm.len() > 1 {
        out.notes.push(format!(
            "the file states {} length units ({:?} mm per unit); each solid is read in its own",
            out.length_units_mm.len(),
            out.length_units_mm
        ));
    }
    let read = topology::read_solids(&ex);
    if stop.is_cancelled() {
        return Err(timed_out("reading topology"));
    }
    for f in read.failures {
        out.refused.push(StepRefusal {
            item: f.item,
            stage: "read",
            reason: f.message,
        });
    }
    for s in read.solids {
        if stop.is_cancelled() {
            return Err(timed_out("building solids"));
        }
        let item = s.item;
        match build(k, s) {
            Ok(solid) => out.solids.push(solid),
            Err(_) if stop.is_cancelled() => return Err(timed_out("building solids")),
            Err((stage, reason)) => out.refused.push(StepRefusal {
                item,
                stage,
                reason,
            }),
        }
    }
    out.refused.sort_by_key(|r| r.item);
    Ok(out)
}

/// The distinct length units the file's representation contexts state, in
/// millimetres per unit. A context whose unit list names no length unit
/// states none. `Err` names a length unit the reader cannot convert: it
/// would read such a unit as millimetres without a word (its units module
/// says so), and a guessed scale is the silent wrong answer, so the seam
/// refuses the file as crates/app's `step_units` does (#306).
fn length_units(ex: &Exchange) -> Result<Vec<f64>, String> {
    let mut found: Vec<f64> = Vec::new();
    for (id, e) in &ex.entities {
        let Some(list) = e
            .record("GLOBAL_UNIT_ASSIGNED_CONTEXT")
            .and_then(|r| r.first())
            .and_then(p21::Param::as_list)
        else {
            continue;
        };
        let mut states_length = false;
        for u in list.iter().filter_map(p21::Param::as_ref_id) {
            let Some(unit) = ex.get(u).filter(|x| x.has("LENGTH_UNIT")) else {
                continue;
            };
            states_length = true;
            // An SI length is a (prefixed) metre; anything else must be a
            // conversion the reader can follow to one.
            let si_metre = unit
                .record("SI_UNIT")
                .map(|si| si.get(1).and_then(p21::Param::as_enum) == Some("METRE"));
            let sized = match (si_metre, units::unit_factor(ex, u)) {
                (Some(false), _) => None,
                (None, _) if unit.record("CONVERSION_BASED_UNIT").is_none() => None,
                (_, Some((units::UnitKind::Length, f))) if f.is_finite() && f > 0.0 => Some(f),
                _ => None,
            };
            if sized.is_none() {
                return Err(format!("length unit #{u} cannot be converted to millimetres"));
            }
        }
        if !states_length {
            continue;
        }
        let len = units::context(ex, *id).units.len;
        if !found.iter().any(|f| f.to_bits() == len.to_bits()) {
            found.push(len);
        }
    }
    Ok(found)
}

/// One record solid as a checked kernel solid. `Err` is `(stage, reason)`.
fn build(k: &mut SolidcoreKernel, s: topology::Solid) -> Result<StepSolid, (&'static str, String)> {
    if s.sheet {
        return Err((
            "read",
            "a surface model of open shells, not a closed solid".into(),
        ));
    }
    // Untrusted: the record comes from a file, so the kernel heals it as it
    // builds it (the conversion's own rule for records not made by a
    // modeller that keeps the kernel's rules).
    let mut brep =
        to_kernel_body(&mut k.k, &s.brep, false).map_err(|e| ("build", e.to_string()))?;
    // Finding at the pinned commit (bench, tests/step_agreement.rs): the
    // build leaves the port's cached `Face::shell` behind its face-use
    // links on every fixture, which the integrity check below refuses. The
    // port's own operations rewrite those caches before they return; the
    // conversion does not, so the seam does (it changes the cached fields
    // only, never a result).
    brep.refresh_cached_topology();
    let mut brep = checked("read_step", brep).map_err(|e| ("check", e.to_string()))?;
    let measure = k.measure(&mut brep).map_err(|e| ("measure", e.to_string()))?;
    let area = brep_area(&k.k, &mut brep, VOLUME_ACCURACY)
        .map_err(|e| ("measure", failed("read_step", &e).to_string()))?;
    if !(area.is_finite() && area > 0.0) {
        return Err(("measure", format!("not a positive finite area {area}")));
    }
    // The same law the native importer keeps (docs/decisions/27, point 3):
    // a vertex off its edges by more than the limit is not a model a
    // toolpath may trust, and an edge with no 3D curve cannot be judged.
    let gaps = vertex_gaps(k, &brep).map_err(|e| ("check", e.to_string()))?;
    if gaps.unjudged_edges > 0 {
        return Err((
            "check",
            format!(
                "{} edge(s) without a 3D curve: gaps cannot be judged",
                gaps.unjudged_edges
            ),
        ));
    }
    if gaps.gap > HEAL_GAP_LIMIT {
        return Err((
            "check",
            format!(
                "a vertex sits {:.3e} mm off its edges (limit {HEAL_GAP_LIMIT:e})",
                gaps.gap
            ),
        ));
    }
    Ok(StepSolid {
        item: s.item,
        name: s.name,
        brep,
        record: s.brep,
        measure,
        area,
        vertex_gap: gaps.gap,
        notes: s.notes,
    })
}

/// A STEP file imported whole: every solid item of the file is a solid.
#[derive(Debug)]
pub struct StepImport {
    /// The file's solids, in millimetres.
    pub solids: Vec<StepSolid>,
    /// See [`StepRead::length_units_mm`].
    pub length_units_mm: Vec<f64>,
    /// File-level diagnostics.
    pub notes: Vec<String>,
}

/// The importer: [`read_step`], and a model only when EVERY solid item of
/// the file came through. A file that reads only in part is refused by
/// name, never handed out with a body missing - a toolpath planned against
/// a part with a solid left out cuts through it.
pub fn import_step(
    k: &mut SolidcoreKernel,
    bytes: &[u8],
    stop: &CancelToken,
) -> Result<StepImport, KernelError> {
    let read = read_step(k, bytes, stop)?;
    if let Some(first) = read.refused.first() {
        return Err(KernelError::InvalidResult {
            kernel: NAME,
            op: "import_step",
            detail: format!(
                "{} of {} solid item(s) refused; first #{} at {}: {}",
                read.refused.len(),
                read.refused.len().saturating_add(read.solids.len()),
                first.item,
                first.stage,
                first.reason
            ),
        });
    }
    if read.solids.is_empty() {
        return Err(KernelError::InvalidResult {
            kernel: NAME,
            op: "import_step",
            detail: "the file holds no solid".into(),
        });
    }
    Ok(StepImport {
        solids: read.solids,
        length_units_mm: read.length_units_mm,
        notes: read.notes,
    })
}
