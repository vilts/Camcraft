//! The rotary subplate's figures (#821): the base plate a 4th axis is
//! bolted to, located on a fixture plate by two pin bores. Its frame has
//! the origin on the first bore at the underside, +x toward the second
//! (`accessory`'s frames), so `plate::mount` seats it on a dowel pair
//! exactly as it seats a two-pin locator.
//!
//! The defaults are generic figures of our own, named as such in the
//! dialog and MCP - no maker's dimension is committed; the user enters
//! their own subplate's (`docs/decisions/821-rotary-on-plate-stack.md`).
//!
//! Provenance: our own model; pin-located base plates are common shop
//! workholding (machine-shop handbooks).

use super::Accessory;

const IN: f64 = 25.4;

/// How far a placed subplate's bore spacing may differ from the dowel
/// pair it stands on, mm: a pin in a bore, not a slot.
pub const SPACING_TOL: f64 = 0.05;

/// The generic subplate: 10 x 6 x 3/4 in, 1/2 in pin bores 6 in apart,
/// the first 2 in from the -x end, both on the width's centre line.
/// Generic figures, not any maker's.
#[must_use]
pub fn generic() -> Accessory {
    Accessory::RotarySubplate {
        length: 10.0 * IN,
        width: 6.0 * IN,
        thickness: 0.75 * IN,
        bore_spacing: 6.0 * IN,
        bore_x: 2.0 * IN,
        bore_y: 3.0 * IN,
        bore_diameter: 0.5 * IN,
    }
}

/// The subplate law past "every figure a positive, finite length" (which
/// `Accessory::check` holds first): both bores on the plate, and the
/// bores narrower than their spacing. Refusals name the figure.
pub fn check(a: &Accessory) -> Result<(), String> {
    let Accessory::RotarySubplate { length, width, bore_spacing, bore_x, bore_y, bore_diameter, .. } = *a else {
        return Ok(());
    };
    let what = "rotary subplate";
    let r = 0.5 * bore_diameter;
    if bore_diameter >= bore_spacing {
        return Err(format!(
            "{what} pin bore diameter {bore_diameter:.3} mm must be less than the bore spacing {bore_spacing:.3} mm"
        ));
    }
    if bore_x < r {
        return Err(format!("{what} first bore from the -X end {bore_x:.3} mm puts the bore over the edge"));
    }
    if bore_x + bore_spacing + r > length {
        return Err(format!(
            "{what} second bore at {:.3} mm from the -X end runs off the {length:.3} mm length",
            bore_x + bore_spacing
        ));
    }
    if bore_y < r || bore_y + r > width {
        return Err(format!("{what} bores from the -Y edge {bore_y:.3} mm put the bores off the {width:.3} mm width"));
    }
    Ok(())
}

/// The subplate's thickness and bore spacing, when `a` is one.
#[must_use]
pub fn figures(a: &Accessory) -> Option<(f64, f64)> {
    match *a {
        Accessory::RotarySubplate { thickness, bore_spacing, .. } => Some((thickness, bore_spacing)),
        _ => None,
    }
}
