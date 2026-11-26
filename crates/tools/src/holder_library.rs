//! The built-in holder library (#793, #792): steep-taper holders - BT30
//! and BT40 (JIS B 6339 / MAS 403 BT), CAT40 (ASME B5.50) - in ER collet
//! chuck (ISO 15488 / DIN 6499 collets and nuts) and Weldon side-lock
//! (DIN 1835 form B / DIN 6359 bores) versions, plus standalone
//! (straight-shank) ER11-ER40 collet chucks, plus the Tormach TTS
//! straight-shank holders (#794, `holder_library_tts.rs`), each drawn as a spun
//! profile (#93) the tool editor drops into a tool's holder. Specced
//! from the published standards' dimension tables (the numbers and
//! their sources are recorded in docs/decisions/793-taper-holders.md
//! and docs/decisions/792-er-collets.md); the outlines are our own
//! typical, conservative bodies, not any vendor's model.
//!
//! The laws here:
//! - **Never smaller than the steel.** The flange is drawn as a full
//!   cylinder at its outside diameter (the 60-degree gripper groove and
//!   the drive slots are enveloped, never cut away), the nut at its
//!   outside diameter over its whole length, and the chuck body as wide
//!   as its nut. The body-to-flange transition many holders flare
//!   through is enveloped by a 45-degree cone from the body out to the
//!   flange, ending at the flange's front face ([`FLARE`]); a Weldon
//!   body is drawn [`WELDON_SCREW`] wider than its catalog diameter for
//!   the clamp screw's head.
//! - **The profile stops at the gauge line.** The taper inside the
//!   spindle is never in reach of the work; the spindle housing the
//!   stack laws judge rides on the profile's top, which is the gauge
//!   line - where the spindle face is. A standalone chuck's gauge line
//!   is its body's back face, where its straight shank goes into the
//!   spindle or adapter.
//! - **Refusals by name**: an unknown entry, a gauge length too short
//!   to hold the nut and flange, a shank the collet or bore cannot
//!   take, a non-finite or negative number.

use crate::spun::SpunPoint;

/// One spindle mount: what stands between the gauge line and the tool
/// (mm). A steep taper (its V-flange), or a straight-shank system's
/// locating collar (TTS, #794: no standoff, the collar's top face is
/// the gauge line).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Taper {
    pub name: &'static str,
    /// What the editor calls the ring below the gauge line: "flange"
    /// or "collar".
    pub ring: &'static str,
    /// The taper's diameter at the gauge line.
    pub gauge_d: f64,
    /// The V-flange's outside diameter.
    pub flange_d: f64,
    /// From the gauge line to the flange's back (spindle-side) face.
    pub standoff: f64,
    /// The flange's thickness along the axis.
    pub flange_t: f64,
}

impl Taper {
    /// From the gauge line to the flange's front (tool-side) face.
    #[must_use]
    pub fn front_face(&self) -> f64 {
        self.standoff + self.flange_t
    }

    /// The radius of the taper between the flange and the gauge line:
    /// the gauge diameter grown by the 7/24 cone over the standoff, so
    /// it is never smaller than the cone it stands for.
    #[must_use]
    pub fn standoff_r(&self) -> f64 {
        (self.gauge_d + self.standoff * 7.0 / 24.0) * 0.5
    }
}

/// JIS B 6339 / MAS 403 BT30.
pub const BT30: Taper = Taper { name: "BT30", ring: "flange", gauge_d: 31.75, flange_d: 46.0, standoff: 2.0, flange_t: 20.0 };
/// JIS B 6339 / MAS 403 BT40.
pub const BT40: Taper = Taper { name: "BT40", ring: "flange", gauge_d: 44.45, flange_d: 63.0, standoff: 2.0, flange_t: 25.0 };
/// ASME B5.50 size 40 (CAT40, V-flange): 1.750 in gauge, 2.500 in
/// flange, 0.125 in standoff, 0.625 in flange.
pub const CAT40: Taper = Taper { name: "CAT40", ring: "flange", gauge_d: 44.45, flange_d: 63.5, standoff: 3.175, flange_t: 15.875 };

/// One ER collet size (ISO 15488) with its clamping nut (DIN 6499,
/// the standard round nut, not a slim "mini" nut).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ErSize {
    pub name: &'static str,
    /// The smallest and largest shank its collets clamp.
    pub min_bore: f64,
    pub max_bore: f64,
    /// The nut's outside diameter and length along the axis.
    pub nut_d: f64,
    pub nut_len: f64,
}

/// ER11: Ø0.5-7 collets, M14×0.75 nut; the round nut is drawn at the
/// top of its makers' Ø19-22 range.
pub const ER11: ErSize = ErSize { name: "ER11", min_bore: 0.5, max_bore: 7.0, nut_d: 22.0, nut_len: 11.5 };
pub const ER16: ErSize = ErSize { name: "ER16", min_bore: 0.5, max_bore: 10.0, nut_d: 32.0, nut_len: 17.5 };
pub const ER20: ErSize = ErSize { name: "ER20", min_bore: 1.0, max_bore: 13.0, nut_d: 35.0, nut_len: 19.5 };
pub const ER25: ErSize = ErSize { name: "ER25", min_bore: 1.0, max_bore: 16.0, nut_d: 42.0, nut_len: 20.0 };
pub const ER32: ErSize = ErSize { name: "ER32", min_bore: 2.0, max_bore: 20.0, nut_d: 50.0, nut_len: 22.5 };
/// ER40: Ø3-26 collets, M50×1.5 nut; Ø63 round nut at its longest
/// common length.
pub const ER40: ErSize = ErSize { name: "ER40", min_bore: 3.0, max_bore: 26.0, nut_d: 63.0, nut_len: 29.0 };

/// The one ER table every chuck in the library draws from - the
/// taper-mounted ones and the standalone ones alike.
pub const ER_SIZES: [ErSize; 6] = [ER11, ER16, ER20, ER25, ER32, ER40];

/// The radius (mm) a Weldon body is drawn wider than its catalog
/// diameter: the side-lock clamp screw's head can stand proud of the
/// body (a worn or substitute screw, a shallow counterbore).
pub const WELDON_SCREW: f64 = 1.0;
/// The slope of the envelope drawn over the body-to-flange flare: it
/// rises one radial mm per axial mm (45 degrees), from the body's
/// radius to the flange's, ending at the flange's front face.
pub const FLARE: f64 = 1.0;

/// How the holder grips the tool.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Clamp {
    /// An ER collet chuck: the nut on the nose, a body as wide as the
    /// nut up to the flange.
    Er(ErSize),
    /// A Weldon side-lock holder: a plain body of `body_d` around a
    /// `bore` for the tool's shank.
    Weldon { bore: f64, body_d: f64 },
}

/// One library entry: a taper (`None`: a standalone straight-shank
/// chuck), a clamp, a default gauge length (from the gauge line to the
/// holder's nose face).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HolderEntry {
    pub taper: Option<Taper>,
    pub clamp: Clamp,
    pub gauge_len: f64,
    /// The bore as the entry's name states it ("6 mm", "1/2 in").
    label: &'static str,
}

/// The nut, flange and body must leave at least this much (mm) of body
/// between them: a gauge length that does not is refused.
pub const MIN_BODY: f64 = 1.0;
/// The longest gauge length the library draws (mm).
pub const MAX_GAUGE: f64 = 500.0;
/// A Weldon bore takes a shank of its own size, within this (mm).
pub const BORE_SLACK: f64 = 0.02;

/// Why a library pick is refused. Each refusal names itself.
#[derive(Clone, Debug, PartialEq)]
pub enum LibraryError {
    Unknown(String),
    NonFinite,
    BelowTip,
    TooShort { min: f64 },
    TooLong,
    Shank { shank_d: f64, takes: String },
}

impl core::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unknown(n) => write!(f, "there is no holder '{n}' in the holder library"),
            Self::NonFinite => write!(f, "the gauge length and the projection must be finite numbers"),
            Self::BelowTip => write!(f, "the holder's nose cannot sit below the tool tip (projection < 0)"),
            Self::TooShort { min } => {
                write!(f, "the gauge length is shorter than the flange and nose need ({min:.2} mm at least)")
            }
            Self::TooLong => write!(f, "the gauge length is longer than {MAX_GAUGE} mm"),
            Self::Shank { shank_d, takes } => write!(f, "a Ø{shank_d:.3} mm shank does not fit - the holder takes {takes}"),
        }
    }
}

impl std::error::Error for LibraryError {}

impl HolderEntry {
    /// The name the editor lists and MCP takes: "BT30 ER20",
    /// "BT40 Weldon 12 mm", "CAT40 Weldon 1/2 in", "ER20 chuck".
    #[must_use]
    pub fn name(&self) -> String {
        match (self.taper, self.clamp) {
            (Some(t), Clamp::Er(er)) => format!("{} {}", t.name, er.name),
            (Some(t), Clamp::Weldon { .. }) => format!("{} Weldon {}", t.name, self.label),
            (None, Clamp::Er(er)) => format!("{} chuck", er.name),
            (None, Clamp::Weldon { .. }) => format!("Weldon {}", self.label),
        }
    }

    /// The radius the nose and body are drawn at: the nut's, or the
    /// Weldon body's plus [`WELDON_SCREW`].
    #[must_use]
    pub fn body_r(&self) -> f64 {
        match self.clamp {
            Clamp::Er(er) => er.nut_d * 0.5,
            Clamp::Weldon { body_d, .. } => body_d * 0.5 + WELDON_SCREW,
        }
    }

    /// How far (mm, along the axis) below the flange's front face the
    /// flare envelope starts: the radial rise from body to flange over
    /// the [`FLARE`] slope (0 without a flange, or a body as wide).
    #[must_use]
    pub fn flare_len(&self) -> f64 {
        self.taper.map_or(0.0, |t| (t.flange_d * 0.5 - self.body_r()).max(0.0) / FLARE)
    }

    /// The shortest gauge length that still holds the flange, the nose
    /// (nut or body), and between them the flare or [`MIN_BODY`] of
    /// body, whichever is longer.
    #[must_use]
    pub fn min_gauge(&self) -> f64 {
        let nose = match self.clamp {
            Clamp::Er(er) => er.nut_len,
            Clamp::Weldon { .. } => 0.0,
        };
        self.taper.map_or(0.0, |t| t.front_face()) + nose + MIN_BODY.max(self.flare_len())
    }

    /// Can the holder take a shank of `shank_d`? An ER chuck takes its
    /// collet range, a Weldon bore its own size.
    pub fn fits_shank(&self, shank_d: f64) -> Result<(), LibraryError> {
        let (ok, takes) = match self.clamp {
            Clamp::Er(er) => (
                shank_d >= er.min_bore - 1e-9 && shank_d <= er.max_bore + 1e-9,
                format!("{} collets, Ø{}-{} mm", er.name, er.min_bore, er.max_bore),
            ),
            Clamp::Weldon { bore, .. } => ((shank_d - bore).abs() <= BORE_SLACK, format!("a Ø{bore:.3} mm shank")),
        };
        if ok && shank_d.is_finite() { Ok(()) } else { Err(LibraryError::Shank { shank_d, takes }) }
    }

    /// The holder's half-outline (tip-up, as the spun profile reads it)
    /// with its nose face `nose` above the tool tip and the gauge line
    /// `gauge_len` above the nose: nose (nut or body), body up to the
    /// flare, the 45-degree flare envelope out to the flange's front
    /// face, the flange, the standoff, closed to the axis at the gauge
    /// line. A standalone chuck is its nut and body up to the gauge
    /// line.
    pub fn points(&self, nose: f64, gauge_len: f64) -> Result<Vec<SpunPoint>, LibraryError> {
        if !(nose.is_finite() && gauge_len.is_finite()) {
            return Err(LibraryError::NonFinite);
        }
        if nose < 0.0 {
            return Err(LibraryError::BelowTip);
        }
        let min = self.min_gauge();
        if gauge_len < min - 1e-9 {
            return Err(LibraryError::TooShort { min });
        }
        if gauge_len > MAX_GAUGE {
            return Err(LibraryError::TooLong);
        }
        let gauge = nose + gauge_len;
        let p = SpunPoint::line;
        let r = self.body_r();
        let mut pts = vec![p(nose, 0.0), p(nose, r)];
        if let Clamp::Er(er) = self.clamp {
            // The nut, then the body as wide as the nut: the nut's top
            // stays a point of its own so it can be edited.
            pts.push(p(nose + er.nut_len, r));
        }
        match self.taper {
            None => pts.extend([p(gauge, r), p(gauge, 0.0)]),
            Some(t) => {
                let front = gauge - t.front_face();
                let back = gauge - t.standoff;
                let fr = t.flange_d * 0.5;
                pts.extend([p(front - self.flare_len(), r), p(front, fr.max(r)), p(front, fr), p(back, fr)]);
                // A collar (no standoff) tops out at the gauge line.
                if t.standoff > 0.0 {
                    pts.extend([p(back, t.standoff_r()), p(gauge, t.standoff_r())]);
                }
                pts.push(p(gauge, 0.0));
            }
        }
        // A body as wide as the flange leaves a zero-length step.
        pts.dedup_by(|b, a| (a.z - b.z).hypot(a.r - b.r) < 1e-9);
        Ok(pts)
    }
}

/// A Weldon bore: its label, bore and typical body diameter (mm).
type Bore = (&'static str, f64, f64);

/// Metric Weldon bores (DIN 1835 form B shanks) with the body
/// diameters typical of DIN 6359 side-lock holders.
const METRIC_BORES: [Bore; 6] =
    [("6 mm", 6.0, 25.0), ("8 mm", 8.0, 28.0), ("10 mm", 10.0, 35.0), ("12 mm", 12.0, 42.0), ("16 mm", 16.0, 48.0), ("20 mm", 20.0, 52.0)];
/// Inch Weldon bores with the body diameters typical of inch end-mill
/// holders (1.00, 1.25, 1.375, 1.75, 1.75 in).
const INCH_BORES: [Bore; 5] = [
    ("1/4 in", 6.35, 25.4),
    ("3/8 in", 9.525, 31.75),
    ("1/2 in", 12.7, 34.925),
    ("5/8 in", 15.875, 44.45),
    ("3/4 in", 19.05, 44.45),
];

/// The standalone chucks' default gauge lengths (mm, nose face to the
/// body's back face), ER11 to ER40: typical straight-shank chuck heads,
/// rounded up - a longer body draws more steel, never less.
const STANDALONE_GAUGE: [f64; 6] = [30.0, 40.0, 45.0, 50.0, 55.0, 65.0];

/// Every built-in holder, in the editor's order: by taper, ER chucks
/// then Weldon holders.
#[must_use]
pub fn library() -> Vec<HolderEntry> {
    let er = |taper: Option<Taper>, er: &ErSize, gauge_len: f64| HolderEntry {
        taper,
        clamp: Clamp::Er(*er),
        gauge_len,
        label: "",
    };
    let sizes = |from: &str, to: &str| {
        let at = |n: &str| ER_SIZES.iter().position(|e| e.name == n).unwrap_or(0);
        ER_SIZES.get(at(from)..=at(to)).unwrap_or(&[]).iter()
    };
    let weldon = |taper: Taper, b: &Bore, gauge_len: f64| HolderEntry {
        taper: Some(taper),
        clamp: Clamp::Weldon { bore: b.1, body_d: b.2 },
        gauge_len,
        label: b.0,
    };
    let mut v = Vec::new();
    v.extend(sizes("ER16", "ER32").map(|e| er(Some(BT30), e, 60.0)));
    // BT30's 46 mm flange: the 16 mm and 1/2 in bores are the largest a
    // BT30 side-lock body is built for.
    v.extend(METRIC_BORES.iter().filter(|b| b.1 <= 16.0).map(|b| weldon(BT30, b, 60.0)));
    v.extend(INCH_BORES.iter().filter(|b| b.1 <= 12.7).map(|b| weldon(BT30, b, 60.0)));
    v.extend(sizes("ER16", "ER32").map(|e| er(Some(BT40), e, 70.0)));
    // ER40's Ø63 nut is the BT40 flange's width: a longer default gauge.
    v.extend(sizes("ER40", "ER40").map(|e| er(Some(BT40), e, 80.0)));
    v.extend(METRIC_BORES.iter().chain(INCH_BORES.iter()).map(|b| weldon(BT40, b, 63.0)));
    v.extend(sizes("ER32", "ER32").map(|e| er(Some(CAT40), e, 70.0)));
    v.extend(INCH_BORES.iter().chain(METRIC_BORES.iter()).map(|b| weldon(CAT40, b, 63.5)));
    // #792: the standalone straight-shank chucks, nose face to the
    // body's back face (where the shank goes into the spindle).
    v.extend(ER_SIZES.iter().zip(STANDALONE_GAUGE).map(|(e, g)| er(None, e, g)));
    // #794: the Tormach TTS holders.
    v.extend(tts::entries());
    v
}

/// The library entry called `name` (as [`HolderEntry::name`] states it,
/// case and spacing ignored).
pub fn find(name: &str) -> Result<HolderEntry, LibraryError> {
    let key = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase();
    let want = key(name);
    library().into_iter().find(|e| key(&e.name()) == want).ok_or_else(|| LibraryError::Unknown(name.to_owned()))
}

#[path = "holder_library_tts.rs"]
mod tts;
pub use tts::{TTS, TTS_BODY_D};

#[path = "holder_library_weldon.rs"]
mod weldon;
pub use weldon::{picker_rows, resolve, weldon_for, weldon_name, PickerRow};

#[path = "holder_library_ref.rs"]
mod reference;
pub use reference::HolderRef;

#[cfg(test)]
#[path = "holder_library_tests.rs"]
mod tests;
