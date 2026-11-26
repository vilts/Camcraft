//! Build and validate imported solids with the public solidcore-convert and solidcore B-rep APIs.

use solidcore::mass::integrator::properties::brep_area;
use solidcore::topology::Brep;
use solidcore::topology::brep::validation::{CheckLevel, check_brep};
use solidcore_convert::brep::{to_kernel, to_kernel_body};
use solidcore_record::brep::Brep as Record;

use super::StepSolid;
use crate::solidcore_impl::{SolidcoreKernel, VOLUME_ACCURACY, checked, failed};
use crate::solidcore_import::{EdgeEndpoint, HEAL_GAP_LIMIT, vertex_gaps_with_edge};
use crate::{Kernel, Measure};

type Refusal = (&'static str, String);

pub(crate) enum RecordSource {
    Step(u64),
    Solidcraft,
}

struct Candidate {
    brep: Brep,
    gap: f64,
    worst_edge: Option<EdgeEndpoint>,
}

/// Both candidate bodies must pass the same checks. The converter leaves the
/// face-shell cache behind the face-use links; refresh it before checking (#916).
fn check_topology(k: &SolidcoreKernel, mut brep: Brep) -> Result<Candidate, Refusal> {
    brep.refresh_cached_topology();
    let brep = checked("read_step", brep).map_err(|e| ("check", e.to_string()))?;
    // The healer can enlarge vertex tolerances without moving a vertex.
    // Keep measuring the physical gap (RECOMMENDED.md, #27, stage 3, point 3).
    let (gaps, worst_edge) =
        vertex_gaps_with_edge(k, &brep).map_err(|e| ("check", e.to_string()))?;
    if gaps.unjudged_edges > 0 {
        return Err((
            "check",
            format!(
                "{} edge(s) without a 3D curve: gaps cannot be judged",
                gaps.unjudged_edges
            ),
        ));
    }
    Ok(Candidate {
        brep,
        gap: gaps.gap,
        worst_edge,
    })
}

/// Integrate only the selected solid, after its endpoint and validity checks.
fn measure_body(k: &mut SolidcoreKernel, brep: &mut Brep) -> Result<(Measure, f64), Refusal> {
    let measure = k.measure(brep).map_err(|e| ("measure", e.to_string()))?;
    let area = brep_area(&k.k, brep, VOLUME_ACCURACY)
        .map_err(|e| ("measure", failed("read_step", &e).to_string()))?;
    if !(area.is_finite() && area > 0.0) {
        return Err(("measure", format!("not a positive finite area {area}")));
    }
    Ok((measure, area))
}

/// Inspect the original topology only after healing removed edges and failed
/// the endpoint limit. Skipping the converter's healer here only constructs
/// the candidate. Its full kernel validity report gates its use (#3252).
fn check_source_topology(
    k: &mut SolidcoreKernel,
    record: &Record,
    healed: Candidate,
) -> Result<(Candidate, String), Refusal> {
    let healed_edges = healed.brep.edge_list().len();
    let healed_gap = healed.gap;
    let healed_summary =
        format!("after healing: {healed_edges} edges, maximum gap {healed_gap:.3e} mm");
    // The rejected body can be large. Keep only its measurements for the retry.
    drop(healed);
    let source_error = |(stage, why): Refusal| {
        (
            stage,
            format!(
                "{healed_summary} (limit {HEAL_GAP_LIMIT:e} mm); could not check the source topology: {why}"
            ),
        )
    };
    k.k.check_cancelled("STEP source recovery")
        .map_err(|e| ("check", e.to_string()))?;
    // The STEP reader exports no source entity IDs for edges. The converter
    // explicitly maps each input-record edge index to the unhealed edge ID.
    let (brep, order) =
        to_kernel(&mut k.k, record, true).map_err(|e| source_error(("build", e.to_string())))?;
    let mut source = check_topology(k, brep).map_err(source_error)?;
    let source_edges = source.brep.edge_list().len();
    if source.gap > HEAL_GAP_LIMIT {
        let location = source.worst_edge.and_then(|worst| {
            let index = order.edges.iter().position(|&e| e == worst.edge)?;
            let vertex = worst.vertex;
            Some(format!(
                "Input-record edge index {index} (zero-based), vertex ({:.6}, {:.6}, {:.6}) mm.",
                vertex.x, vertex.y, vertex.z
            ))
        });
        let location = location
            .as_deref()
            .unwrap_or("Could not identify the input-record edge.");
        return Err((
            "check",
            format!(
                "The source endpoint gap is {} mm, above the {HEAL_GAP_LIMIT} mm limit. {location} \
                 Before healing: {source_edges} edges; after healing: {healed_edges} edges, maximum gap {} mm.",
                over_limit_gap_mm(source.gap),
                over_limit_gap_mm(healed_gap)
            ),
        ));
    }
    let comparison = format!(
        "before healing: {source_edges} edges, maximum vertex-to-endpoint gap {:.3e} mm; {healed_summary}",
        source.gap
    );
    k.k.check_cancelled("STEP source validation")
        .map_err(|e| ("check", e.to_string()))?;
    let validity = check_brep(&mut k.k, &mut source.brep, CheckLevel::Level2).map_err(|e| {
        (
            "check",
            format!("{comparison}; source validation failed: {e}"),
        )
    })?;
    if let Some(reason) = validity.lines().first() {
        return Err((
            "check",
            format!("{comparison}; source validation failed: {reason}"),
        ));
    }
    Ok((source, comparison))
}

/// Keep a small excess visible even when six decimal places would hide it.
fn over_limit_gap_mm(gap: f64) -> String {
    let rounded = format!("{gap:.6}");
    if rounded == format!("{HEAL_GAP_LIMIT:.6}") {
        gap.to_string()
    } else {
        rounded
    }
}

/// A reader record in mm, measured and checked before the mesher gets it.
pub(crate) fn build_record(
    k: &mut SolidcoreKernel,
    source: RecordSource,
    name: String,
    record: Record,
    mut notes: Vec<String>,
) -> Result<StepSolid, Refusal> {
    // A file is untrusted. Run the converter's ordinary healing pass first.
    let brep = to_kernel_body(&mut k.k, &record, false).map_err(|e| ("build", e.to_string()))?;
    let mut body = check_topology(k, brep)?;
    let mut recovered = None;
    if body.gap > HEAL_GAP_LIMIT {
        if !matches!(source, RecordSource::Step(_))
            || body.brep.edge_list().len() >= record.edges.len()
        {
            return Err((
                "check",
                format!(
                    "a vertex sits {:.3e} mm off its edges (limit {HEAL_GAP_LIMIT:e})",
                    body.gap
                ),
            ));
        }
        let (original, comparison) = check_source_topology(k, &record, body)?;
        body = original;
        recovered = Some(comparison);
    }
    k.k.check_cancelled("import solid measurement")
        .map_err(|e| ("measure", e.to_string()))?;
    let (measure, area) =
        measure_body(k, &mut body.brep).map_err(|(stage, why)| match &recovered {
            Some(comparison) => (
                stage,
                format!("{comparison}; source measurement failed: {why}"),
            ),
            None => (stage, why),
        })?;
    if let Some(comparison) = recovered {
        notes.push(format!("Kept the original STEP edges. {comparison}."));
    }
    Ok(StepSolid {
        item: match source {
            RecordSource::Step(item) => item,
            RecordSource::Solidcraft => 0,
        },
        name,
        brep: body.brep,
        record,
        measure,
        area,
        vertex_gap: body.gap,
        notes,
    })
}
