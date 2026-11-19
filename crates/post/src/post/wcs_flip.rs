//! Carry a flipped WCS axis (`PARAM WcsFlip`) into the CNC record stream (#1649).
//! Provenance: plane geometry plus the public G-code standards (RS-274 /
//! NIST RS274NGC, ISO 6983) - the same reflection `invert` applies for a
//! machine's inverted axis (#374), stated here per setup. No vendor
//! material consulted.
//!
//! The meaning: a flipped axis `a` of a work offset reads the same
//! physical point with the other sign, measured from the same origin -
//! the posted word is `-(p - O)` on `a`. The cut does not change; only
//! the numbers the controller reads do. So a section that states
//! `WcsFlip` is mirrored exactly as `invert::translate_rules` mirrors a
//! whole program: coordinates, arc centres, `Start*`, `Origin*` and the
//! stock box negate on `a`; an arc reverses (G2 <-> G3) when its plane
//! holds exactly one flipped axis; cutter compensation swaps sides
//! (G41 <-> G42) when exactly one of X and Y is flipped. Both X and Y
//! flipped is a half turn about Z: nothing reverses.
//!
//! Refused by name, never guessed: a flipped Z (tool-length compensation
//! and the canned cycles' retract assume the controller's +Z leaves the
//! work), a script that does not declare `axisInversion = true`, a
//! section that is also rotated (`WcsYaw`), turning, EDM and plasma
//! sections, probing (the maintainer's decision of 2026-11-18: the probe's
//! work-offset write is proven for a machine axis that runs reversed,
//! #392, not for a mirrored work offset), and everything the mirror
//! itself refuses (threading cycles, A/B/C moves, live tooling). A
//! stream that leans on a modal origin or chain start across a change of
//! flip is refused too: the script would subtract the wrong origin.

use serde_json::Value;

use super::PostError;
use super::cnc::Record;
use super::invert::{Flip, Rules, translate_rules};
use super::js::Script;
use super::wcs_rotate::{chunk_bounds, read_finite};

const AXES: [char; 3] = ['X', 'Y', 'Z'];

/// The stream's word for the flags: "X", "XY", "XYZ"; `None` for no flip.
#[must_use]
pub fn letters(flip: [bool; 3]) -> Option<String> {
    let s: String = AXES.iter().zip(flip).filter(|(_, on)| *on).map(|(c, _)| *c).collect();
    (!s.is_empty()).then_some(s)
}

/// Read a `WcsFlip` value: each of X, Y, Z at most once, at least one.
/// Anything else is refused - a value this pass cannot read must never
/// post as "not flipped".
pub fn parse(value: &Value) -> Result<[bool; 3], PostError> {
    let bad = || PostError::Refused(format!("stream parameter WcsFlip must be axis letters (X, Y, Z), got {value}"));
    let word = value.as_str().ok_or_else(bad)?;
    let mut flip = [false; 3];
    for c in word.chars() {
        let k = AXES.iter().position(|a| *a == c).ok_or_else(bad)?;
        let slot = flip.get_mut(k).ok_or_else(bad)?;
        if *slot {
            return Err(bad());
        }
        *slot = true;
    }
    if flip.iter().any(|f| *f) { Ok(flip) } else { Err(bad()) }
}

/// "its X axis flipped" / "its X and Y axes flipped".
fn phrase(flip: [bool; 3]) -> String {
    let names: Vec<String> = AXES.iter().zip(flip).filter(|(_, on)| *on).map(|(c, _)| c.to_string()).collect();
    match names.as_slice() {
        [one] => format!("its {one} axis flipped"),
        [init @ .., last] => format!("its {} and {last} axes flipped", init.join(", ")),
        [] => "no axis flipped".to_owned(),
    }
}

/// What one section states, read before anything is changed.
#[derive(Default)]
struct Stated {
    flip: Option<[bool; 3]>,
    yaw: f64,
    origin: [bool; 2],
    start: [bool; 2],
    motion: bool,
    op: String,
    /// A section kind the mirror is not verified for, by its name.
    foreign: Option<&'static str>,
    /// The section probes (a touch, a guard, a back-off or a result).
    probe: bool,
}

fn read(chunk: &[Record]) -> Result<Stated, PostError> {
    let mut st = Stated::default();
    for r in chunk {
        match r {
            Record::Param { name, value } => match name.as_str() {
                "WcsFlip" => {
                    let f = parse(value)?;
                    if st.flip.is_some_and(|old| old != f) {
                        return Err(PostError::Refused("one section states two different WcsFlip values".into()));
                    }
                    st.flip = Some(f);
                }
                "WcsYaw" => st.yaw = read_finite(value, "WcsYaw")?,
                "OriginX" => st.origin[0] = true,
                "OriginY" => st.origin[1] = true,
                "StartX" => st.start[0] = true,
                "StartY" => st.start[1] = true,
                "OperationName" => st.op = value.as_str().unwrap_or("").to_owned(),
                "StockTurnRadius" | "SurfaceSpeed" | "ToolOrientation" => st.foreign = Some("a turning section"),
                "EdmCondition" | "SinkerCondition" => st.foreign = Some("an EDM section"),
                _ => {}
            },
            Record::Plasma(_) | Record::Torch(_) => st.foreign = Some("a plasma section"),
            Record::Move(_) | Record::Cycle(_) => st.motion = true,
            Record::Touch(_) | Record::Guard(_) | Record::Retreat(_) => {
                st.motion = true;
                st.probe = true;
            }
            Record::Found(_) => st.probe = true,
            _ => {}
        }
    }
    Ok(st)
}

fn who(op: &str) -> String {
    if op.is_empty() { String::new() } else { format!("operation {op}: ") }
}

/// Mirror every section that states `PARAM WcsFlip` about its work
/// origin, or refuse by name. One record out for each record in, so the
/// viewer's move indices hold. A stream that states no flip returns
/// untouched.
pub fn apply(records: Vec<Record>, s: &Script) -> Result<Vec<Record>, PostError> {
    if !records.iter().any(|r| matches!(r, Record::Param { name, .. } if name == "WcsFlip")) {
        return Ok(records);
    }
    let spans = chunk_bounds(&records);
    let mut out: Vec<Record> = Vec::with_capacity(records.len());
    let mut it = records.into_iter();
    // The flip under which the script's modal origin (X, Y) was last
    // stated, and under which the engine's chain last moved. `None`:
    // never stated - zero, which a mirror leaves where it is.
    let mut origin_under: [Option<bool>; 2] = [None, None];
    let mut chain_under: [Option<bool>; 2] = [None, None];
    for (start, end) in spans {
        let chunk: Vec<Record> = it.by_ref().take(end.saturating_sub(start)).collect();
        let st = read(&chunk)?;
        let flip = st.flip.unwrap_or([false; 3]);
        let op = who(&st.op);
        for k in 0..2 {
            let on = flip.get(k).copied().unwrap_or(false);
            let letter = AXES.get(k).copied().unwrap_or('?');
            let origin_stale = !st.origin.get(k).copied().unwrap_or(false)
                && origin_under.get(k).copied().flatten().is_some_and(|u| u != on);
            let chain_stale = !st.start.get(k).copied().unwrap_or(false)
                && chain_under.get(k).copied().flatten().is_some_and(|u| u != on);
            if st.motion && (origin_stale || chain_stale) {
                let what = if origin_stale { format!("Origin{letter}") } else { format!("Start{letter}") };
                return Err(PostError::Refused(format!(
                    "{op}the section does not state {what}, and the WCS {letter} flip changes from the section \
                     before it - each section of a stream with a flipped WCS must state its own origin and start point"
                )));
            }
            if st.origin.get(k).copied().unwrap_or(false)
                && let Some(u) = origin_under.get_mut(k)
            {
                *u = Some(on);
            }
            if (st.motion || st.start.get(k).copied().unwrap_or(false))
                && let Some(u) = chain_under.get_mut(k)
            {
                *u = Some(on);
            }
        }
        if st.flip.is_none() {
            out.extend(chunk);
            continue;
        }
        let what = format!("{op}the WCS has {}", phrase(flip));
        if flip[2] {
            return Err(PostError::Refused(format!(
                "{what}; posting a flipped Z is not supported yet - tool-length compensation (G43) and the canned \
                 cycles' retract assume the controller's +Z leaves the work - clear Flip Z on the WCS page"
            )));
        }
        if !s.translates_inverted_axes() {
            return Err(PostError::Refused(format!(
                "{what}, and this post script is not verified for a mirrored program (it does not declare \
                 axisInversion = true) - post with a script that does (Fanuc, Haas), or clear the flip on the WCS page"
            )));
        }
        if st.yaw.to_radians().abs() > 1e-9 {
            return Err(PostError::Refused(format!(
                "{what} and is also rotated {:.3} deg about Z; a flipped and rotated WCS is not supported yet - \
                 clear the flip or the rotation",
                st.yaw.abs()
            )));
        }
        if let Some(kind) = st.foreign {
            return Err(PostError::Refused(format!(
                "{what}; {kind} is not supported for a flipped WCS axis yet - clear the flip on the WCS page"
            )));
        }
        if st.probe {
            return Err(PostError::Refused(format!(
                "{what}; probing does not post in a flipped WCS - the probe's work-offset write is not verified \
                 for a mirrored work offset - probe in a setup without a flip, or clear the flip on the WCS page"
            )));
        }
        let rules = Rules { probing: false, helix: true, cause: "a flipped WCS axis" };
        let mirrored = translate_rules(chunk, Flip { x: flip[0], y: flip[1], z: false }, rules)
            .map_err(|why| PostError::Refused(format!("{what}: {}", why.join("; "))))?;
        out.extend(mirrored);
    }
    out.extend(it);
    Ok(out)
}
