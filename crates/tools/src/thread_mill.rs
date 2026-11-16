//! Thread mills (#894): what the single-form and multi-form cutter records mean, their sanity law and the pitch door a thread-milling operation asks.
//!
//! Provenance: the ISO general-purpose metric thread profile (ISO 68-1
//! basic profile; ISO 724 basic dimensions: internal thread depth
//! 5/8 H = 0.5413 x pitch; ISO 965-1 / DIN 13 external design root:
//! 17/24 H = 0.6134 x pitch, H = 0.866 x pitch) and the thread-milling
//! chapters of public machine-tool handbooks (Machinery's Handbook,
//! "Thread milling"; Smid, "CNC Programming Handbook", helical milling):
//! a multi-form cutter carries rows of teeth at one pitch and finishes
//! the thread in one helical revolution, a single-form cutter carries one
//! row and rides one turn per thread, so it cuts a range of pitches. No
//! vendor catalog or CAM product material consulted.
//!
//! Pure module: no flecs, no I/O. A thread mill is a [`Tool`] of kind
//! [`ToolKind::ThreadMill`] (multi-form) or [`ToolKind::ThreadMillSingle`]
//! reading `diameter` (across the tooth crests), `tip_angle` (the form's
//! included flank angle), `pitch` (the multi-form's one pitch, the
//! single-form's coarsest), `pitch_min` (the single-form's finest),
//! `flute_len` (the cutting length: rows x pitch on a multi-form, the
//! tooth's height on a single-form) and `flutes` (cutting edges around).
//! A neck rides the drawn cutter outline ([`crate::cutter_profile`]).

use crate::tools::{Tool, ToolKind};

/// Radial depth of an internal ISO metric / unified thread per mm of
/// pitch: minor cylinder to major, 5/8 H (ISO 724).
pub const INTERNAL_DEPTH_PER_PITCH: f64 = 0.5413;

/// Radial depth of an external ISO metric thread per mm of pitch: major
/// cylinder to the design root, 17/24 H (ISO 965-1, DIN 13). It is also
/// the tooth height a thread mill must carry to cut either thread.
pub const EXTERNAL_DEPTH_PER_PITCH: f64 = 0.6134;

/// The form angle the thread-milling operation cuts: the 60-degree
/// included angle of ISO metric and unified threads.
pub const FORM_ANGLE: f64 = 60.0;

/// Agreement slack between a stated pitch and a tool's, mm: a typed
/// value, not float dust.
const PITCH_AGREE: f64 = 1e-6;

/// Is `kind` one of the two thread-mill kinds?
#[must_use]
pub fn is_thread_mill(kind: ToolKind) -> bool {
    matches!(kind, ToolKind::ThreadMill | ToolKind::ThreadMillSingle)
}

/// The kind-scoped half of [`Tool::is_sane`] for a thread mill: a
/// positive pitch (a real range on a single-form), a form angle that is
/// a V, a core left under the teeth at the coarsest pitch, and - on a
/// multi-form - a cutting length holding at least one row.
#[must_use]
pub fn is_sane(t: &Tool) -> bool {
    let form = t.tip_angle > 0.0 && t.tip_angle < 180.0;
    let core = t.diameter * 0.5 > EXTERNAL_DEPTH_PER_PITCH * t.pitch;
    let pitches = match t.kind {
        ToolKind::ThreadMill => t.pitch > 0.0 && t.flute_len >= t.pitch,
        ToolKind::ThreadMillSingle => t.pitch > 0.0 && t.pitch_min > 0.0 && t.pitch_min <= t.pitch,
        _ => return false,
    };
    form && core && pitches
}

/// Rows of teeth along the axis: the cutting length over the pitch on a
/// multi-form (at least one), one on a single-form and on anything else.
#[must_use]
pub fn rows(t: &Tool) -> u32 {
    if t.kind != ToolKind::ThreadMill || !(t.pitch > 0.0 && t.flute_len.is_finite()) {
        return 1;
    }
    let n = (t.flute_len / t.pitch + 1e-9).floor();
    if n.is_finite() && n >= 1.0 { n.min(4096.0) as u32 } else { 1 }
}

/// The pitch a thread-milling operation cuts with `t`, or the refusal by
/// name. `stated` is the operation's own pitch: a multi-form mill cuts
/// only its one pitch (unstated reads it), a single-form mill needs the
/// pitch stated and inside its range.
pub fn resolve_pitch(t: &Tool, stated: Option<f64>) -> Result<f64, String> {
    if let Some(p) = stated
        && !(p.is_finite() && p > 0.0)
    {
        return Err(format!("the thread pitch must be positive (it states {p} mm)"));
    }
    match t.kind {
        ToolKind::ThreadMill => match stated {
            None => Ok(t.pitch),
            Some(p) if (p - t.pitch).abs() <= PITCH_AGREE => Ok(t.pitch),
            Some(p) => Err(format!(
                "a multi-form thread mill cuts only its own {} mm pitch - the operation states {p} mm (mount a mill ground to that pitch, or a single-form thread mill)",
                t.pitch
            )),
        },
        ToolKind::ThreadMillSingle => match stated {
            None => Err(format!(
                "a single-form thread mill cuts any pitch from {} to {} mm - state the thread's pitch on the operation",
                t.pitch_min, t.pitch
            )),
            Some(p) if p >= t.pitch_min - PITCH_AGREE && p <= t.pitch + PITCH_AGREE => Ok(p),
            Some(p) => Err(format!(
                "a {p} mm pitch is outside this single-form thread mill's {} to {} mm range",
                t.pitch_min, t.pitch
            )),
        },
        other => Err(format!("a {other} is not a thread mill - mount a thread mill or a single-form thread mill")),
    }
}

/// The form door: the operation cuts the 60-degree profile, so a mill
/// ground to another angle (55-degree Whitworth, 30-degree trapezoidal)
/// is refused by name instead of cutting a wrong flank.
#[must_use]
pub fn form_refusal(t: &Tool) -> Option<String> {
    ((t.tip_angle - FORM_ANGLE).abs() > 1e-6).then(|| {
        format!(
            "thread milling cuts the 60-degree ISO metric / unified form - this thread mill's teeth are ground to {} degrees",
            t.tip_angle
        )
    })
}

/// The radius a thread mill removes with certainty: its tooth roots,
/// crest radius minus the internal thread depth at its (coarsest) pitch -
/// the reading a tap already has. `None` when no core is left.
#[must_use]
pub fn core_radius(t: &Tool) -> Option<f64> {
    let core = t.diameter * 0.5 - INTERNAL_DEPTH_PER_PITCH * t.pitch;
    (core.is_finite() && core > 0.0).then_some(core)
}

/// A thread mill of crest diameter `d` for the starter numbers tests and
/// the editor's draft use: multi-form at `pitch`, or single-form over
/// `pitch_min..=pitch` when `pitch_min` is positive. Carbide, 3 flutes,
/// a shank of the crest diameter rounded up to a common gauge.
#[must_use]
pub fn thread_mill(d: f64, pitch: f64, pitch_min: f64, cutting_len: f64) -> Tool {
    let single = pitch_min > 0.0;
    Tool {
        kind: if single { ToolKind::ThreadMillSingle } else { ToolKind::ThreadMill },
        diameter: d,
        corner_r: 0.0,
        tip_angle: FORM_ANGLE,
        pitch,
        pitch_min: if single { pitch_min } else { 0.0 },
        nose_radius: 0.0,
        width: 0.0,
        flute_len: cutting_len,
        shank_d: d,
        shank_len: 40.0,
        holder_d: 44.0,
        holder_len: 35.0,
        material_group: "aluminum".into(),
        rpm: 6000.0,
        feed: 300.0,
        tool_material: Some(crate::tools::ToolMaterial::Carbide),
        flutes: Some(3),
        center_cutting: false,
        orientation: None,
        turning: None,
        catalog: crate::catalog_fields::ToolCatalog::default(),
        cutting: Vec::new(),
        outline: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multi_form_mill_cuts_only_its_own_pitch() {
        let t = thread_mill(8.0, 1.5, 0.0, 12.0);
        assert!(t.is_sane());
        assert_eq!(rows(&t), 8);
        assert_eq!(resolve_pitch(&t, None), Ok(1.5));
        assert_eq!(resolve_pitch(&t, Some(1.5)), Ok(1.5));
        let err = resolve_pitch(&t, Some(1.0)).unwrap_err();
        assert!(err.contains("only its own 1.5 mm pitch") && err.contains("1 mm"), "{err}");
    }

    #[test]
    fn a_single_form_mill_cuts_its_range_and_needs_the_pitch_stated() {
        let t = thread_mill(6.0, 1.5, 0.5, 2.0);
        assert!(t.is_sane());
        assert_eq!(rows(&t), 1);
        assert_eq!(resolve_pitch(&t, Some(0.5)), Ok(0.5));
        assert_eq!(resolve_pitch(&t, Some(1.5)), Ok(1.5));
        assert!(resolve_pitch(&t, None).unwrap_err().contains("state the thread's pitch"));
        let err = resolve_pitch(&t, Some(2.0)).unwrap_err();
        assert!(err.contains("outside") && err.contains("0.5 to 1.5 mm"), "{err}");
        for bad in [0.0, -1.0, f64::NAN] {
            assert!(resolve_pitch(&t, Some(bad)).unwrap_err().contains("must be positive"));
        }
    }

    #[test]
    fn nonsense_thread_mills_are_refused() {
        let multi = thread_mill(8.0, 1.5, 0.0, 12.0);
        assert!(!Tool { pitch: 0.0, ..multi.clone() }.is_sane(), "no pitch");
        assert!(!Tool { tip_angle: 0.0, ..multi.clone() }.is_sane(), "no form");
        assert!(!Tool { flute_len: 1.0, ..multi.clone() }.is_sane(), "not one row of teeth");
        assert!(!Tool { diameter: 1.5, ..multi.clone() }.is_sane(), "teeth deeper than the cutter's radius");
        assert!(!Tool { pitch_min: -0.1, ..multi.clone() }.is_sane());
        let single = thread_mill(6.0, 1.5, 0.5, 2.0);
        assert!(!Tool { pitch_min: 0.0, ..single.clone() }.is_sane(), "a range needs its finest pitch");
        assert!(!Tool { pitch_min: 2.0, ..single }.is_sane(), "finest above coarsest");
        // Another kind never reads as a thread mill.
        let tap = Tool { kind: ToolKind::Tap, ..multi };
        assert!(resolve_pitch(&tap, None).unwrap_err().contains("not a thread mill"));
    }

    #[test]
    fn the_form_door_names_another_angle() {
        let t = thread_mill(8.0, 1.5, 0.0, 12.0);
        assert_eq!(form_refusal(&t), None);
        let w = Tool { tip_angle: 55.0, ..t.clone() };
        assert!(w.is_sane(), "a 55-degree mill is a real tool");
        assert!(form_refusal(&w).is_some_and(|m| m.contains("55 degrees")));
        assert!((core_radius(&t).unwrap() - (4.0 - 0.5413 * 1.5)).abs() < 1e-12);
    }
}
