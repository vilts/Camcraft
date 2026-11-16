//! Sinker EDM orbit patterns (#544): the electrode's lateral excursion
//! at the burn floor - circular, vector or planetary - expanded into
//! explicit XY offsets, so the burn's CL carries every move the
//! electrode makes and the 3-axis stock sim verifies exactly what posts
//! (#290 D3). A canned orbit word is the control's business and is not
//! emitted here.
//!
//! Provenance: orbital (planetary) die-sinking as described in public
//! EDM machining texts and machine-tool handbooks - after the plunge
//! the electrode translates in the XY plane so its face sweeps beyond
//! its own outline by the orbit radius: on a circle (circular), out and
//! back along straight radial vectors (vector), or on a radius growing
//! turn by turn from zero (planetary). The chordal discretisation and
//! the refusals are our own design (#290 D6). No vendor material.
//!
//! Exactness: every vertex lies ON or INSIDE the circle of the stated
//! radius (`r cos`, `r sin` of the vertex angle, scaled down when a
//! rounding would place it one ulp beyond), so a chord - the segment
//! between two points of a disc - never leaves the disc: no orbit move
//! overshoots its radius. The circle's vertices fall on the four axis
//! directions, so a rectangular electrode's walls move out by exactly
//! the radius.

use crate::machining2d::GenError;

/// The orbit geometries CamCraft generates (#290 D6). The machine
/// lists which it offers under `edm.orbits`, by [`OrbitKind::word`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrbitKind {
    /// A full circle of the radius at the burn floor.
    Circular,
    /// Straight out to the radius and back, along evenly spaced
    /// directions starting at +X.
    Vector,
    /// A spiral from the centre out to the radius over a number of
    /// turns, closed by a full circle of the radius.
    Planetary,
}

impl OrbitKind {
    /// The geometry's name, as a machine's `edm.orbits[].kind` states it.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            OrbitKind::Circular => "circular",
            OrbitKind::Vector => "vector",
            OrbitKind::Planetary => "planetary",
        }
    }
}

/// One orbit: the pattern, its radius and its counts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    pub kind: OrbitKind,
    /// The largest excursion of the electrode centre from the burn
    /// point, mm. Every move stays within it.
    pub radius: f64,
    /// Vector: how many evenly spaced directions (from +X). Ignored by
    /// the other patterns.
    pub vectors: u32,
    /// Planetary: how many turns the spiral takes to reach the radius.
    /// Ignored by the other patterns.
    pub turns: u32,
}

/// The largest chord sagitta (deviation inside the circle), mm.
pub const ORBIT_CHORD_TOL: f64 = 0.001;
/// Bounds on the counts, so an orbit never balloons into a hang.
pub const MAX_VECTORS: u32 = 72;
pub const MAX_TURNS: u32 = 20;
/// Bounds on the chords per full turn.
const MIN_CHORDS: u32 = 8;
const MAX_CHORDS: u32 = 1440;

impl Orbit {
    /// Structural sanity, no machine: the radius finite-positive and
    /// the pattern's count in range. The refusal says which, by name.
    ///
    /// # Errors
    /// The first number out of range, in words.
    pub fn check(&self) -> Result<(), GenError> {
        let k = self.kind.word();
        if !(self.radius.is_finite() && self.radius > 0.0) {
            return Err(GenError::Invalid(format!(
                "sinker orbit: a {k} orbit of radius {} moves nothing - state a positive radius",
                self.radius
            )));
        }
        if self.kind == OrbitKind::Vector && !(1..=MAX_VECTORS).contains(&self.vectors) {
            return Err(GenError::Invalid(format!(
                "sinker orbit: a vector orbit takes 1 to {MAX_VECTORS} directions, not {}",
                self.vectors
            )));
        }
        if self.kind == OrbitKind::Planetary && !(1..=MAX_TURNS).contains(&self.turns) {
            return Err(GenError::Invalid(format!(
                "sinker orbit: a planetary orbit takes 1 to {MAX_TURNS} turns, not {}",
                self.turns
            )));
        }
        Ok(())
    }
}

/// Chords per full turn of radius `r`: enough that the sagitta stays
/// under [`ORBIT_CHORD_TOL`], a multiple of four (so the axis
/// directions are vertices), clamped.
fn chords_per_turn(r: f64) -> u32 {
    let c = 1.0 - ORBIT_CHORD_TOL / r;
    let n = if c <= -1.0 { MIN_CHORDS as f64 } else { (std::f64::consts::PI / c.acos()).ceil() };
    let n = if n.is_finite() { n.clamp(MIN_CHORDS as f64, MAX_CHORDS as f64) as u32 } else { MAX_CHORDS };
    n.div_ceil(4) * 4
}

/// The point at distance `rho` (<= `r`) and angle `a`, never past `r`.
fn on_disc(rho: f64, a: f64, r: f64) -> (f64, f64) {
    let (x, y) = (rho * a.cos(), rho * a.sin());
    let h = x.hypot(y);
    if h > r {
        let s = r / h;
        // Shrinking by r/h can still round up; step toward zero until in.
        let (mut x, mut y) = (x * s, y * s);
        while x.hypot(y) > r {
            x = x.next_down_toward_zero();
            y = y.next_down_toward_zero();
        }
        (x, y)
    } else {
        (x, y)
    }
}

trait TowardZero {
    fn next_down_toward_zero(self) -> Self;
}

impl TowardZero for f64 {
    fn next_down_toward_zero(self) -> f64 {
        if self == 0.0 {
            0.0
        } else {
            f64::from_bits(self.to_bits() - 1)
        }
    }
}

/// The orbit's XY offsets from the burn point, in order, starting after
/// the centre (where the plunge ends) and ending back on it. Every
/// offset lies within the radius.
///
/// # Errors
/// [`Orbit::check`]'s refusals.
pub fn orbit_offsets(o: &Orbit) -> Result<Vec<(f64, f64)>, GenError> {
    o.check()?;
    let r = o.radius;
    let tau = std::f64::consts::TAU;
    let n = chords_per_turn(r);
    let circle = |out: &mut Vec<(f64, f64)>| {
        for k in 1..=n {
            out.push(if k == n { (r, 0.0) } else { on_disc(r, tau * f64::from(k) / f64::from(n), r) });
        }
    };
    let mut out = Vec::new();
    match o.kind {
        OrbitKind::Circular => {
            out.push((r, 0.0));
            circle(&mut out);
        }
        OrbitKind::Vector => {
            for k in 0..o.vectors {
                let a = tau * f64::from(k) / f64::from(o.vectors);
                out.push(if k == 0 { (r, 0.0) } else { on_disc(r, a, r) });
                out.push((0.0, 0.0));
            }
            return Ok(out);
        }
        OrbitKind::Planetary => {
            let steps = n.saturating_mul(o.turns);
            for k in 1..=steps {
                let f = f64::from(k) / f64::from(steps);
                out.push(if k == steps { (r, 0.0) } else { on_disc(r * f, tau * f64::from(k) / f64::from(n), r) });
            }
            circle(&mut out);
        }
    }
    out.push((0.0, 0.0));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn orbit(kind: OrbitKind, radius: f64) -> Orbit {
        Orbit { kind, radius, vectors: 4, turns: 3 }
    }

    #[test]
    fn sinker_orbit_every_pattern_stays_inside_its_radius_and_closes() {
        for kind in [OrbitKind::Circular, OrbitKind::Vector, OrbitKind::Planetary] {
            for r in [0.0005, 0.05, 0.3, 1.7, 12.0] {
                let pts = orbit_offsets(&orbit(kind, r)).expect("an orbit");
                assert!(pts.iter().all(|(x, y)| x.hypot(*y) <= r), "{kind:?} r {r} overshoots");
                assert_eq!(pts.last(), Some(&(0.0, 0.0)), "{kind:?} returns to the centre");
                assert!(pts.contains(&(r, 0.0)), "{kind:?} reaches the radius exactly");
            }
        }
    }

    #[test]
    fn sinker_orbit_circle_holds_its_chord_tolerance_on_the_axes() {
        let r = 0.5;
        let n = chords_per_turn(r);
        assert_eq!(n % 4, 0);
        assert!(r * (1.0 - (std::f64::consts::PI / f64::from(n)).cos()) <= ORBIT_CHORD_TOL);
        let pts = orbit_offsets(&orbit(OrbitKind::Circular, r)).expect("circle");
        let at = |x: f64, y: f64| pts.iter().any(|p| (p.0 - x).abs() < 1e-12 && (p.1 - y).abs() < 1e-12);
        assert!(at(r, 0.0) && at(0.0, r) && at(-r, 0.0) && at(0.0, -r), "the axis directions are vertices");
    }

    #[test]
    fn sinker_orbit_refuses_garbled_numbers_by_name() {
        let e = orbit_offsets(&orbit(OrbitKind::Circular, 0.0)).expect_err("zero");
        assert!(e.to_string().contains("state a positive radius"), "{e}");
        let e = orbit_offsets(&Orbit { vectors: 0, ..orbit(OrbitKind::Vector, 0.2) }).expect_err("no vectors");
        assert!(e.to_string().contains("directions"), "{e}");
        let e = orbit_offsets(&Orbit { turns: 99, ..orbit(OrbitKind::Planetary, 0.2) }).expect_err("too many turns");
        assert!(e.to_string().contains("turns"), "{e}");
        assert!(orbit_offsets(&orbit(OrbitKind::Circular, f64::NAN)).is_err());
    }
}
