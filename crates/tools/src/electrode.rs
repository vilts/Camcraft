//! The sinker EDM electrode as a tool record (#541): a rectangular
//! solid burned straight down Z, held at a pickup - or, with an outline
//! (#1152, `electrode_outline`), a body of that section.
//!
//! Provenance: general die-sink EDM practice from public machine-tool
//! handbooks (the electrode is a shaped solid fed into the work, the
//! spark gap set by the condition, the electrode held by a pickup the
//! control measures from); field mapping and laws are our own design
//! (#290 D1-D10). No vendor material.
//!
//! The record rides the existing [`Tool`] fields so the library, the
//! project file and the commands carry it unchanged:
//! - `diameter` - the burning face's X size,
//! - `width` - its Y size,
//! - `flute_len` - the electrode's length below the holder: the deepest
//!   burn it can make without burying its pickup,
//! - `shank_len` - the pickup offset: from the electrode's top face up
//!   to the pickup reference the control measures the electrode from,
//! - `outline` - the body's section about the burn point (#1152); empty
//!   for a rectangular face. `diameter` and `width` are then its X and
//!   Y extents.

use crate::tools::{Tool, ToolKind};

/// Largest face side the record accepts, mm - the same bound
/// [`Tool::is_sane`] puts on every diameter.
const MAX_SIDE: f64 = 500.0;

/// The electrode's kind-scoped sanity (the `Tool::is_sane` arm): a
/// stated Y size within the diameter's bound, and a sharp-cornered face.
#[must_use]
pub fn is_sane(t: &Tool) -> bool {
    t.width.is_finite()
        && t.width > 0.0
        && t.width <= MAX_SIDE
        && t.corner_r == 0.0
        && crate::electrode_outline::record_refusal(t).is_none()
}

/// The electrode's refusal, by name: what the commands and the library
/// reader say before a garbled electrode enters the document. `None`
/// for any other kind, and for a sound electrode.
#[must_use]
pub fn refusal(t: &Tool) -> Option<String> {
    if t.kind != ToolKind::Electrode {
        return None;
    }
    if t.rpm != 0.0 {
        return Some(format!(
            "a sinker electrode turns no spindle - its rpm must be 0 (it states {})",
            t.rpm
        ));
    }
    if !(t.width.is_finite() && t.width > 0.0) {
        return Some(format!(
            "a sinker electrode states its Y size in its width (it states {}); its diameter is the X size",
            t.width
        ));
    }
    if t.corner_r != 0.0 {
        return Some(format!(
            "a sinker electrode's face is sharp-cornered - a corner radius of {} is not modelled",
            t.corner_r
        ));
    }
    crate::electrode_outline::record_refusal(t)
}

/// The body's section about the burn point (#1152): `Some` for an
/// electrode that states an outline, `None` for a rectangular face and
/// for every other kind.
#[must_use]
pub fn outline(t: &Tool) -> Option<&[[f64; 2]]> {
    (t.kind == ToolKind::Electrode && !t.outline.is_empty()).then_some(t.outline.as_slice())
}

/// The face's half sizes `(x, y)`, mm: the box the stock sim sweeps.
#[must_use]
pub fn half_sizes(t: &Tool) -> (f64, f64) {
    (t.diameter * 0.5, t.width * 0.5)
}

/// A rectangular electrode `x` by `y` mm, `length` long below the
/// holder, held `pickup` above its top face. `feed` is a nominal servo
/// rate; the sinker op states its own.
#[must_use]
pub fn electrode_tool(x: f64, y: f64, length: f64, pickup: f64) -> Tool {
    Tool {
        pitch_min: 0.0,
        kind: ToolKind::Electrode,
        diameter: x,
        corner_r: 0.0,
        tip_angle: 0.0,
        pitch: 0.0,
        nose_radius: 0.0,
        width: y,
        flute_len: length,
        // The stem up to the pickup, drawn as wide as the face's X
        // side - a plain envelope for the spun-body laws.
        shank_d: x,
        shank_len: pickup,
        holder_d: 0.0,
        holder_len: 0.0,
        material_group: "graphite".into(),
        rpm: 0.0,
        feed: 50.0,
        tool_material: None,
        flutes: None,
        center_cutting: true,
        orientation: None,
        turning: None,
        catalog: crate::catalog_fields::ToolCatalog::default(),
        cutting: Vec::new(),
        outline: Vec::new(),
    }
}

/// A body electrode of section `outline` (mm about the burn point),
/// `length` long below the holder, held `pickup` above its top face
/// (#1152). Its two sizes are the outline's extents; an outline that is
/// no section, or is not centred, makes a record its own door refuses.
#[must_use]
pub fn electrode_outline_tool(outline: &[[f64; 2]], length: f64, pickup: f64) -> Tool {
    let (x, y) = crate::electrode_outline::bounds(outline).map_or((0.0, 0.0), |(lo, hi)| (hi[0] - lo[0], hi[1] - lo[1]));
    Tool { outline: outline.to_vec(), ..electrode_tool(x, y, length, pickup) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electrode_is_sane_and_refuses_by_name() {
        let e = electrode_tool(10.0, 6.0, 40.0, 15.0);
        assert!(e.is_sane());
        assert_eq!(e.torch_refusal(), None);
        assert_eq!(half_sizes(&e), (5.0, 3.0));

        let spun = Tool { rpm: 1000.0, ..e.clone() };
        assert!(!spun.is_sane());
        assert!(spun.torch_refusal().is_some_and(|w| w.contains("rpm must be 0")));

        let flat = Tool { width: 0.0, ..e.clone() };
        assert!(!flat.is_sane());
        assert!(flat.torch_refusal().is_some_and(|w| w.contains("Y size")));

        let round = Tool { corner_r: 1.0, ..e };
        assert!(!round.is_sane());
        assert!(round.torch_refusal().is_some_and(|w| w.contains("sharp-cornered")));
    }
}
