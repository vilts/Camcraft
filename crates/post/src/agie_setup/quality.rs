//! The work qualities that a zero-step piece can request, from the quality presets of our
//! own implementation and our offline Agie Control checks (#2954, #2984, #2966).

use super::QN2_OFFSETS_UM;

/// The one quality object of a piece. Its name identifies the requested
/// values, so that no generic name binds other resident data (#2984).
/// The control preselects the trims for the quality; the piece arms nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    /// Fine Ra 0.30 µm, Te 1.50 µm, Tkm 1.00 µm, at every height in
    /// [`QN2_OFFSETS_UM`]. Every punch, die, group and cutoff uses it.
    #[default]
    Ccb030,
    /// Ra 0.35 µm, Te 1.50 µm, Tkm 1.00 µm with the speed option set, for one
    /// closed punch on a 20 mm plate only (#2966). See [`CCB035_OFFSETS_UM`].
    Ccb035,
}

/// The TECCUT technology that the control preselects for CCB035. The "Dis"
/// column of TECCUT is a match distance that changes with the request, so the
/// technology name is the stable key (#2966, control check of 25 Nov).
pub const CCB035_TECHNOLOGY: &str = "estbca25nsn060b035020";

/// Wire-centre offsets in µm of the main cut and the four trims on a
/// straight outside face for quality CCB035. With the speed option set, the
/// control showed the TECCUT header `TYPE:VS`, preselected
/// [`CCB035_TECHNOLOGY`] and plotted these five passes on a 20 mm Berco Cut
/// 0.25 punch in steel 0001 (#2966). No other height is measured.
pub const CCB035_OFFSETS_UM: [(u16, [u16; 5]); 1] = [(20, [234, 189, 154, 139, 129])];

impl Quality {
    /// The native quality name that the piece creates.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Ccb030 => "CCB030",
            Self::Ccb035 => "CCB035",
        }
    }

    /// The requested Ra in µm, as the control displays it.
    #[must_use]
    pub fn ra(self) -> &'static str {
        match self {
            Self::Ccb030 => "0.30",
            Self::Ccb035 => "0.35",
        }
    }

    /// The six `JE_CreateQuality` values: Ra, Te and Tkm in µm and three
    /// options. The first option is the speed flag ("Snelheid" on the control).
    pub(super) fn request(self) -> String {
        let speed = match self {
            Self::Ccb030 => "F",
            Self::Ccb035 => "T",
        };
        format!("{},1.50,1.00,{speed},F,F", self.ra())
    }

    /// The measured stock thicknesses in mm, in table order.
    #[must_use]
    pub fn thicknesses_mm(self) -> Vec<u16> {
        match self {
            Self::Ccb030 => QN2_OFFSETS_UM.iter().map(|(mm, _)| *mm).collect(),
            Self::Ccb035 => CCB035_OFFSETS_UM.iter().map(|(mm, _)| *mm).collect(),
        }
    }

    /// The measured offsets of all passes for a stock thickness in µm, main
    /// cut first, if it is qualified. CCB030 has 4 passes and CCB035 has 5.
    #[must_use]
    pub fn offsets_um(self, height_um: i32) -> Option<&'static [u16]> {
        let at = |mm: &u16| i32::from(*mm) * 1000 == height_um;
        match self {
            Self::Ccb030 => {
                let table: &'static [(u16, [u16; 4])] = &QN2_OFFSETS_UM;
                table.iter().find(|(mm, _)| at(mm)).map(|(_, o)| &o[..])
            }
            Self::Ccb035 => {
                let table: &'static [(u16, [u16; 5])] = &CCB035_OFFSETS_UM;
                table.iter().find(|(mm, _)| at(mm)).map(|(_, o)| &o[..])
            }
        }
    }

    /// The measured main-cut (rough) offset in µm for a stock thickness in µm.
    #[must_use]
    pub fn rough_um(self, height_um: i32) -> Option<u16> {
        self.offsets_um(height_um)?.first().copied()
    }

    /// The operator text for the requested options. The CCB030 text is that of #2984.
    pub(super) fn options_text(self) -> &'static str {
        match self {
            Self::Ccb030 => "three disabled options",
            Self::Ccb035 => "the speed option set, the other two options disabled",
        }
    }

    /// The operator text for the TECCUT step. The CCB030 text is that of #2984.
    pub(super) fn teccut_text(self) -> String {
        match self {
            Self::Ccb030 => "Assign the matching TECCUT row.".into(),
            Self::Ccb035 => format!(
                "TECCUT must show the header TYPE:VS and preselect technology {CCB035_TECHNOLOGY} \
                 with 5 passes. If not, stop. Assign it."
            ),
        }
    }
}
