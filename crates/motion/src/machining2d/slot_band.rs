//! The slot's fit law (#3475): does a tool fit a slot of a given width.
//!
//! Provenance: our own closed form for the stadium slot (`slots.rs`),
//! from plain offset geometry; no vendor material consulted.

use super::face::GenError;

/// One rule for the slot generator and the commands that assign a tool:
/// the half-width `h = width / 2 - r - leave` of the band that the tool
/// centre can travel. `Ok(Some(h))` when the tool fits (`h == 0` is the
/// full-width slot), `Ok(None)` when the tool is too wide, and an error
/// for numbers that no slot can have (not finite, a width or diameter
/// not over 0, a negative stock to leave).
pub fn slot_centre_band(width: f64, tool_diameter: f64, stock_to_leave: f64) -> Result<Option<f64>, GenError> {
    if !(width.is_finite() && tool_diameter.is_finite() && stock_to_leave.is_finite()) {
        return Err(GenError::Invalid("parameters must be finite".into()));
    }
    if width <= 0.0 {
        return Err(GenError::Invalid("slot width must be positive".into()));
    }
    if tool_diameter <= 0.0 {
        return Err(GenError::Invalid("tool diameter must be positive".into()));
    }
    if stock_to_leave < 0.0 {
        return Err(GenError::Invalid("stock to leave must not be negative".into()));
    }
    let h = width * 0.5 - tool_diameter * 0.5 - stock_to_leave;
    Ok((h >= 0.0).then_some(h))
}

#[cfg(test)]
mod tests {
    use super::slot_centre_band;

    #[test]
    fn a_smaller_tool_fits_with_the_band_left() {
        assert_eq!(slot_centre_band(10.0, 6.0, 0.5).ok(), Some(Some(1.5)));
    }

    #[test]
    fn a_tool_of_the_slot_width_fits_on_the_centre_line() {
        assert_eq!(slot_centre_band(6.0, 6.0, 0.0).ok(), Some(Some(0.0)));
    }

    #[test]
    fn a_wider_tool_does_not_fit() {
        assert_eq!(slot_centre_band(6.0, 100.0, 0.0).ok(), Some(None));
    }

    #[test]
    fn stock_to_leave_makes_a_tool_of_the_slot_width_too_wide() {
        assert_eq!(slot_centre_band(6.0, 6.0, 0.1).ok(), Some(None));
    }

    #[test]
    fn numbers_no_slot_can_have_are_refused_cleanly() {
        for (w, d, l) in [
            (f64::NAN, 6.0, 0.0),
            (6.0, f64::INFINITY, 0.0),
            (0.0, 6.0, 0.0),
            (-1.0, 6.0, 0.0),
            (6.0, 0.0, 0.0),
            (6.0, 3.0, -0.1),
        ] {
            assert!(slot_centre_band(w, d, l).is_err(), "{w} {d} {l}");
        }
    }
}
