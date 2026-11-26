//! STEP import on the solidcore side of the seam (M12.1 stage 4, #916):
//! file bytes -> solids in millimetres + the file's length units +
//! diagnostics, with a size bound and a time bound. Since #917 this is the
//! app's only STEP model reader (through `camcraft_geometry::step`).
//! Provenance: calls the public API of solidcore-step (the ISO 10303-21
//! exchange structure and ISO 10303-42 geometry and topology reader of the
//! solidcore workspace, Apache-2.0, written from those public standards),
//! solidcore-convert (`brep::to_kernel_body`) and solidcore (the Apache-2.0
//! port of NVIDIA's SMLib: cancel token, area integrator), with the pcurves
//! that have no closed form made by [`crate::solidcore_trims`], as their rustdoc
//! documents them, at the commit pinned in Cargo.toml. No reader internals
//! are reimplemented here and no HSMWorks material was consulted.

use std::panic::{AssertUnwindSafe, catch_unwind};

use solidcore::kernel::CancelToken;
use solidcore::topology::Brep;
use solidcore_step::p21::{self, Progress};
use solidcore_step::topology;

use crate::solidcore_impl::SolidcoreKernel;
use crate::solidcore_trims::KernelTrims;
use crate::step_edge_gap::{EdgeGap, EdgeGapCap, raised_edge_gap};
use crate::{KernelError, Measure};

mod record;
pub(crate) use record::{RecordSource, build_record};

pub use crate::solidcore_step_import::{StepImport, StepModel, StepSheet, import_step, import_step_model, import_step_model_assuming};

pub(crate) const NAME: &str = "solidcore";

/// Why a surface model is not a solid.
pub const SHEET: &str = "a surface model of open shells, not a closed solid";

/// Largest STEP file the seam hands the reader (64 MiB, the native reader's
/// bound). The reader's own cap is 512 MiB; it holds the text and every
/// parsed entity in memory, so the seam refuses earlier, by name.
pub const MAX_STEP_BYTES: usize = 64 << 20;

/// One checked solid of a STEP file, in millimetres.
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
    /// Surface area (mm^2), the port's estimate at [`crate::solidcore_impl::VOLUME_ACCURACY`].
    pub area: f64,
    /// Largest vertex-to-edge-end distance left in the solid (mm), always
    /// at or under [`crate::solidcore_import::HEAL_GAP_LIMIT`].
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
    /// The millimetres per unit that the caller stated for a file whose
    /// contexts state no length unit (#1874, [`read_step_assuming`]), when
    /// the reader used it. `None`: the file states its own unit, or no
    /// unit was stated. A stated unit is also in `length_units_mm`.
    pub assumed_mm: Option<f64>,
    /// File-level diagnostics (duplicate entity ids, mixed units, faces
    /// wound against their neighbours).
    pub notes: Vec<String>,
    /// The surface models (open shells only): read, not built as solids.
    pub sheets: Vec<StepSheet>,
    /// The file's edge-gap cap and the widest gap kept (#2976).
    pub edge_gap: EdgeGap,
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
    read_step_assuming(k, bytes, stop, None)
}

/// [`read_step`], and `assume_mm` millimetres per unit for a file whose
/// contexts state no length unit (#1874): the caller's unit is written into
/// the file's unit contexts before the read, so the whole model reads in
/// it. A file that states its own unit keeps it; `assume_mm` is then not
/// used. `None` reads a file with no unit at 1 mm per unit, as before.
pub fn read_step_assuming(
    k: &mut SolidcoreKernel,
    bytes: &[u8],
    stop: &CancelToken,
    assume_mm: Option<f64>,
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
    let guarded = catch_unwind(AssertUnwindSafe(|| read_guarded(k, text, stop, assume_mm)));
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
    assume_mm: Option<f64>,
) -> Result<StepRead, KernelError> {
    let mut ex = p21::parse_with(text, Some(&Watch(stop))).map_err(|e| match e {
        p21::Error::Cancelled => timed_out("parsing"),
        other => refused_input(other.to_string()),
    })?;
    // Validation and topology must read the same canonical unit records.
    crate::step_units_guard::expand_short_names(&mut ex);
    crate::step_units_guard::check_angle_units(&ex).map_err(refused_input)?;
    crate::step_curve_guard::check(&ex).map_err(refused_input)?;
    let mut units = crate::step_length_units::read(&ex).map_err(refused_input)?;
    let mut assumed_mm = None;
    if units.mm.is_empty()
        && let Some(mm) = assume_mm
    {
        crate::step_assumed_unit::assume(&mut ex, mm).map_err(refused_input)?;
        units = crate::step_length_units::read(&ex).map_err(refused_input)?;
        assumed_mm = Some(mm);
    }
    let mut out = StepRead {
        length_units_mm: units.mm.clone(),
        assumed_mm,
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
    out.notes.extend(crate::step_face_winding::notes(&ex, stop).map_err(|_| timed_out("checking face winding"))?);
    // The pcurves with no closed form (a free-form edge on a spline face)
    // are made by the kernel, each checked by its lift (#917), each edge
    // held to the cap that the file's stated tolerance sets (#2976).
    let cap = EdgeGapCap::read(&ex);
    let (read, measured) = {
        let trims = KernelTrims::with_cap(&mut k.k, cap);
        let read = topology::read_solids_with(&ex, &trims);
        (read, trims.widest_gap_mm())
    };
    if stop.is_cancelled() {
        return Err(timed_out("reading topology"));
    }
    // Never a silent widening: an edge kept over the cap of a file that
    // states no tolerance is noted with its gap and the file's tolerance.
    if measured > crate::step_edge_gap::MAX_EDGE_GAP_MM {
        out.notes.push(format!(
            "an edge lies {measured:.4} mm off its face's surface; the file states a tolerance of {:.4} mm, so the reader accepts up to {:.4} mm",
            cap.stated_mm.unwrap_or(0.0),
            cap.cap_mm
        ));
    }
    let raised = read.solids.iter().map(|s| raised_edge_gap(&s.brep)).fold(0.0, f64::max);
    out.edge_gap = EdgeGap {
        cap,
        widest_mm: measured.max(raised),
    };
    // An item with no unit context reads its angles in radians (#3079).
    let unlisted = crate::step_units_guard::UnlistedAngles::new(&ex);
    let mut read = read;
    read.solids.retain(|s| match unlisted.refusal(s.item) {
        Some(reason) => {
            out.refused.push(StepRefusal { item: s.item, stage: "read", reason });
            false
        }
        None => true,
    });
    for f in read.failures {
        out.refused.push(StepRefusal {
            item: f.item,
            stage: "read",
            // The wrong angle unit is the first cause of what then failed.
            reason: unlisted.refusal(f.item).unwrap_or(f.message),
        });
    }
    for s in read.solids {
        if stop.is_cancelled() {
            return Err(timed_out("building solids"));
        }
        let item = s.item;
        if let Some(reason) = units.unstated(item) {
            out.refused.push(StepRefusal {
                item,
                stage: "read",
                reason,
            });
            continue;
        }
        if s.sheet {
            out.sheets.push(StepSheet { item, name: s.name, record: s.brep, notes: s.notes });
            continue;
        }
        match build_record(k, RecordSource::Step(item), s.name, s.brep, s.notes) {
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
