//! Split from `machining3d.rs` by M5.0.1; structure only.

use super::params::Pocket3DParams;
use super::rest::EPS;
use super::feed::{guard_input, home, sim_of, subtract_upright};
use super::hsm::Seam;
use super::ladder::ladder;

use crate::cl::Toolpath;
use crate::geo2d::Region;
use crate::geometry::Triangle;
use crate::machining2d::{
    cut_levels, GenError,
};
use crate::stock_sim::{Columns, StockReport};
/// The per-level cutter plan and its walk, level after level - the 3-D
/// sibling of `machining2d::pocket`'s run, with the passes recomputed
/// from the model at every z (that is the whole difference: 2-D knows
/// its plan once, 3-D's plan IS the ladder).
pub fn pocket3d(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
) -> Result<Toolpath, GenError> {
    run_plain(facets, footprint, p, false, None)
}

/// The re-machining sibling (M3.1 D): the same ladder, run against the
/// stock state the previous operations left (`mask: Some`, see the
/// app crate's `rest`). Every masked level target is a subset of the
/// unmasked one, so a first operation - or a program with nothing cut
/// before it - generates the identical path.
pub fn pocket3d_prev(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
    mask: Option<&Columns<'_>>,
) -> Result<Toolpath, GenError> {
    run_plain(facets, footprint, p, false, mask)
}

/// The offset-shell twin (M2.5 F): roughing by OFFSET SHELLS of the
/// model, not the level staircase's flat rings. The ladder stays
/// whole - the same zs planes, the same per-level islands and their
/// XY erosion, the same entry arms, links, superset laws and
/// Empty-by-name refusals - and what changes is the Z every CUT
/// vertex lands on: `min(plane, roof(q, r + ℓ) + leave)`, floored at
/// `max(plane - stepdown, bottom_z)`, where `roof` is the model's
/// highest nearby facet top (`machining3d::shell`). Every XY leg is
/// subdivided to
/// chords of at most ℓ - the tool radius - so each vertex's decision
/// disc bounds both the leg it starts and the one it ends: the
/// staircase's pyramid between plane and surface is cut away in the
/// pass itself, while the steep flanks where the roof outgrows the
/// plane stay exactly the waterline the plain ladder cut there. On a
/// prismatic model every roof query answers at or above the plane
/// and the shells collapse onto the planes themselves - the
/// prismatic equivalence test measures that, move for move. The
/// axial hop per pass never exceeds `stepdown`, the same bite a
/// plain ladder's dive already takes, and the M2.3 simulation still
/// holds the closing verdict.
pub fn pocket3d_offset(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
) -> Result<Toolpath, GenError> {
    run_plain(facets, footprint, p, true, None)
}

/// The re-machining sibling of [`pocket3d_offset`] (same M3.1 D law
/// as [`pocket3d_prev`]).
pub fn pocket3d_offset_prev(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
    mask: Option<&Columns<'_>>,
) -> Result<Toolpath, GenError> {
    run_plain(facets, footprint, p, true, mask)
}

fn run_plain(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
    shell: bool,
    mask: Option<&Columns<'_>>,
) -> Result<Toolpath, GenError> {
    guard_input(facets, footprint, p)?;
    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown)?;
    ladder(facets, footprint, p, &zs, None, mask, home(p), shell, None)
}

/// The plain ladder with a second, smaller footprint for its AIR
/// levels (M13.8, the rotary wrap's seam law): every plane strictly
/// above `air_above` - where no part material can stand, so the
/// level's target is its whole footprint - is cleared over
/// `air_footprint`; every plane at or below it over `footprint`, exactly
/// as [`pocket3d`] would. The planes are the one `cut_levels` ladder
/// either way, and the second leg starts where the first ended (the
/// rest ladder's chaining law), so the program differs from
/// [`pocket3d`]'s only in what the air levels sweep. With no plane
/// above `air_above` it IS [`pocket3d`], move for move.
pub fn pocket3d_air_split(
    facets: &[Triangle],
    air_footprint: &Region,
    footprint: &Region,
    air_above: f64,
    seam: Option<(f64, f64)>,
    p: &Pocket3DParams,
) -> Result<Toolpath, GenError> {
    guard_input(facets, footprint, p)?;
    // Under Adaptive every developed level goes to the HSM planner
    // (issue #9, lifting M11.4's pin); `seam` - (period, reach) - names
    // the levels whose area the wrap's seam cuts, each leg reading its
    // own cover's ends. OffsetRings and every other strategy run as
    // they always did.
    let seam_of = |fp: &Region| -> Option<Seam> {
        let (period, reach) = seam?;
        let (lo, hi) = fp.bounds()?;
        Some(Seam { lo: lo.x, hi: hi.x, period, reach })
    };
    let (sm_air, sm_cut) = (seam_of(air_footprint), seam_of(footprint));
    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown)?;
    let k = air_levels(&zs, air_above);
    if k == 0 || air_footprint.is_empty() {
        return ladder(facets, footprint, p, &zs, None, None, home(p), false, sm_cut.as_ref());
    }
    let air = ladder(facets, air_footprint, p, &zs[..k], None, None, home(p), false, sm_air.as_ref())?;
    if k == zs.len() {
        return Ok(air);
    }
    let start = air.moves.last().map_or_else(|| home(p), |m| m.to);
    let mut moves = air.moves;
    let mut notes = air.notes;
    match ladder(facets, footprint, p, &zs[k..], None, None, start, false, sm_cut.as_ref()) {
        Ok(rest) => {
            moves.extend(rest.moves);
            notes.extend(rest.notes);
        }
        // The material planes found nothing to cut: the air leg is the
        // whole job (the single ladder would have said the same).
        Err(GenError::Empty(_)) => {}
        // #1015: the air leg stands in front of the material leg's ghost.
        Err(e) => return Err(e.after_moves(&moves)),
    }
    let path = Toolpath {
        moves,
        cycles: Vec::new(),
        notes,
        comps: Vec::new(),
        threads: Vec::new(),
        probes: Vec::new(),
        breaks: Vec::new(),
        plasma: None,
    };
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// How many of the ladder's planes `zs` (top down) stand above
/// `air_above` (issue #55): a plane within [`EPS`] of it touches the
/// part's leave envelope, so it is a material plane, not an air one.
/// The bare `>` sorted the plane AT the envelope by the last bit of the
/// wrap's own radius arithmetic (6.500000000000001 against 6.5 on the
/// r7 bar), so the same work could sweep one lap or two by float noise.
pub(super) fn air_levels(zs: &[f64], air_above: f64) -> usize {
    zs.iter().take_while(|z| **z > air_above + EPS).count()
}

/// The rest-material sibling (M2.1b): the same level ladder, then a
/// second, half-stepdown pass aimed at what the simulation says is
/// still standing. The coarse ladder's staircase is the acknowledged
/// price of planes - at most `stepdown` above a raised surface - and
/// this closes it toward stepdown/2 per round the caller dares to run.
/// Entries here are always plain plunges, and the superset law is why:
/// a column the coarse ladder cleared at z is clear at every plane
/// above z, so a vertical dive into rest material can never cross part
/// geometry. The report is the sim's verdict on the COMBINED path,
/// measured against the mesh's own surface.
pub fn pocket3d_with_rest(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
) -> Result<(Toolpath, StockReport), GenError> {
    run_rest(facets, footprint, p, false, None)
}

/// The offset-shell twin of [`pocket3d_with_rest`]: shells on the
/// coarse leg, shells on the rest leg, and the sim's verdict between
/// them as always - the rest ladder's staircase closing half the
/// stepdown is exactly what the shell's normal-eroded center costs
/// less of (M2.5 F).
pub fn pocket3d_offset_with_rest(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
) -> Result<(Toolpath, StockReport), GenError> {
    run_rest(facets, footprint, p, true, None)
}

/// The re-machining sibling of [`pocket3d_offset_with_rest`].
pub fn pocket3d_offset_with_rest_prev(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
    mask: Option<&Columns<'_>>,
) -> Result<(Toolpath, StockReport), GenError> {
    run_rest(facets, footprint, p, true, mask)
}

/// The re-machining sibling of [`pocket3d_with_rest`] (M3.1 D): the
/// coarse ladder already skips what the program ahead machined, the
/// rest leg chases what even that left standing, and the closing
/// report reads the whole program's stock, not this operation's
/// private copy of it.
pub fn pocket3d_with_rest_prev(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
    mask: Option<&Columns<'_>>,
) -> Result<(Toolpath, StockReport), GenError> {
    run_rest(facets, footprint, p, false, mask)
}

fn run_rest(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
    shell: bool,
    mask: Option<&Columns<'_>>,
) -> Result<(Toolpath, StockReport), GenError> {
    guard_input(facets, footprint, p)?;
    let r = p.tool_diameter * 0.5;
    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown)?;
    let coarse = ladder(facets, footprint, p, &zs, None, mask, home(p), shell, None)?;

    // What the coarse ladder leaves standing, column by column: the
    // op-frame grid simulated from raw stock, or - when the program
    // cut before us - the document grid with our moves added to its
    // history (one grid, one story; M3.1 D).
    let dex = match mask {
        Some(prev) => {
            let mut d = prev.to_stock();
            subtract_upright(&mut d, &coarse.moves, r)?;
            d
        }
        None => {
            let (mut d, _) = sim_of(facets, footprint, p)?;
            subtract_upright(&mut d, &coarse.moves, r)?;
            d
        }
    };
    let rest = dex.columns()?;

    // The rest ladder: same laws, half the step, rest-shaped targets.
    let zs_r = cut_levels(p.top_z, p.bottom_z, p.stepdown * 0.5)?;
    let start = coarse.moves.last().map_or_else(|| home(p), |m| m.to);
    let mut moves = coarse.moves;
    // The coarse leg's notes join the rest leg's below, in program
    // order (M8.1): the status line reads the substitutions as the
    // machine lived them.
    let mut notes = coarse.notes;
    match ladder(facets, footprint, p, &zs_r, Some(&rest), None, start, shell, None) {
        Ok(rp) => {
            moves.extend(rp.moves);
            notes.extend(rp.notes);
        }
        // Nothing stood above any rest plane: the coarse path was
        // already clean at lattice scale. A success, not a refusal.
        Err(GenError::Empty(_)) => {}
        // #1015: the coarse ladder stands in front of the rest leg's ghost.
        Err(e) => return Err(e.after_moves(&moves)),
    }
    let path = Toolpath {
        moves,
        cycles: Vec::new(),
        notes,
        comps: Vec::new(),
        threads: Vec::new(),
        probes: Vec::new(),
        breaks: Vec::new(),
        plasma: None,
    };
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }

    // The verdict on the whole job: re-run the sim from where this
    // operation started - raw stock, or the program's standing state
    // - over the combined path, so the report describes the machining,
    // not one of its legs.
    let mut dex = match mask {
        Some(prev) => prev.to_stock(),
        None => sim_of(facets, footprint, p)?.0,
    };
    subtract_upright(&mut dex, &path.moves, r)?;
    // `Stock::measure` samples the model on the stock's own lattice -
    // the part surface this verdict always measured against.
    Ok((path, dex.measure(facets, footprint, EPS)))
}
