//! Face rows kept clear of "Do not touch" items (#3343): each level's rows are cut short at a cutter-centre keep-out, and a row-end step that crosses it becomes a link.
//! Provenance: our own interval clipping of straight rows against a planar region (the facer's `face_holes` row law); no vendor material consulted.

use std::borrow::Cow;

use crate::geo2d::{Loop, Point2, Region};

use super::face_holes;
use super::gen_error::GenError;

/// The cutter-centre keep-out that a Face reads at each level (#3343).
pub struct FaceKeep<'a> {
    /// The protected items, named for a note or a refusal.
    pub names: String,
    /// The keep-out at cut level `z`, in the facer's own frame: each tool
    /// centre whose tool, at any tip height from `z` up to the clearance
    /// plane, comes nearer to a protected item than the clearance.
    pub at: &'a dyn Fn(f64) -> Result<Region, GenError>,
}

/// One level's rows, cut short at the keep-out.
pub(super) struct Level<'p> {
    pub pieces: Cow<'p, [Vec<(f64, f64)>]>,
    /// Where the level is entered from the clearance plane.
    pub start: (f64, f64),
    keepout: Region,
}

impl Level<'_> {
    /// True when the row-end step at `x` from row `y0` to row `y1` stays
    /// out of the keep-out. The test box holds the sharp step and the
    /// blended semicircle on either side of it.
    pub fn step_clear(&self, x: f64, y0: f64, y1: f64) -> bool {
        if self.keepout.is_empty() {
            return true;
        }
        let h = (y1 - y0).abs() * 0.5;
        let (lo, hi) = (y0.min(y1), y0.max(y1));
        let pts = vec![
            Point2::new(x - h, lo),
            Point2::new(x + h, lo),
            Point2::new(x + h, hi),
            Point2::new(x - h, hi),
        ];
        let Ok(boxed) = Loop::new(pts).and_then(|l| Region::new(vec![l])) else {
            return false;
        };
        self.keepout.intersect(&boxed).is_ok_and(|i| i.is_empty())
    }
}

/// What the keep-out did over all levels, for the path's notes.
#[derive(Default)]
pub(super) struct Tally {
    /// Levels with at least one row piece (each one is cut).
    pub levels: usize,
    blocked: usize,
    clipped: bool,
    /// The lowest level that is cut (#3526): the depth the face reaches.
    reached: Option<f64>,
    /// The lowest level asked for: Cut Z.
    floor: Option<f64>,
}

impl Tally {
    /// Count level `z` as cut.
    fn cut(&mut self, z: f64) {
        self.levels += 1;
        self.reached = Some(self.reached.map_or(z, |r| r.min(z)));
    }

    /// The notes of the keep-out, or the refusal when no level is cut.
    pub fn finish(&self, keep: Option<&FaceKeep<'_>>) -> Result<Vec<String>, GenError> {
        let Some(k) = keep else {
            return Ok(Vec::new());
        };
        if self.levels == 0 {
            return Err(GenError::Invalid(format!(
                "cannot face clear of protected 'Do not touch' {}: no safe facing room remains",
                k.names
            )));
        }
        // #3526: a blocked level leaves the whole face above Cut Z. No
        // path state says "incomplete", so the cut is refused by name,
        // with the depth that the face would reach.
        if self.blocked > 0 {
            // Both are set once a level is cut (`levels > 0` above).
            let z = |v: Option<f64>| v.map_or_else(|| "?".to_owned(), |v| format!("{v:.3}"));
            let (reached, floor) = (z(self.reached), z(self.floor));
            return Err(GenError::Invalid(format!(
                "cannot face down to Cut Z {floor} clear of protected 'Do not touch' {}: {} lower facing level(s) have no safe room, so the face would stop at Z {reached} - set Cut Z to {reached}, use a smaller clearance, or remove the item",
                k.names, self.blocked
            )));
        }
        let mut notes = Vec::new();
        if self.clipped {
            notes.push(format!(
                "Do not touch ({}) cut the facing rows short beside the protected items; the material there stays unfaced",
                k.names
            ));
        }
        Ok(notes)
    }
}

/// The rows of level `z`: `pieces` unchanged where the keep-out is empty
/// (entered at `entry`, the facer's own start), else each piece cut short
/// at the keep-out and the level entered at its first piece. `None` when
/// the keep-out takes every row of the level.
pub(super) fn level<'p>(
    keep: Option<&FaceKeep<'_>>,
    z: f64,
    rows: &[(f64, f64, f64)],
    pieces: &'p [Vec<(f64, f64)>],
    entry: (f64, f64),
    tally: &mut Tally,
) -> Result<Option<Level<'p>>, GenError> {
    let keepout = match keep {
        Some(k) => (k.at)(z)?,
        None => Region::empty(),
    };
    tally.floor = Some(tally.floor.map_or(z, |f| f.min(z)));
    if keepout.is_empty() {
        tally.cut(z);
        return Ok(Some(Level { pieces: Cow::Borrowed(pieces), start: entry, keepout }));
    }
    let cut: Vec<Vec<(f64, f64)>> = rows
        .iter()
        .zip(pieces)
        .map(|(&(y, _, _), ps)| {
            ps.iter().flat_map(|&(x0, x1)| face_holes::pieces(&keepout, y, x0, x1)).collect()
        })
        .collect();
    let start = rows.iter().zip(&cut).find_map(|(&(y, _, _), ps)| ps.first().map(|&(x, _)| (x, y)));
    let Some(start) = start else {
        tally.blocked += 1;
        return Ok(None);
    };
    if cut.as_slice() != pieces {
        tally.clipped = true;
    }
    tally.cut(z);
    Ok(Some(Level { pieces: Cow::Owned(cut), start, keepout }))
}
