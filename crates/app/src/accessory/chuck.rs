//! The lathe chucks as built-in accessories (#1442): their generic
//! default figures, their fit to a setup's round stock and (#1468) the
//! jaw chucks' grip direction. The chucks themselves - figures, law,
//! shells - are `camcraft_sim::chuck`.
//!
//! Provenance: our own generic figures from general machining knowledge
//! (a 160 mm 3-jaw, a 200 mm 4-jaw, a small collet chuck); no vendor
//! catalogue or model copied.

use super::{Accessory, ColletChuck, JawChuck};
use crate::geometry::{Aabb, Point3};

pub use camcraft_sim::chuck::Grip;

/// The jaw chucks' figures that may be zero (plain jaws): the jaws' law
/// reads them, not the accessory law's more-than-zero rule.
pub const MAY_BE_ZERO: [&str; 2] = ["step_height", "step_depth"];

/// The figure a fit to the stock sets, by kind (`None`: not a chuck).
#[must_use]
pub fn fitted_dim(a: &Accessory) -> Option<&'static str> {
    match a {
        Accessory::ThreeJawChuck(_) | Accessory::FourJawChuck(_) => Some("grip_diameter"),
        Accessory::ColletChuck(_) => Some("collet_bore"),
        _ => None,
    }
}

/// Does stock box `stock` read as round stock a chuck fits on?
#[must_use]
pub fn stock_is_round(stock: &Aabb) -> bool {
    camcraft_sim::chuck::round_stock(stock.min, stock.max).is_ok()
}

impl Accessory {
    /// A jaw chuck's grip direction (`None`: no jaws).
    #[must_use]
    pub fn grip(&self) -> Option<Grip> {
        match self {
            Self::ThreeJawChuck(c) | Self::FourJawChuck(c) => Some(c.grip),
            _ => None,
        }
    }

    /// A jaw chuck's grip direction, to set.
    pub fn grip_mut(&mut self) -> Option<&mut Grip> {
        match self {
            Self::ThreeJawChuck(c) | Self::FourJawChuck(c) => Some(&mut c.grip),
            _ => None,
        }
    }

    /// The same jaw chuck gripping `key`'s way (`outside`, `inside`);
    /// refused by name for another kind or key.
    pub fn with_grip(mut self, key: &str) -> Result<Self, String> {
        let kind = self.label();
        let grip = Grip::of(key).ok_or_else(|| format!("no grip \"{key}\" (one of {})", Grip::KEYS.join(", ")))?;
        *self.grip_mut().ok_or_else(|| format!("a {kind} has no jaws to grip {key}"))? = grip;
        Ok(self)
    }
}

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
        grip: Grip::Outside,
        step_height: 0.0,
        step_depth: 0.0,
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
/// stock's axis, a jaw chuck's face at the stock's underside (stepped
/// jaws, #1468: the step height below it, the stock on the step), a
/// collet chuck's nose face its nose length above it (the stock that
/// deep in the collet). Refused by name for a kind that is not a chuck,
/// stock that is not round, an inside grip (the stock's box does not
/// give the bore it opens into), or a chuck the grip makes impossible.
pub fn fit_to_stock(a: &Accessory, stock: &Aabb) -> Result<(Accessory, Point3), String> {
    let (d, under) = camcraft_sim::chuck::round_stock(stock.min, stock.max)?;
    let (fit, lift) = match a {
        Accessory::ThreeJawChuck(c) => (Accessory::ThreeJawChuck(outside(c)?.gripping(d)), -seat(c)),
        Accessory::FourJawChuck(c) => (Accessory::FourJawChuck(outside(c)?.gripping(d)), -seat(c)),
        Accessory::ColletChuck(c) => (Accessory::ColletChuck(c.gripping(d)), c.nose_length),
        other => return Err(format!("only a lathe chuck fits to the stock, not a {}", other.label())),
    };
    fit.check().map_err(|e| format!("closed on the {d:.3} mm stock: {e}"))?;
    Ok((fit, Point3::new(under.x, under.y, under.z + lift)))
}

/// How high over the chuck's face the work sits: on the step of stepped
/// jaws, else on the face.
fn seat(c: &JawChuck) -> f64 {
    if c.stepped() { c.step_height } else { 0.0 }
}

/// `c`, when it grips outside: only that fits the stock's outside.
fn outside(c: &JawChuck) -> Result<JawChuck, String> {
    match c.grip {
        Grip::Outside => Ok(*c),
        Grip::Inside => Err("an inside grip opens into the work's bore, which the stock's box does not give: \
                             set the grip diameter to the bore and place the chuck by position"
            .into()),
    }
}
