//! The lathe chucks as built-in accessories (#1442): their generic
//! default figures and their fit to a setup's round stock. The chucks
//! themselves - figures, law, shells - are `camcraft_sim::chuck`.
//!
//! Provenance: our own generic figures from general machining knowledge
//! (a 160 mm 3-jaw, a 200 mm 4-jaw, a small collet chuck); no vendor
//! catalogue or model copied.

use super::{Accessory, ColletChuck, JawChuck};
use crate::geometry::{Aabb, Point3};

/// Is `a` a lathe chuck?
#[must_use]
pub fn is_chuck(a: &Accessory) -> bool {
    matches!(a, Accessory::ThreeJawChuck(_) | Accessory::FourJawChuck(_) | Accessory::ColletChuck(_))
}

/// The generic chuck of kind `kind`; None for any other kind.
#[must_use]
pub fn generic(kind: &str) -> Option<Accessory> {
    let jaw = JawChuck {
        body_diameter: 160.0,
        body_length: 70.0,
        bore_diameter: 40.0,
        grip_diameter: 40.0,
        jaw_length: 35.0,
        jaw_width: 20.0,
        jaw_height: 30.0,
    };
    Some(match kind {
        "three_jaw_chuck" => Accessory::ThreeJawChuck(jaw),
        "four_jaw_chuck" => Accessory::FourJawChuck(JawChuck { body_diameter: 200.0, body_length: 80.0, jaw_length: 50.0, ..jaw }),
        "collet_chuck" => Accessory::ColletChuck(ColletChuck {
            body_diameter: 100.0,
            body_length: 40.0,
            nose_diameter: 50.0,
            nose_length: 30.0,
            collet_bore: 20.0,
        }),
        _ => return None,
    })
}

/// Chuck `a` closed on the round stock `stock` stands as (standing on a
/// vertical spindle axis), and where its frame's origin goes: on the
/// stock's axis, a jaw chuck's face at the stock's underside, a collet
/// chuck's nose face its nose length above it (the stock that deep in
/// the collet). Refused by name for a kind that is not a chuck, stock
/// that is not round, or a chuck the grip makes impossible.
pub fn fit_to_stock(a: &Accessory, stock: &Aabb) -> Result<(Accessory, Point3), String> {
    let (d, under) = camcraft_sim::chuck::round_stock(stock.min, stock.max)?;
    let (fit, lift) = match a {
        Accessory::ThreeJawChuck(c) => (Accessory::ThreeJawChuck(c.gripping(d)), 0.0),
        Accessory::FourJawChuck(c) => (Accessory::FourJawChuck(c.gripping(d)), 0.0),
        Accessory::ColletChuck(c) => (Accessory::ColletChuck(c.gripping(d)), c.nose_length),
        other => return Err(format!("only a lathe chuck fits to the stock, not a {}", other.label())),
    };
    fit.check().map_err(|e| format!("closed on the {d:.3} mm stock: {e}"))?;
    Ok((fit, Point3::new(under.x, under.y, under.z + lift)))
}
