//! The solidcore (SMLib port) impl of the kernel seam: primitives, the three
//! booleans, whole offset, edge blends (solidcore_blend.rs), and exact measure via the port's mass-property
//! integrator and tight bounding box.
//! Provenance: calls solidcore's public API (first written against 1.0.1,
//! now the commit pinned in Cargo.toml, #916) as documented in its rustdoc
//! and its own public tests (boolean_api.rs, offset fixtures); no
//! SMLib/solidcore internals reimplemented here.

use solidcore::boolean::api_brep::{boolean_difference, boolean_intersection, boolean_union};
use solidcore::geom::attribute::{AttributeStore, ObjectKey};
use solidcore::mass::integrator::properties::{PropertyAttributes, brep_volume};
use solidcore::math::Point3;
use solidcore::offset::offset_executive::p2::{OffsetOptions, offset_brep_simple};
use solidcore::primitive::api_primitives as prim;
use solidcore::query::brep::{BoundingBoxOptions, calculate_bounding_box};
use solidcore::topology::Brep;

use crate::{
    Blend, Kernel, KernelError, Measure, P3, finite_point, offset_distance, positive_len,
};

const NAME: &str = "solidcore";

/// Relative accuracy asked of the port's volume integrator (the port's own
/// boolean tests run at this value).
pub const VOLUME_ACCURACY: f64 = 1e-7;

/// solidcore behind the seam. Holds the port's explicit kernel state
/// (tolerances, RNG, caches - SMLib's `SmContext`), default-constructed: the
/// default path matches SMLib 1.0.1 bit for bit, no opt-in upstream fixes.
#[derive(Default)]
pub struct SolidcoreKernel {
    /// The port's kernel state; public so the bench can flip opt-in fixes.
    pub k: solidcore::Kernel,
}

pub(crate) fn failed(op: &'static str, e: &solidcore::SolidError) -> KernelError {
    KernelError::Failed {
        kernel: NAME,
        op,
        detail: e.to_string(),
    }
}

fn p(v: P3) -> Point3 {
    Point3::new(v[0], v[1], v[2])
}

/// An operand with no faces is refused before the port sees it: the port
/// answers `box & empty` with the whole box, a wrong answer dressed as a
/// solid (pinned in tests/corrupt_input.rs), and an empty operand is a
/// caller bug for every boolean.
fn operands(op: &'static str, a: &Brep, b: &Brep) -> Result<(), KernelError> {
    if a.faces().is_empty() || b.faces().is_empty() {
        Err(KernelError::InvalidInput {
            op,
            detail: "empty operand (a B-rep with no faces)".into(),
        })
    } else {
        Ok(())
    }
}

/// A result must pass the port's integrity check and be a closed manifold
/// solid before the seam hands it out.
pub(crate) fn checked(op: &'static str, b: Brep) -> Result<Brep, KernelError> {
    solidcore::topology::integrity::check(&b).map_err(|e| KernelError::InvalidResult {
        kernel: NAME,
        op,
        detail: format!("integrity: {e}"),
    })?;
    let solid = solidcore::topology::brep::p2::is_manifold_solid(&b).map_err(|e| failed(op, &e))?;
    if solid {
        Ok(b)
    } else {
        Err(KernelError::InvalidResult {
            kernel: NAME,
            op,
            detail: "not a closed manifold solid (empty or open result)".into(),
        })
    }
}

impl Kernel for SolidcoreKernel {
    type Solid = Brep;

    fn name(&self) -> &'static str {
        NAME
    }

    fn make_box(&mut self, origin: P3, size: P3) -> Result<Brep, KernelError> {
        finite_point("make_box", origin)?;
        for (axis, v) in ["x", "y", "z"].iter().zip(size) {
            positive_len("make_box", axis, v)?;
        }
        let b = prim::create_box(&mut self.k, p(origin), size[0], size[1], size[2])
            .map_err(|e| failed("make_box", &e))?;
        checked("make_box", b)
    }

    fn make_cylinder(&mut self, base: P3, radius: f64, height: f64) -> Result<Brep, KernelError> {
        finite_point("make_cylinder", base)?;
        positive_len("make_cylinder", "radius", radius)?;
        positive_len("make_cylinder", "height", height)?;
        let b = prim::create_cylinder(&mut self.k, p(base), radius, height)
            .map_err(|e| failed("make_cylinder", &e))?;
        checked("make_cylinder", b)
    }

    fn boolean_subtract(&mut self, a: Brep, b: Brep) -> Result<Brep, KernelError> {
        operands("boolean_subtract", &a, &b)?;
        let r = boolean_difference(&mut self.k, a, b).map_err(|e| failed("boolean_subtract", &e))?;
        checked("boolean_subtract", r)
    }

    fn boolean_union(&mut self, a: Brep, b: Brep) -> Result<Brep, KernelError> {
        operands("boolean_union", &a, &b)?;
        let r = boolean_union(&mut self.k, a, b).map_err(|e| failed("boolean_union", &e))?;
        checked("boolean_union", r)
    }

    fn boolean_intersect(&mut self, a: Brep, b: Brep) -> Result<Brep, KernelError> {
        operands("boolean_intersect", &a, &b)?;
        let r = boolean_intersection(&mut self.k, a, b)
            .map_err(|e| failed("boolean_intersect", &e))?;
        checked("boolean_intersect", r)
    }

    fn blend_edges(
        &mut self,
        s: Brep,
        picks: &[P3],
        radius: f64,
        kind: Blend,
    ) -> Result<Brep, KernelError> {
        crate::solidcore_blend::blend_edges(self, s, picks, radius, kind)
    }

    fn heal(&mut self, s: Brep) -> Result<Brep, KernelError> {
        crate::solidcore_import::heal(self, s)
    }

    /// Guards around the port's offset, each one answering a bench finding
    /// (see tests/corrupt_input.rs `KNOWN_PORT_FINDINGS`):
    /// - `merge_results: true` - with SMLib's default `false`, a plain box
    ///   inset by 1 comes back `Ok` as a one-region, zero-volume B-rep;
    /// - an inset of at least half the source's thinnest bbox extent is
    ///   refused up front - the port returns a manifold, positive-volume
    ///   solid for a 10-box inset by 6 (an inverted 2-cube, not empty);
    /// - the result's volume must move the way the sign says (inset
    ///   smaller, outset larger) - solidcore 1.0.1 returned an unchanged
    ///   copy for a NaN distance (the pinned commit of #916 refuses it),
    ///   which this law also catches downstream.
    ///
    /// The bbox guard only sees the overall extent: a thin wall in a large
    /// part can still collapse unseen - the monotonicity law is the backstop,
    /// not a proof (stage-2 work, recorded in the hand-off).
    fn offset(&mut self, s: &mut Brep, distance: f64) -> Result<Brep, KernelError> {
        offset_distance(distance)?;
        let before = self.measure(s)?;
        if distance < 0.0 {
            let thinnest = (0..3)
                .map(|i| before.max[i] - before.min[i])
                .fold(f64::INFINITY, f64::min);
            if -distance * 2.0 >= thinnest {
                return Err(KernelError::InvalidInput {
                    op: "offset",
                    detail: format!(
                        "inset {} collapses a solid whose thinnest extent is {thinnest}",
                        -distance
                    ),
                });
            }
        }
        let opts = OffsetOptions {
            distance,
            extend_convex_edges: false,
            merge_results: true,
        };
        let r = offset_brep_simple(&mut self.k, s, opts).map_err(|e| failed("offset", &e))?;
        let mut r = checked("offset", r)?;
        let after = self.measure(&mut r)?;
        let moved_right = if distance > 0.0 {
            after.volume > before.volume
        } else {
            after.volume < before.volume
        };
        if moved_right {
            Ok(r)
        } else {
            Err(KernelError::InvalidResult {
                kernel: NAME,
                op: "offset",
                detail: format!(
                    "offset {distance} moved the volume the wrong way ({} -> {})",
                    before.volume, after.volume
                ),
            })
        }
    }

    fn measure(&mut self, s: &mut Brep) -> Result<Measure, KernelError> {
        let store = AttributeStore::default();
        let key_of = |_| ObjectKey(0);
        let attrs = PropertyAttributes {
            store: &store,
            key_of: &key_of,
        };
        let volume =
            brep_volume(&self.k, s, VOLUME_ACCURACY, &attrs).map_err(|e| failed("measure", &e))?;
        let opts = BoundingBoxOptions {
            approx: false,
            tight: true,
            expand_by_zone_tol_3d: false,
        };
        let bb = calculate_bounding_box(&self.k, &mut *s, opts).map_err(|e| failed("measure", &e))?;
        let (lo, hi) = (bb.min(), bb.max());
        let m = Measure {
            volume,
            min: [lo.x, lo.y, lo.z],
            max: [hi.x, hi.y, hi.z],
        };
        let finite = m.volume.is_finite() && m.min.iter().chain(&m.max).all(|c| c.is_finite());
        // An empty or inside-out B-rep integrates to <= 0: not a solid.
        if finite && m.volume > 0.0 {
            Ok(m)
        } else {
            Err(KernelError::InvalidResult {
                kernel: NAME,
                op: "measure",
                detail: format!("not a positive finite solid measure {m:?}"),
            })
        }
    }
}
