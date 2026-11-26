//! Abrasive-waterjet records (#1882): the per-path waterjet header
//! (abrasive feed on or off, the pierce dwell and the valve lag dwell)
//! and one span per cut contour - where the jet opens, where it closes,
//! and which contour it cuts.
//!
//! Provenance: our own model, specced from general abrasive-waterjet
//! practice (the jet opens over the pierce point, the valve needs a
//! short time to give a full stream, the abrasive feed starts on the
//! open jet, a pierce dwells before the cut moves). No vendor CAM
//! output, post or control file was read. The records are independent
//! of the plasma records (`super::torch`): a waterjet has no torch
//! height control, so no plasma word applies to it.
//!
//! The moves stay the truth: every open-jet move is an ordinary feed
//! (or arc) in the cut plane, and the rapids between contours are the
//! closed-jet travel. The records say what the moves alone cannot.

use super::moves::{Move, MoveKind};
use super::torch::{milli, split_word, ContourRole};

/// One open-jet stretch of a waterjet path: the jet opens before
/// `moves[on]` and closes after `moves[off]`. `contour` is the
/// contour's id within the path, unique, in cut order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JetSpan {
    pub contour: u32,
    pub role: ContourRole,
    pub on: usize,
    pub off: usize,
}

/// The waterjet statement of a path: whether abrasive feeds with the
/// jet (`false` = a pure-water cut), the pierce dwell and the valve
/// lag dwell in seconds (`None` = the program states none), and the
/// open-jet spans in cut order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WaterjetCut {
    pub abrasive: bool,
    pub pierce_dwell: Option<f64>,
    pub valve_lag: Option<f64>,
    pub jets: Vec<JetSpan>,
}

fn dwell_ok(v: Option<f64>) -> bool {
    v.is_none_or(|x| x.is_finite() && x >= 0.0)
}

impl WaterjetCut {
    /// Does every span sit inside a path of these moves, open only over
    /// cutting motion, in increasing order without overlap, with ids
    /// unique, and are the dwells finite and not negative?
    #[must_use]
    pub fn is_sane(&self, moves: &[Move]) -> bool {
        if !dwell_ok(self.pierce_dwell) || !dwell_ok(self.valve_lag) {
            return false;
        }
        let mut next = 0usize;
        for (i, j) in self.jets.iter().enumerate() {
            if j.on < next || j.on > j.off || j.off >= moves.len() {
                return false;
            }
            if moves[j.on..=j.off].iter().any(|m| m.kind == MoveKind::Rapid) {
                return false;
            }
            if self.jets[..i].iter().any(|o| o.contour == j.contour) {
                return false;
            }
            next = j.off + 1;
        }
        true
    }

    /// The `; WATERJET` header line (no newline): `ABRASIVE` when the
    /// abrasive feeds, then `DWELL<ms>` (the pierce dwell) and
    /// `LAG<ms>` (the valve lag dwell) when stated.
    #[must_use]
    pub fn header_line(&self) -> String {
        let q = |s: f64| (s * 1000.0).round() as i64;
        let mut out = "; WATERJET".to_owned();
        if self.abrasive {
            out.push_str(" ABRASIVE");
        }
        if let Some(d) = self.pierce_dwell {
            out.push_str(&format!(" DWELL{}", q(d)));
        }
        if let Some(d) = self.valve_lag {
            out.push_str(&format!(" LAG{}", q(d)));
        }
        out
    }
}

impl JetSpan {
    /// The `; JET ON` line (no newline): `C<id> <ROLE>`.
    #[must_use]
    pub fn on_line(&self) -> String {
        format!("; JET ON C{} {}", self.contour, self.role.word())
    }

    /// The `; JET OFF` line (no newline).
    #[must_use]
    pub fn off_line(&self) -> String {
        format!("; JET OFF C{}", self.contour)
    }
}

/// Read a `WATERJET` header's words (after the keyword). Each word at
/// most once; a negative dwell is refused.
pub fn parse_header(words: &str) -> Result<WaterjetCut, String> {
    let mut w = WaterjetCut::default();
    let mut seen: Vec<&str> = Vec::new();
    for word in words.split_whitespace() {
        let (k, v) = if word == "ABRASIVE" { (word, "") } else { split_word(word) };
        if seen.contains(&k) {
            return Err(format!("WATERJET states {k} twice"));
        }
        seen.push(k);
        match k {
            "ABRASIVE" => w.abrasive = true,
            "DWELL" => w.pierce_dwell = Some(milli(v)?),
            "LAG" => w.valve_lag = Some(milli(v)?),
            other => return Err(format!("unknown WATERJET word {other:?}")),
        }
    }
    if !dwell_ok(w.pierce_dwell) || !dwell_ok(w.valve_lag) {
        return Err("WATERJET DWELL and LAG must not be negative".into());
    }
    Ok(w)
}

/// Read a `JET ON` line's words (after `ON`); `on` is the index of the
/// move that follows.
pub fn parse_on(words: &str, on: usize) -> Result<JetSpan, String> {
    let mut it = words.split_whitespace();
    let id = it.next().and_then(|w| w.strip_prefix('C')).ok_or("JET ON needs C<id>")?;
    let contour = id.parse::<u32>().map_err(|_| format!("contour id {id:?}"))?;
    let role = it.next().and_then(ContourRole::from_word).ok_or("JET ON needs OUTER, INNER or OPEN")?;
    if let Some(extra) = it.next() {
        return Err(format!("unknown JET word {extra:?}"));
    }
    Ok(JetSpan { contour, role, on, off: on })
}

/// Read a `JET OFF` line's contour id.
pub fn parse_off(words: &str) -> Result<u32, String> {
    let id = words.trim().strip_prefix('C').ok_or("JET OFF needs C<id>")?;
    id.parse::<u32>().map_err(|_| format!("contour id {id:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point3;

    fn cut() -> WaterjetCut {
        WaterjetCut {
            abrasive: true,
            pierce_dwell: Some(0.5),
            valve_lag: Some(0.25),
            jets: vec![JetSpan { contour: 0, role: ContourRole::Inner, on: 1, off: 2 }],
        }
    }

    #[test]
    fn waterjet_lines_round_trip() {
        let c = cut();
        assert_eq!(c.header_line(), "; WATERJET ABRASIVE DWELL500 LAG250");
        let back = parse_header(c.header_line().trim_start_matches("; WATERJET")).expect("header reads");
        assert_eq!(back, WaterjetCut { jets: Vec::new(), ..c.clone() });
        let j = c.jets[0];
        assert_eq!(j.on_line(), "; JET ON C0 INNER");
        assert_eq!(j.off_line(), "; JET OFF C0");
        assert_eq!(parse_on(j.on_line().trim_start_matches("; JET ON"), 1), Ok(JetSpan { off: 1, ..j }));
        assert_eq!(parse_off(j.off_line().trim_start_matches("; JET OFF")), Ok(0));
        let water = WaterjetCut::default();
        assert_eq!(water.header_line(), "; WATERJET");
        assert_eq!(parse_header(""), Ok(water));
    }

    #[test]
    fn waterjet_bad_words_are_refused_by_name() {
        for (words, why) in [
            ("DWELL-5", "negative"),
            ("DWELL5 DWELL6", "twice"),
            ("KERF100", "unknown"),
            ("DWELL5x", "integer"),
        ] {
            let e = parse_header(words).expect_err(words);
            assert!(e.contains(why), "{words}: {e}");
        }
        assert!(parse_on(" C1", 0).is_err());
        assert!(parse_on(" C1 OUTER SMALL650", 0).is_err());
        assert!(parse_off(" 1").is_err());
    }

    #[test]
    fn waterjet_spans_must_sit_on_cut_moves() {
        let p = |x| Point3::new(x, 0.0, 0.0);
        let moves = vec![
            Move::new(MoveKind::Rapid, p(0.0), p(1.0), None),
            Move::new(MoveKind::Feed, p(1.0), p(2.0), Some(100.0)),
            Move::new(MoveKind::Feed, p(2.0), p(3.0), Some(100.0)),
        ];
        assert!(cut().is_sane(&moves));
        let mut rapid = cut();
        rapid.jets[0].on = 0;
        assert!(!rapid.is_sane(&moves), "an open jet over a rapid");
        let mut past = cut();
        past.jets[0].off = 3;
        assert!(!past.is_sane(&moves), "a span past the last move");
        let mut twice = cut();
        twice.jets.push(JetSpan { contour: 0, role: ContourRole::Outer, on: 2, off: 2 });
        assert!(!twice.is_sane(&moves), "an overlap and a repeated id");
        let bad = WaterjetCut { valve_lag: Some(f64::NAN), ..cut() };
        assert!(!bad.is_sane(&moves), "a non-finite dwell");
    }
}
