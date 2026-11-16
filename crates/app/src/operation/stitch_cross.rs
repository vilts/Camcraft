//! Open-run stitching across model entities (M13.11, the M1.5 leftover:
//! "two runs meeting across two model entities do not compose into one
//! ring"). Two parts butted edge to edge - a plate and the block it
//! sits on, a model split into bodies - carry one rim between them, and
//! a user who picks its edges on both sees one loop. After the
//! per-entity walk of `stitch` has taken every ring one entity closes
//! on its own, the runs it left are walked again here, open to every
//! picked model, welded on the same lattice, oriented by the same
//! shoelace law, and committed as one ring exactly as the per-entity
//! case is.
//!
//! Two things a single entity never had to answer are refused by name:
//! a junction three runs leave from (the picks spell two rings, and a
//! guess gouges), and a ring whose runs stand on different planes in
//! the walk's frame (the planar kinds machine one plane; a ring that
//! steps between two is not their boundary).
//!
//! Provenance: our own design notes. No vendor material consulted.

use super::*;

/// The cross-entity pass over the runs the per-entity walk left: runs
/// `used` are skipped, every closed walk becomes one more ring in
/// `rings` with its picks marked `consumed`. Nothing happens unless the
/// picked open runs span more than one entity - a selection on one
/// model stitches exactly as it always did, byte for byte.
pub(super) fn stitch_across(
    runs: &[Run],
    rings: &mut Rings,
    consumed: &mut HashMap<usize, usize>,
) -> Result<(), GenError> {
    let Some(first) = runs.first() else {
        return Ok(());
    };
    if runs.iter().all(|r| r.entity == first.entity) {
        return Ok(());
    }
    for s in 0..runs.len() {
        if run_used(&runs[s], consumed) {
            continue;
        }
        let Some(seq) = walk_ring(runs, s, consumed, true)? else {
            continue;
        };
        planar_check(runs, &seq)?;
        let pts = compose_ring(runs, &seq, rings.len(), consumed);
        let names = entities_of(runs, seq.iter().map(|(j, _)| *j));
        rings.push((format!("{STITCHED_RIM} {}", name_list(&names)), pts, seq.len()));
    }
    Ok(())
}

/// The planar law: every point of the composed ring stands within one
/// weld step of one plane in the walk's frame, or the ring is refused
/// by name with the two heights it straddles.
fn planar_check(runs: &[Run], seq: &[(usize, bool)]) -> Result<(), GenError> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for (j, _) in seq {
        let Some(r) = runs.get(*j) else { continue };
        for &z in &r.zs {
            lo = lo.min(z);
            hi = hi.max(z);
        }
    }
    if !lo.is_finite() || !hi.is_finite() || hi - lo <= WELD_TOL {
        return Ok(());
    }
    let names = entities_of(runs, seq.iter().map(|(j, _)| *j));
    Err(GenError::Invalid(format!(
        "the picked edges of {} meet end to end but lie on different planes (Z {lo:.3} to {hi:.3}) - a stitched ring must be planar; pick the edges on one plane, or machine each model on its own",
        name_list(&names)
    )))
}

/// The fork refusal: the runs at the junction `at` (indices into `runs`,
/// the arriving run and the walk's anchor included) meet at `far`, and
/// no rule picks a way on.
///
/// It names no spot (#677): the stitch runs while the job is collected,
/// in the walk's (tilted) frame, before `run_job`'s frame map - a spot
/// from here would be drawn in the wrong frame.
pub(super) fn fork_refusal(runs: &[Run], at: &[usize], far: (i64, i64)) -> GenError {
    let names = entities_of(runs, at.iter().copied());
    let ways = at.len();
    GenError::Invalid(format!(
        "the picked edges of {} meet {ways} ways at ({:.3}, {:.3}) - a ring across models must meet end to end, one way on; remove the stray edge, or pick one model's loop",
        name_list(&names),
        far.0 as f64 / 1e6,
        far.1 as f64 / 1e6,
    ))
}

/// The distinct entity names behind `ixs`, in first-seen order.
fn entities_of(runs: &[Run], ixs: impl Iterator<Item = usize>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for j in ixs {
        if let Some(r) = runs.get(j)
            && !out.contains(&r.entity)
        {
            out.push(r.entity.clone());
        }
    }
    out
}

/// `'a'`, `'a' and 'b'`, `'a', 'b' and 'c'`.
fn name_list(names: &[String]) -> String {
    let quoted: Vec<String> = names.iter().map(|n| format!("'{n}'")).collect();
    match quoted.len() {
        0 => String::new(),
        1 => quoted[0].clone(),
        n => format!("{} and {}", quoted[..n - 1].join(", "), quoted[n - 1]),
    }
}
