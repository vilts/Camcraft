//! Plasma torch records (#670): the per-path plasma header (kerf, the
//! compensation choice, the controller's heights and delay) and one
//! span per cut contour - where the arc is struck and where it goes
//! out, which contour it cuts, whether that contour is a part's outer
//! edge, a hole or an open run, and the contour it sits inside.
//!
//! Provenance: our own model, specced from public plasma practice -
//! Hypertherm's cut-quality guidance (outer contours clockwise, holes
//! counter-clockwise, the square edge on the part), the LinuxCNC
//! Plasma Primer §13 (small holes: lead in from near the centre, no
//! lead-out, reduced feed) and Tormach's public 1300PL manual and
//! PathPilot post guidelines (the torch is lit per contour; the
//! controller owns pierce height, cut height and delay when the
//! program leaves them to it). No vendor CAM output was read.
//!
//! The moves stay the truth: every lit move is an ordinary feed (or
//! arc) at the cut plane, and the rapids between contours are the
//! torch-off travel. The records say what the moves alone cannot -
//! the torch state, the nesting, and the kerf the moves were offset
//! by - so a post can voice the arc on/off and a verifier can judge
//! cut order and kerf crossings from the CL alone (#671, #672).

use super::moves::{Move, MoveKind};

/// What a cut contour is: the outer edge of a part (material inside,
/// waste outside), a hole (waste inside), or an open run (a slit, cut
/// on its centre line - it has no side).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContourRole {
    Outer,
    Inner,
    Open,
}

impl ContourRole {
    /// The dump's word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            ContourRole::Outer => "OUTER",
            ContourRole::Inner => "INNER",
            ContourRole::Open => "OPEN",
        }
    }

    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        match w {
            "OUTER" => Some(ContourRole::Outer),
            "INNER" => Some(ContourRole::Inner),
            "OPEN" => Some(ContourRole::Open),
            _ => None,
        }
    }
}

/// One lit stretch of a plasma path: the torch is struck before
/// `moves[on]` (the lead-in, or the first cut move of an open run) and
/// goes out after `moves[off]` (the lead-out end, or the overburn end
/// of a small hole). `contour` is the contour's id within the path,
/// `parent` the id of the closed contour it lies inside (an island in
/// a hole names the hole; the hole names its part's outer edge).
/// `small` is `Some(factor)` when the small-hole rule cut it: lead in
/// from near the centre, no lead-out, the cut feed times `factor`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TorchSpan {
    pub contour: u32,
    pub role: ContourRole,
    pub parent: Option<u32>,
    pub on: usize,
    pub off: usize,
    pub small: Option<f64>,
}

/// The plasma statement of a path: the kerf the moves were offset by
/// (mm, the torch's kerf width), whether the control compensates
/// (G41/G42 spans ride [`super::CompSpan`]) or the moves already carry
/// the offset, the heights and delay the program states (`None` = the
/// controller decides), whether the feed is the controller's (no F on
/// the lit moves), whether the cut direction was reversed from the
/// outer-CW / inner-CCW default, and the lit spans in cut order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlasmaCut {
    pub kerf: f64,
    pub control_comp: bool,
    pub pierce_height: Option<f64>,
    pub cut_height: Option<f64>,
    pub pierce_delay: Option<f64>,
    pub auto_feed: bool,
    pub reversed: bool,
    pub torches: Vec<TorchSpan>,
}

impl PlasmaCut {
    /// Does every span sit inside a path of these moves, lit only over
    /// cutting motion, in increasing order without overlap, with ids
    /// unique and every parent a contour of the path? The door the
    /// dump and the loader check before trusting the records.
    #[must_use]
    pub fn is_sane(&self, moves: &[Move]) -> bool {
        let pos = |v: Option<f64>| v.is_none_or(|x| x.is_finite() && x >= 0.0);
        if !(self.kerf.is_finite() && self.kerf > 0.0)
            || !pos(self.pierce_height)
            || !pos(self.cut_height)
            || !pos(self.pierce_delay)
        {
            return false;
        }
        let mut next = 0usize;
        for (i, t) in self.torches.iter().enumerate() {
            if t.on < next || t.on > t.off || t.off >= moves.len() {
                return false;
            }
            if moves[t.on..=t.off].iter().any(|m| m.kind == MoveKind::Rapid) {
                return false;
            }
            if t.small.is_some_and(|f| !(f.is_finite() && f > 0.0 && f <= 1.0)) {
                return false;
            }
            if self.torches[..i].iter().any(|o| o.contour == t.contour) {
                return false;
            }
            if t.parent.is_some_and(|p| p == t.contour || !self.torches.iter().any(|o| o.contour == p)) {
                return false;
            }
            next = t.off + 1;
        }
        true
    }

    /// The `; PLASMA` header line of the dump (no newline): kerf in
    /// µm, then the stated words - `CONTROL`, `PIERCE<µm>`,
    /// `CUTH<µm>`, `DELAY<ms>`, `AUTOFEED`, `REVERSED`.
    #[must_use]
    pub fn header_line(&self) -> String {
        let q = |mm: f64| (mm * 1000.0).round() as i64;
        let mut s = format!("; PLASMA KERF{}", q(self.kerf));
        if self.control_comp {
            s.push_str(" CONTROL");
        }
        if let Some(h) = self.pierce_height {
            s.push_str(&format!(" PIERCE{}", q(h)));
        }
        if let Some(h) = self.cut_height {
            s.push_str(&format!(" CUTH{}", q(h)));
        }
        if let Some(d) = self.pierce_delay {
            s.push_str(&format!(" DELAY{}", q(d)));
        }
        if self.auto_feed {
            s.push_str(" AUTOFEED");
        }
        if self.reversed {
            s.push_str(" REVERSED");
        }
        s
    }
}

impl TorchSpan {
    /// The `; TORCH ON` line (no newline): `C<id> <ROLE>` then
    /// `PARENT<id>` when nested and `SMALL<factor x 1000>` under the
    /// small-hole rule.
    #[must_use]
    pub fn on_line(&self) -> String {
        let mut s = format!("; TORCH ON C{} {}", self.contour, self.role.word());
        if let Some(p) = self.parent {
            s.push_str(&format!(" PARENT{p}"));
        }
        if let Some(f) = self.small {
            s.push_str(&format!(" SMALL{}", (f * 1000.0).round() as i64));
        }
        s
    }

    /// The `; TORCH OFF` line (no newline).
    #[must_use]
    pub fn off_line(&self) -> String {
        format!("; TORCH OFF C{}", self.contour)
    }
}

/// Read a `PLASMA` header's words (after the keyword).
pub fn parse_header(words: &str) -> Result<PlasmaCut, String> {
    let mut p = PlasmaCut::default();
    let mut kerf = None;
    for w in words.split_whitespace() {
        match w {
            "CONTROL" => p.control_comp = true,
            "AUTOFEED" => p.auto_feed = true,
            "REVERSED" => p.reversed = true,
            _ => {
                let (k, v) = split_word(w);
                let v = milli(v)?;
                match k {
                    "KERF" => kerf = Some(v),
                    "PIERCE" => p.pierce_height = Some(v),
                    "CUTH" => p.cut_height = Some(v),
                    "DELAY" => p.pierce_delay = Some(v),
                    other => return Err(format!("unknown PLASMA word {other:?}")),
                }
            }
        }
    }
    p.kerf = kerf.ok_or("PLASMA needs KERF")?;
    Ok(p)
}

/// Read a `TORCH ON` line's words (after `ON`); `on` is the index of
/// the move that follows.
pub fn parse_on(words: &str, on: usize) -> Result<TorchSpan, String> {
    let mut it = words.split_whitespace();
    let id = it.next().and_then(|w| w.strip_prefix('C')).ok_or("TORCH ON needs C<id>")?;
    let contour = id.parse::<u32>().map_err(|_| format!("contour id {id:?}"))?;
    let role = it.next().and_then(ContourRole::from_word).ok_or("TORCH ON needs OUTER, INNER or OPEN")?;
    let mut t = TorchSpan { contour, role, parent: None, on, off: on, small: None };
    for w in it {
        let (k, v) = split_word(w);
        match k {
            "PARENT" => t.parent = Some(v.parse::<u32>().map_err(|_| format!("PARENT {v:?}"))?),
            "SMALL" => t.small = Some(milli(v)?),
            other => return Err(format!("unknown TORCH word {other:?}")),
        }
    }
    Ok(t)
}

/// Read a `TORCH OFF` line's contour id.
pub fn parse_off(words: &str) -> Result<u32, String> {
    let id = words.trim().strip_prefix('C').ok_or("TORCH OFF needs C<id>")?;
    id.parse::<u32>().map_err(|_| format!("contour id {id:?}"))
}

pub(super) fn split_word(w: &str) -> (&str, &str) {
    let at = w.find(|c: char| c == '-' || c.is_ascii_digit()).unwrap_or(w.len());
    w.split_at(at)
}

pub(super) fn milli(v: &str) -> Result<f64, String> {
    v.parse::<i64>().map(|n| n as f64 / 1000.0).map_err(|_| format!("not an integer: {v:?}"))
}
