//! Generation job orchestration and the 3D Pocket reach preflight.
//! Provenance: standard axial tool-stack clearance; issue #1844.

use super::*;

/// Run a fully-collected job: every world read happened in
/// [`collect_job`], on the caller's thread, and nothing here names a
/// `World`. Batch jobs run on worker threads, where a `&World` cannot
/// exist. The reach preflight refuses unsupported depths before any
/// generator builds a path.
pub fn run_job(mut job: GenJob) -> Result<(Toolpath, Vec<Warning>), GenError> {
    // The spindle law's set-aside note (issue #71 review) rides the
    // path whichever arm cut it: a note, never a warning - the post
    // is not held for features the op honestly leaves standing.
    let note = job.spindle_note.take();
    let carried = job.carried_overhang.take();
    let tool_r = job.tool.diameter * 0.5;
    let kind = job.params.kind();
    // #261 part 2: a refusal's spot is in the frame the generators ran
    // in (a 3+2 tilt's local one); the UI draws in world, so it is
    // carried out through the same frame the path would have been.
    let frame = job.frame;
    // #835: a refusal naming the holder of a tool with a body says it
    // judged the cylinder enclosing that body - #842: only where it did,
    // a body whose band stack does not fit the record.
    let judged_tool = job.tool.clone();
    let body = job.body.clone().filter(|p| crate::operation::body_judged::falls_back(&judged_tool, p));
    let _span = tracing::info_span!("generate", op = %job.op_name).entered();
    let started = crate::clock::Instant::now();
    tracing::debug!(kind = %kind, "generation started");
    let (mut path, warns) = pocket_reach(&job).and_then(|()| run_job_arms(job))
        .map_err(|e| crate::operation::body_judged::name_enclosure(e, &judged_tool, body.as_ref()))
        .map_err(|e| e.map_location(|p| frame.map_or(p, |f| f.to_world(p))))
        // #261 part 3: a refusal's ghost rides the same frame map as a path.
        .map_err(|e| match &frame {
            Some(f) => e.map_ghost(|t| f.world_path(t, machining2d::ARC_FIT_TOL)),
            None => e,
        })
        .inspect_err(|e| {
            tracing::warn!(kind = %kind, ms = crate::logging::elapsed_ms(started), "generation refused: {e}");
        })?;
    tracing::info!(
        kind = %kind,
        moves = path.moves.len(),
        warnings = warns.len(),
        ms = crate::logging::elapsed_ms(started),
        "generated"
    );
    let carried = carried.map(|o| o.note(&path, tool_r));
    for n in note.into_iter().chain(carried) {
        if !path.notes.contains(&n) {
            path.notes.push(n);
        }
    }
    Ok((path, warns))
}

/// The ladder proves only cutter-radius clearance. Until it can widen
/// each level for the assembly, require every wider band to stay above
/// the material at Bottom Z. This is deliberately conservative: no XY
/// clearance or previously removed stock is credited by this preflight.
fn pocket_reach(job: &GenJob) -> Result<(), GenError> {
    if !matches!(job.params, OpParams::Pocket3D { .. }) || job.frame.is_some() {
        // Tilted whole-model pockets already refuse in open_footprint;
        // their existing diagnostic keeps precedence.
        return Ok(());
    }
    let bottom = req_z(job.hs.bottom, "Bottom of Part Z")?;
    let top = job.material.top.max(job.stock_aabb.map_or(job.material.top, |(_, hi)| hi.z));
    let depth = top - bottom;
    let geom = job.tool.geom();
    let stack = crate::tool_body::stack(&geom, job.body.as_ref()).with_neck(job.neck.clone());
    // Read exactly the bands used by verification, including its
    // enclosing-cylinder fallback for a body that does not fit.
    let cutter = verify::Cutter::of(&job.tool).with_body(&geom, &stack);
    for (name, bands) in [("shank", cutter.shank_bands()), ("holder", cutter.holder_bands())] {
        for (above, radius, len) in bands {
            if len > 0.0 && radius > cutter.radius && depth >= above {
                return Err(GenError::Invalid(format!(
                    "3D Pocket cannot prove {name} clearance for tool '{}': Bottom of Part Z is {:.3} mm below the material top, but its wider {name} starts {:.3} mm above the tip. Raise Bottom of Part Z above {:.3} mm or use a longer-reaching tool. This conservative reach check does not credit lateral clearance or previously removed stock.",
                    job.tool_name, depth, above, top - above,
                )));
            }
        }
    }
    Ok(())
}
