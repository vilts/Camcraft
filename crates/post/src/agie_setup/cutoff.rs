//! Setup-only straight open cut on a 20 mm plate, from our own implementation of the open-work format
//! and CamCraft's offline control checks of #2962.

use super::{SetupError, SetupFiles, WorkSide, piece, qn2_offsets_um, setup_files, valid_stem};

/// The stock thickness of the control-checked open cut (#2962).
pub const CUTOFF_HEIGHT_UM: i32 = 20_000;

/// The kerf side of the directed open cut, seen along the ISO line from its first point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutoffSide {
    /// `ID_PUNCH,c_left`: the control offsets the wire to the left of the line.
    Left,
    /// `ID_PUNCH,c_right`: the control offsets the wire to the right of the line.
    Right,
}

/// Setup coordinates for one straight open cut with two air endpoints.
/// XY coordinates are integer micrometres in the same work frame as the ISO.
#[derive(Debug)]
pub struct CutoffSetup20<'a> {
    /// Common DOS file stem for the ISO, SBR and SBL (for example, `LOWER20`).
    pub stem: &'a str,
    /// Lower stock limits [X, Y].
    pub stock_min_um: [i32; 2],
    /// Upper stock limits [X, Y].
    pub stock_max_um: [i32; 2],
    /// The kerf side of the open cut.
    pub side: CutoffSide,
    /// The entry point `stp1`, in air before the first stock face.
    pub entry_um: [i32; 2],
    /// The exit point `stp2`, in air beyond the opposite stock face.
    pub exit_um: [i32; 2],
    /// Stock thickness below the stock top Z0. Must be [`CUTOFF_HEIGHT_UM`].
    pub height_um: i32,
}

impl CutoffSetup20<'_> {
    /// Encode one native open cut with zero erosion steps and no separation on import.
    ///
    /// The caller must verify the nominal line, its air tails and approaches,
    /// the compensated cuts against the stock corners and the installed
    /// travel, and that both pieces stay clamped through all passes.
    ///
    /// # Errors
    /// Refuses invalid file stems and stock bounds, any thickness other than
    /// 20 mm, and endpoints that are not in air beyond opposite stock faces.
    pub fn encode(&self) -> Result<SetupFiles, SetupError> {
        if !valid_stem(self.stem) {
            return Err(SetupError::InvalidStem);
        }
        if self.height_um != CUTOFF_HEIGHT_UM {
            return Err(SetupError::UnqualifiedCutoffHeight);
        }
        let offsets = qn2_offsets_um(self.height_um).ok_or(SetupError::UnqualifiedHeight)?;
        let mut opposite = false;
        for (axis, stroke_um) in [350_000, 250_000].into_iter().enumerate() {
            let min = i64::from(self.stock_min_um[axis]);
            let max = i64::from(self.stock_max_um[axis]);
            if min >= max || max.max(0) - min.min(0) > stroke_um {
                return Err(SetupError::InvalidStock);
            }
            let entry = i64::from(self.entry_um[axis]);
            let exit = i64::from(self.exit_um[axis]);
            let low = min.min(0).min(entry).min(exit);
            let high = max.max(0).max(entry).max(exit);
            if high - low > stroke_um {
                return Err(SetupError::CutoffEndpoints);
            }
            opposite |= (entry < min && exit > max) || (exit < min && entry > max);
        }
        if !opposite {
            return Err(SetupError::CutoffEndpoints);
        }
        let side = match self.side {
            CutoffSide::Left => WorkSide::Left,
            CutoffSide::Right => WorkSide::Right,
        };
        let piece = piece::Piece {
            stem: self.stem,
            stock_min_um: self.stock_min_um,
            stock_max_um: self.stock_max_um,
            height_um: self.height_um,
            quality: super::Quality::Ccb030,
        };
        let work = piece::Work {
            geometry: self.stem,
            side: side.native(),
            start_um: self.entry_um,
            exit_um: Some(self.exit_um),
        };
        let side_check = format!(
            concat!(
                " Confirm the {} side, no separation cut, the entry stp1 and the exit stp2 in air,",
                " and one stock crossing for each pass. Keep both pieces clamped through all passes.",
            ),
            side.native()
        );
        Ok(setup_files(
            &piece,
            &work,
            "geometry, entry point and exit point",
            &offsets,
            &side_check,
        ))
    }
}
