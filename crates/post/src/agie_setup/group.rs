//! Setup-only group of two to nine captured punches or two die openings on one 20 mm plate, from
//! the work format of our own implementation and CamCraft's offline control checks of #2961, #2977 and #2980.

use super::{SetupError, SetupFiles, WorkSide, piece, qn2_offsets_um, resources, valid_stem};

/// The stock thickness of the control-checked punch group (#2961).
pub const GROUP_HEIGHT_UM: i32 = 20_000;

/// The largest control-checked punch group (#2980). Each ISO stem adds one
/// digit to the group name, so nine works use the digits 1 to 9.
pub const MAX_PUNCH_WORKS: usize = 9;

/// The number of openings in the control-checked die group (#2977).
const DIE_GROUP_WORKS: usize = 2;

/// One work of a group (a captured punch or a die opening): its own ISO
/// geometry and its own start hole.
#[derive(Debug, Clone, Copy)]
pub struct GroupPunch<'a> {
    /// DOS stem of this work's ISO geometry (for example, `PAIR201`).
    pub iso_stem: &'a str,
    /// Centre of this work's 2 mm threading hole [X, Y], in µm.
    pub start_hole_um: [i32; 2],
}

/// Setup coordinates for a group of closed punches, each with its own ISO and
/// start hole, on one stock. The works are `CUT1`, `CUT2` and so on in list
/// order. All works refer to the one quality object of the piece (#2984).
/// XY coordinates are integer micrometres in the same work frame as the ISOs.
///
/// The caller must verify each nominal contour and lead as for
/// [`super::PunchSetup20`]. It must also verify that the complete cutting
/// areas of the punches (solid, hole, lead, rough corner and wire) stay apart.
#[derive(Debug)]
pub struct PunchGroup20<'a> {
    /// DOS stem of the SBR, the SBL and the native group (for example, `PAIR20`).
    pub stem: &'a str,
    /// Lower stock limits [X, Y].
    pub stock_min_um: [i32; 2],
    /// Upper stock limits [X, Y].
    pub stock_max_um: [i32; 2],
    /// Stock thickness below the stock top Z0. Must be [`GROUP_HEIGHT_UM`].
    pub height_um: i32,
    /// The punch works, in cut order.
    pub punches: &'a [GroupPunch<'a>],
}

impl PunchGroup20<'_> {
    /// Encode one native group with one punch work for each entry, each with
    /// zero erosion steps on import.
    ///
    /// # Errors
    /// Refuses invalid or repeated stems, stock bounds and threading-hole
    /// positions, any thickness other than 20 mm, and any number of works
    /// outside 2 to [`MAX_PUNCH_WORKS`].
    pub fn encode(&self) -> Result<SetupFiles, SetupError> {
        self.encode_side(WorkSide::Punch)
    }

    /// Encode one native group with one die-opening work (`ID_PUNCH,c_die`)
    /// for each entry, with zero erosion steps on import (#2977). Each entry
    /// is one through-opening of the same retained plate. Apart from the side
    /// and the operator review, the records are those of [`Self::encode`].
    ///
    /// # Errors
    /// The errors of [`Self::encode`], and any number of openings other than
    /// two.
    pub fn encode_die(&self) -> Result<SetupFiles, SetupError> {
        self.encode_side(WorkSide::Die)
    }

    fn encode_side(&self, side: WorkSide) -> Result<SetupFiles, SetupError> {
        if !valid_stem(self.stem) {
            return Err(SetupError::InvalidStem);
        }
        if self.height_um != GROUP_HEIGHT_UM {
            return Err(SetupError::UnqualifiedGroupHeight);
        }
        let offsets = qn2_offsets_um(self.height_um).ok_or(SetupError::UnqualifiedHeight)?;
        let works = match side {
            WorkSide::Die => DIE_GROUP_WORKS..=DIE_GROUP_WORKS,
            _ => 2..=MAX_PUNCH_WORKS,
        };
        if !works.contains(&self.punches.len()) {
            return Err(SetupError::GroupWorks);
        }
        for (i, punch) in self.punches.iter().enumerate() {
            if !valid_stem(punch.iso_stem) {
                return Err(SetupError::InvalidStem);
            }
            if self.punches[..i]
                .iter()
                .any(|other| other.iso_stem == punch.iso_stem)
            {
                return Err(SetupError::GroupWorks);
            }
        }
        for (axis, stroke_um) in [350_000, 250_000].into_iter().enumerate() {
            let min = i64::from(self.stock_min_um[axis]);
            let max = i64::from(self.stock_max_um[axis]);
            if min >= max || max.max(0) - min.min(0) > stroke_um {
                return Err(SetupError::InvalidStock);
            }
            for punch in self.punches {
                let hole = i64::from(punch.start_hole_um[axis]);
                if hole - 1000 < min || hole + 1000 > max {
                    return Err(SetupError::HoleOutsideStock);
                }
            }
        }
        let piece = piece::Piece {
            stem: self.stem,
            stock_min_um: self.stock_min_um,
            stock_max_um: self.stock_max_um,
            height_um: self.height_um,
            quality: super::Quality::Ccb030,
        };
        let works: Vec<_> = self
            .punches
            .iter()
            .map(|punch| piece::Work {
                geometry: punch.iso_stem,
                side: side.native(),
                start_um: punch.start_hole_um,
                exit_um: None,
            })
            .collect();
        let names = works
            .iter()
            .enumerate()
            .map(|(i, work)| format!("CUT{} {}.ISO", i + 1, work.geometry))
            .collect::<Vec<_>>()
            .join(", ");
        let [rough, trim1, trim2, trim3] = offsets;
        // The punch text is unchanged. A die group names its side, keeps each
        // offset inside its own opening and supports each slug (#2977).
        let side_check = match side {
            WorkSide::Die => concat!(
                "Confirm c_die on each work and that each offset is inside its own opening. ",
                "Confirm that the main cut, the separation and the trims of each work stay inside ",
                "its opening, and that no cutting move crosses the plate between the openings. ",
                "Support each slug before the control releases its separation section.",
            ),
            _ => concat!(
                "Confirm c_punch on each work, and that no cut or threading move of one work enters ",
                "another punch. Support each punch before the control releases its separation section.",
            ),
        };
        Ok(SetupFiles {
            sbl: piece::piece(&piece, &works),
            sbr: resources(&works),
            controller_review: format!(
                concat!(
                    "Import each ISO into ISO Sys and keep the SBR beside the SBL during script import. ",
                    "Select the newly created piece. Earlier pieces can retain their steps on repeat import. ",
                    "Match the ISO resource, geometry and start hole of each work ({names}) to the setup. ",
                    "Confirm zero steps on each work. ",
                    "Verify steel 0001, Berco Cut 0.25 and {height} mm. CCB030 requests Ra 0.30, Te 1.50, ",
                    "Tkm 1.00 and three disabled options. Check the effective values and both visible options ",
                    "on each work. Assign the matching TECCUT row to each work. Check offsets of {rough}, ",
                    "{trim1}, {trim2} and {trim3} µm in order on each work before GRAFICHECK and FORMALCHECK. ",
                    "{side_check}",
                ),
                names = names,
                height = self.height_um / 1000,
                rough = rough,
                trim1 = trim1,
                trim2 = trim2,
                trim3 = trim3,
                side_check = side_check,
            ),
        })
    }
}
