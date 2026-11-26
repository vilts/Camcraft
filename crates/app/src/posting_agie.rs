//! Native Agie job dispatch and posting preflight. Provenance: CAMCraft's
//! verified Wire Contour workflow and the local Agie controller job contract.

use std::collections::BTreeMap;
use std::path::Path;

use flecs_ecs::prelude::World;

use super::{Posted, mounts};
use crate::post::{cnc, options::OptionValue};
use crate::schema::{PostOptions, PostRef};
use crate::model;

#[path = "posting_agie_group.rs"]
mod group;

pub(crate) use camcraft_post::agie::{AgieJob, AgieSide, MAX_PUNCH_WORKS, Quality};
pub use camcraft_post::agie::valid_name;

/// The registry name of the native wire EDM post, the one place it is spelled (#3537).
pub const POST: &str = "Native wire EDM job files";
/// #3537: the name this post had before; a hidden alias that no list shows.
/// A saved project, an option key `builtin:<name>`, a script or an MCP call
/// that still says it selects [`POST`].
pub const LEGACY_POST: &str = "Agie native wire EDM";
const MODEL: &str = "AGIECUT CLASSIC 2S";
pub(crate) const PREVIEW_GAP: f64 = 0.005;

/// The work quality that a Wire Contour machine condition requests (#2966).
/// `AGIE_QN2` requests CCB030 (Ra 0.30 µm) for every qualified job.
/// `AGIE_BERCO_RA035` requests CCB035 (Ra 0.35 µm) for one 20 mm polygon punch.
pub(crate) fn quality_of(condition: &str) -> Option<Quality> {
    match condition {
        "AGIE_QN2" => Some(Quality::Ccb030),
        "AGIE_BERCO_RA035" => Some(Quality::Ccb035),
        _ => None,
    }
}

/// The machine condition that requests `quality`: the inverse of [`quality_of`].
fn condition_name(quality: Quality) -> &'static str {
    match quality {
        Quality::Ccb030 => "AGIE_QN2",
        Quality::Ccb035 => "AGIE_BERCO_RA035",
    }
}

/// The registry name that `name` selects: [`POST`] for its hidden alias, else `name`.
#[must_use]
pub fn current_name(name: &str) -> &str {
    if name == LEGACY_POST { POST } else { name }
}

/// `post` with a hidden alias name replaced by the registry name it selects.
#[must_use]
pub fn current_ref(post: PostRef) -> PostRef {
    match post {
        PostRef::Named(name) if name == LEGACY_POST => PostRef::Named(POST.to_owned()),
        other => other,
    }
}

/// An option key `builtin:<alias>` rewritten to the key of the registry name.
#[must_use]
pub fn current_key(key: &str) -> String {
    match key.strip_prefix("builtin:") {
        Some(name) if name == LEGACY_POST => format!("builtin:{POST}"),
        _ => key.to_owned(),
    }
}

pub fn is_native(post: &PostRef) -> bool {
    matches!(post, PostRef::Named(name) if current_name(name) == POST)
}

pub(super) fn post(
    world: &World,
    dest: &Path,
    extra: Option<&BTreeMap<String, OptionValue>>,
) -> Result<Posted, String> {
    let mount = mounts::single(world, false)?;
    let Some(machine) = mount.machine.1.as_ref() else {
        return Err(format!("The native wire EDM job requires the {MODEL} machine profile"));
    };
    if machine.model != MODEL
        || crate::machine::kind(Some(machine)) != crate::machine::Kind::WireEdm
    {
        return Err(format!("The native wire EDM job requires the {MODEL} machine profile"));
    }
    if let Some(axis) = machine.axes.iter().find(|axis| {
        axis.travel.is_some()
            && (axis.name.eq_ignore_ascii_case("X") || axis.name.eq_ignore_ascii_case("Y"))
    }) {
        return Err(format!(
            "The native wire EDM post cannot verify the declared {} travel against the control's resident rough and trim offsets at the installed work datum",
            axis.name
        ));
    }
    let name = dest.file_name().and_then(|x| x.to_str()).filter(|s| valid_name(s))
        .ok_or("Choose a new native wire EDM job directory named with 1 to 8 uppercase letters or digits, starting with a letter")?;
    let ops = super::program_ops(world)?;
    if !(1..=MAX_PUNCH_WORKS).contains(&ops.len()) {
        return Err(format!("A native wire EDM job requires one active Wire Contour operation, 2 to {MAX_PUNCH_WORKS} for a group of captured punches, or two for the openings of one die plate"));
    }
    let operations = ops
        .iter()
        .map(|(name, entity)| {
            model::operation_of(world, entity)
                .map(|op| (name.as_str(), op))
                .ok_or("The Wire Contour operation is unavailable")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (_, op) = operations
        .first()
        .ok_or("A native wire EDM job requires an active Wire Contour operation")?;
    let setup = op.setup.as_deref();
    if operations
        .iter()
        .any(|(_, other)| other.setup != op.setup || other.tool != op.tool)
    {
        return Err("The works of a group must use the same setup and wire tool".into());
    }
    if crate::setup_post_options::of(world, setup) != PostOptions::default() {
        return Err("Native wire EDM jobs require default post options and One program output".into());
    }
    if crate::units::of(world) != crate::units::Unit::Millimeters {
        return Err("Native wire EDM jobs require a millimetre document".into());
    }
    let info = super::program_info(world);
    if info.number.is_some() || info.comment.is_some() {
        return Err(
            "The native wire EDM job uses the job directory name; clear the NC program number and comment".into(),
        );
    }

    // Use the ordinary post's current operation order, freshness, warning
    // records, setup laws and verification. The wire centres in the stream
    // are only verification evidence; the writer receives the nominal ring.
    for (op_name, records) in super::op_streams_by(world, false)? {
        for record in records {
            if let cnc::Record::Warn { text } = record {
                return Err(format!("Native wire EDM post blocked - {op_name}: {text}"));
            }
        }
    }
    let report = super::verification_report(world);
    if !report.problems.is_empty() {
        return Err(format!(
            "Native wire EDM post blocked - {}",
            report.problems.join("; ")
        ));
    }
    // One selected DXF circle, racetrack (#2022) or convex tangent loop (#2983)
    // posts native arcs. A group reads modeled punches or the openings of one
    // modeled die plate.
    let round = match operations.as_slice() {
        [(op_name, op)] if crate::operation::wire_sketch::selected(&op.src, &op.params) => {
            Some(crate::operation::agie_sketch_job(world, op, op_name)?)
        }
        _ => None,
    };
    let jobs = if round.is_some() { Vec::new() } else { group::jobs(world, &operations)? };
    // The control checked CCB035 on one 20 mm polygon punch only (#2966).
    let qualities = operations
        .iter()
        .map(|(_, op)| crate::operation::agie_quality(world, op))
        .collect::<Result<Vec<_>, _>>()?;
    // The piece registers one quality for all its works (#2984). Name a mixed
    // group in either work order before the single-punch limit of CCB035.
    let mut conditions: Vec<_> = qualities.iter().map(|q| condition_name(*q)).collect();
    conditions.sort_unstable();
    conditions.dedup();
    if conditions.len() > 1 {
        return Err(format!(
            "The works of a group must use the same wire condition. This group mixes {}",
            conditions.join(" and ")
        ));
    }
    let quality = match qualities.as_slice() {
        [quality] if round.is_none() => *quality,
        _ if qualities.iter().all(|q| *q == Quality::Ccb030) => Quality::Ccb030,
        _ => {
            return Err("AGIE_BERCO_RA035 is qualified for one closed polygon punch on a 20 mm plate only. Select AGIE_QN2 for a punch group or a DXF arc punch".into());
        }
    };
    let options = super::program_options_for_script(
        world,
        super::builtin_post_text(POST)?,
        Some(&PostRef::Named(POST.into())),
        setup,
        extra,
    )?;
    let separation = options
        .get("separationLengthMm")
        .and_then(|v| v.as_f64())
        .ok_or("The native wire EDM post must declare its separation cut length in millimetres")?;
    let (bundle, works) = match &round {
        Some((job, shape)) => {
            (camcraft_post::agie::export_round_with_review(job, shape, dest, name, separation)?, 1)
        }
        None => match (quality, jobs.as_slice()) {
            (Quality::Ccb035, [job]) => (
                camcraft_post::agie::export_punch_with_review(
                    job, quality, dest, name, separation,
                )?,
                1,
            ),
            _ => (
                camcraft_post::agie::export_group_with_review(&jobs, dest, name, separation)?,
                jobs.len(),
            ),
        },
    };
    Ok(Posted {
        bundle_files: bundle.files,
        controller_review: Some(bundle.controller_review),
        ops: works,
        lines: 0,
        program: String::new(),
        line_pos: Vec::new(),
        blocks: Vec::new(),
        not_judged: report.not_judged,
    })
}
