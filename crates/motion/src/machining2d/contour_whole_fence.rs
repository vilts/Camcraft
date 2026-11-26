//! Whole-model 2D contour (#2677): the bands are planned first and emitted
//! last, so that each band's lead room knows the rings that the other bands
//! cut on a level it cuts too. A grown ring that takes the levels above its
//! first level ([`super::contour_whole_reach::reach_up`]) can share a plane
//! with a ring of an earlier band, and a lead longer than the gap between the
//! two rings crosses the other ring at that plane (the verifier's `Crossing`
//! law). A thin fence around each such ring is added to the band's lead room,
//! so the lead fitter shortens, reseats or refuses the lead by name. Provenance:
//! polygon offset and union from textbook computational geometry, the planar
//! no-crossing rule of tool path verification; no vendor material consulted.

use crate::cl::PathBuilder;
use crate::geo2d::{Point2, Region};

use super::contour::{PreparedBand, emit_band};
use super::contour_depth::Ladder;
use super::contour_params::ContourParams;
use super::contour_step::Said;
use super::contour_whole_reach::band_sweep;
use super::face::GenError;
use super::fillet_tally::FilletTally;

const SAME_LEVEL: f64 = 1e-6;
/// Half the width (mm) of the fence around another band's ring. It is ten
/// times the lead fitter's largest slack (5 um) and wider than its 0.02 mm
/// sample step, so no sampled lead steps over it.
const FENCE_HALF_WIDTH: f64 = 0.05;
/// Half the width (mm) of the gap the fence keeps around the band's own
/// rings: a ring of another band that runs on one of them is a touch, not a
/// crossing, and a fence there would close the lead's own start.
const OWN_GAP: f64 = 2.0 * FENCE_HALF_WIDTH;

/// One band, planned and not yet emitted.
pub(super) struct Planned {
    pub band: PreparedBand,
    pub lad: Ladder,
    pub pick: Option<(usize, Point2)>,
    pub exit: Option<(usize, Point2)>,
}

/// The emit state a band writes to.
pub(super) struct Sink<'a> {
    pub said: &'a mut Said,
    pub lead_notes: &'a mut Vec<String>,
    pub tally: &'a mut FilletTally,
}

fn shares(a: &[f64], b: &[f64]) -> bool {
    a.iter().any(|&z| b.iter().any(|&w| (z - w).abs() <= SAME_LEVEL))
}

/// The lead room of band `i`: its center, with a fence around each ring of
/// another band that is cut on a level band `i` cuts. `None` when no other
/// band shares a level (the room is the center, as before #2677).
fn room(planned: &[Planned], i: usize) -> Result<Option<Region>, GenError> {
    let Some(me) = planned.get(i) else { return Ok(None) };
    let mut fence = Region::empty();
    for (j, other) in planned.iter().enumerate() {
        if j == i {
            continue;
        }
        for (k, l) in other.band.loops.iter().enumerate() {
            let levels = other.lad.per_ring.get(k).map_or(&[][..], |v| v.as_slice());
            if !shares(levels, &me.lad.zs) {
                continue;
            }
            let ring = Region::new(vec![l.clone()])?;
            fence = fence.union(&band_sweep(&ring, FENCE_HALF_WIDTH)?)?;
        }
    }
    if fence.is_empty() {
        return Ok(None);
    }
    let fence = fence.difference(&band_sweep(&me.band.center, OWN_GAP)?)?;
    Ok(Some(me.band.center.union(&fence)?))
}

/// Emit every planned band in order into `b`, each with its fenced lead room,
/// and empty `planned`.
pub(super) fn flush(
    b: &mut PathBuilder,
    planned: &mut Vec<Planned>,
    p: &ContourParams,
    sink: &mut Sink<'_>,
) -> Result<(), GenError> {
    let rooms = (0..planned.len()).map(|i| room(planned, i)).collect::<Result<Vec<_>, _>>()?;
    for (mut pl, room) in planned.drain(..).zip(rooms) {
        pl.band.room = room;
        emit_band(b, &pl.band, p, &pl.lad, pl.pick, pl.exit, sink.said, sink.lead_notes, sink.tally)?;
    }
    Ok(())
}
