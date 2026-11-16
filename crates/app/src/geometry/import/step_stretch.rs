//! An affine stretch of a B-spline or NURBS STEP face's parameters, so the
//! mesher's parameter plane is about as long in each direction as the face
//! is in space (#1602).
//!
//! Provenance: reparametrisation by a linear change of parameter (Piegl &
//! Tiller, "The NURBS Book", 2nd ed., sec. 6.4: u = f(s) linear leaves the
//! surface and its continuity unchanged and scales the derivatives by the
//! constant f'); the reason it matters is the constrained Delaunay
//! triangulation truck meshes a trimmed face with in its parameter plane
//! (ricosjp, Apache-2.0; Delaunay's empty-circle rule is isotropic). Our
//! own design around both. Nothing from any CAM vendor.
//!
//! Why: truck triangulates a trimmed face in (u, v), keeping a triangle
//! when its circumcircle holds no other point - a rule that assumes a unit
//! of u and a unit of v are about the same length on the face. A thread
//! flank exported as one rational strip is 1.5 mm across u and ~500 mm
//! along v (fourteen turns): its parameter square is 350 times too short
//! in u. Where the grid leaves the plane free (next to the boundary, or
//! everywhere when the face is straight across u), the empty-circle rule
//! then joins a boundary point to the far end of the strip: triangles a
//! quarter of the face long, cutting across the turns, on the #1602 models.
//!
//! How: the mean speed of the face along u and along v (the length of
//! `S_u`, `S_v`) is sampled over its knot domain, and the faster direction
//! is stretched by their ratio, so a parameter unit is about as long on
//! the face either way; the slower direction keeps its own numbers. A face
//! within [`THRESHOLD`] of square is left as it was, so a face that
//! meshed well before meshes exactly as before.

use truck_stepio::r#in::alias::{InnerSpace, ParametricSurface, Surface};

/// Below this speed ratio a face keeps its own parameters.
const THRESHOLD: f64 = 4.0;
/// The largest stretch applied: past it a face is a sliver whose speed
/// sample is not to be trusted with the parameter plane's numbers.
const MAX_STRETCH: f64 = 1.0e6;
/// Samples per direction for the mean speed.
const SAMPLES: usize = 16;

/// How much each parameter is multiplied by in the mesher's plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Stretch {
    pub u: f64,
    pub v: f64,
}

impl Stretch {
    /// No stretch: the surface's own parameters.
    pub(super) const NONE: Stretch = Stretch { u: 1.0, v: 1.0 };

    /// The stretch for `surface`: [`Stretch::NONE`] for a kind that is not
    /// a B-spline or NURBS, a face near square, or one whose speeds cannot
    /// be measured.
    pub(super) fn of(surface: &Surface) -> Stretch {
        let Some(((u0, u1), (v0, v1))) = super::step_pole::knot_domain(surface) else {
            return Stretch::NONE;
        };
        let (mut su, mut sv) = (0.0, 0.0);
        for i in 0..SAMPLES {
            let u = u0 + (u1 - u0) * (i as f64 + 0.5) / SAMPLES as f64;
            for j in 0..SAMPLES {
                let v = v0 + (v1 - v0) * (j as f64 + 0.5) / SAMPLES as f64;
                su += surface.uder(u, v).magnitude();
                sv += surface.vder(u, v).magnitude();
            }
        }
        Self::from_speeds(su, sv)
    }

    /// The stretch for mean speeds `su` along u and `sv` along v (any
    /// common factor).
    fn from_speeds(su: f64, sv: f64) -> Stretch {
        // A zero, NaN or infinite speed gives a ratio outside the range.
        let ratio = (su / sv).max(sv / su);
        if !(THRESHOLD..=MAX_STRETCH).contains(&ratio) {
            return Stretch::NONE;
        }
        if su > sv {
            Stretch { u: ratio, v: 1.0 }
        } else {
            Stretch { u: 1.0, v: ratio }
        }
    }

    /// Mesher-plane parameters to the surface's own.
    pub(super) fn inner(self, (u, v): (f64, f64)) -> (f64, f64) {
        (u / self.u, v / self.v)
    }

    /// The surface's own parameters to the mesher's plane.
    pub(super) fn outer(self, (u, v): (f64, f64)) -> (f64, f64) {
        (u * self.u, v * self.v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_face_near_square_keeps_its_parameters() {
        assert_eq!(Stretch::from_speeds(1.0, 3.9), Stretch::NONE);
        assert_eq!(Stretch::from_speeds(3.9, 1.0), Stretch::NONE);
        assert_eq!(Stretch::from_speeds(1.0, 0.0), Stretch::NONE);
        assert_eq!(Stretch::from_speeds(f64::NAN, 1.0), Stretch::NONE);
        assert_eq!(Stretch::from_speeds(1.0, 2.0e6), Stretch::NONE, "past the cap");
    }

    #[test]
    fn the_faster_direction_is_stretched_by_the_ratio() {
        assert_eq!(Stretch::from_speeds(0.5, 175.0), Stretch { u: 1.0, v: 350.0 });
        assert_eq!(Stretch::from_speeds(40.0, 4.0), Stretch { u: 10.0, v: 1.0 });
        let s = Stretch { u: 1.0, v: 350.0 };
        assert_eq!(s.inner(s.outer((0.25, 0.5))), (0.25, 0.5));
    }
}
