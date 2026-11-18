//! The one machining-frame seam (#763): the single door an operation's
//! world reads and CL wrap go through, composing the two existing frame
//! mechanisms — the setup's `part_alignment` (#326) and the op's 3+2
//! `Tilt` (M5.1 B) — without adding a third.
//!
//! Provenance: our own frame doctrine, specced from the repo's public
//! design notes; rigid-body math, no vendor implementation inspected.
//!
//! ## The contract
//!
//! Three coordinate systems, never a fourth:
//!
//! - **D** — the document frame (as modelled); part entities carry
//!   their `Location` in D.
//! - **S** — the setup frame: the table frame, and the world the CL
//!   stream and the post speak. The stock, the work offsets and the
//!   fixtures are setup FURNITURE — stored IN S already (#326's law),
//!   so the alignment never applies to them; only the tilt reads them
//!   local.
//! - **L** — the local machining frame: XY the cutting plane, +Z the
//!   tool axis; exists only while the op tilts.
//!
//! Point maps, right-to-left application (the `setup_align::compose`
//! convention — the rightmost map applies first):
//!
//! - D → S: `A` = the setup's `part_alignment`, baked into the PART
//!   reads (`facets_in` and siblings, reused verbatim below). Never
//!   inverted — the CL never maps back to D.
//! - S → L: `T` = the op's tilt frame (`TiltedFrame::to_local`),
//!   composed AFTER the alignment read: `p_L = T(A(p_D))`.
//! - L → S (the CL out): `T.to_world`, the tilt's inverse — one wrap
//!   over the whole stream ([`Self::world_path`]), a clone when no
//!   tilt exists (the identity law: bits included).
//!
//! Heights and bounds in L resolve against FULLY TRANSFORMED geometry —
//! the box of `T(A(f))`, never a `z - origin.z` shift of the S box: a
//! Z shift is only correct while the tool axis is the world Z, and the
//! seam must be right for every tilt it will ever carry. Machine
//! capability stays gated where it is: a tilted orientation rides the
//! stream stamped, never flattened to plain XYZ, and the post's
//! tilted-move refusals keep their verdicts (`tests/inverted_post.rs`
//! pins them).

use flecs_ecs::prelude::{Entity, World};

use super::*;
use crate::geometry::Aabb;

/// One operation's frame story: the setup it runs in, and its tilt.
/// The alignment is not held here — it is the setup's own state, read
/// by the `setup_align` helpers at every S-frame read below (one
/// owner, no second copy to drift).
#[derive(Clone, Debug)]
pub struct OpFrame {
    /// The op's declared setup (`None` = the first setup, the
    /// implicit-setup law of [`crate::setup`]).
    setup: Option<String>,
    /// The stored 3+2 letters, never normalized — the letters ARE the
    /// claim. `None` is no index: the ordinary prismatic op.
    tilt: Option<Tilt>,
    /// The letters' one reading ([`Tilt::frame`]); also `None` when
    /// the letters state no direction, which [`Self::checked_frame`]
    /// refuses by name — never guessed here.
    frame: Option<crate::cl::TiltedFrame>,
}

impl OpFrame {
    /// Read an operation's frame story.
    #[must_use]
    pub fn of(op: &Operation) -> Self {
        Self::new(op.setup.clone(), op.params.tilt())
    }

    /// The same construction from the pieces (the seam's tests, and the
    /// later phases' call sites, hold no `Operation`).
    #[must_use]
    pub fn new(setup: Option<String>, tilt: Option<Tilt>) -> Self {
        let frame = tilt.and_then(|t| t.frame());
        Self { setup, tilt, frame }
    }

    /// The tilt frame, refusing letters that state no direction with
    /// the parameter door's own words. The refusal's PRECEDENCE stays
    /// the caller's: `collect_job` asks after the heights resolve,
    /// exactly where the old head asked (the job's error-order law).
    pub fn checked_frame(&self) -> Result<Option<crate::cl::TiltedFrame>, GenError> {
        if self.tilt.is_some() && self.frame.is_none() {
            return Err(GenError::Invalid(boundary_ctx::TILT_NO_DIRECTION.into()));
        }
        Ok(self.frame)
    }

    // ---- S-frame reads: the alignment applied, never the tilt ----

    /// A part entity's pose in S (#763 Phase 2: an aligned part reads
    /// composed with the setup's alignment; fixtures, furniture, and an
    /// unaligned setup read `transform_of` unchanged).
    #[must_use]
    pub fn pose_in_setup(&self, world: &World, entity: &Entity) -> Option<crate::schema::Location> {
        crate::setup_align::pose_in(world, entity, self.setup.as_deref())
    }

    /// A part entity's facets in S (#326: an aligned part reads turned;
    /// an unaligned setup reads exactly the world facets it always
    /// did). This is the door Phase 2 reads picks through too.
    #[must_use]
    pub fn facets_in_setup(&self, world: &World, entity: &Entity) -> Option<Vec<Triangle>> {
        crate::setup_align::facets_in(world, entity, self.setup.as_deref())
    }

    /// One entity's bounds in S.
    #[must_use]
    pub fn entity_aabb_in_setup(&self, world: &World, entity: &Entity) -> Option<Aabb> {
        crate::setup_align::entity_aabb_in(world, entity, self.setup.as_deref())
    }

    /// The whole solid's bounds in S.
    #[must_use]
    pub fn solid_aabb_in_setup(&self, world: &World) -> Option<Aabb> {
        crate::setup_align::solid_aabb_in(world, self.setup.as_deref())
    }

    /// The parts (fixtures left out) as facets in S.
    #[must_use]
    pub fn parts_facets_in_setup(&self, world: &World) -> Vec<Triangle> {
        crate::setup_align::parts_facets_in(world, self.setup.as_deref())
    }

    /// Setup furniture (the stock, the WCS, fixtures): stored IN S, so
    /// the alignment NEVER applies (#326's furniture law). The tilt
    /// still reads them local ([`Self::local_facets`]).
    #[must_use]
    pub fn furniture_facets(&self, world: &World, entity: &Entity) -> Option<Vec<Triangle>> {
        model::world_facets(world, entity)
    }

    // ---- heights: today's S-frame resolve, what collect_job stores ----

    /// The height bag every kind resolves today: S-frame world Zs,
    /// alignment-aware, unshifted (the tilt's own fold stays
    /// `run_job`'s, unchanged by this seam).
    pub fn resolve_heights(
        &self,
        world: &World,
        src: Option<&SrcRef>,
        params: &OpParams,
    ) -> Result<ResolvedHeights, String> {
        resolve_params(world, self.setup.as_deref(), src, params)
    }

    /// The op's material as generation judges it (#904 `judged_material`:
    /// the excess also covers the setup's stock), read through the seam
    /// (the S-frame read `collect_job` always made — one name on it now).
    pub(super) fn material_of(&self, world: &World, src: &SrcRef) -> Result<Material, GenError> {
        super::material::judged_material(world, self.setup.as_deref(), src)
    }

    // ---- the composed seam: L-frame math (fully transformed) ----

    /// S-facets into L: the tilt applied, a clone when none exists.
    #[must_use]
    pub fn local_facets(&self, facets_s: &[Triangle]) -> Vec<Triangle> {
        match &self.frame {
            Some(f) => f.local_facets(facets_s),
            None => facets_s.to_vec(),
        }
    }

    /// A world (S-frame) point's height along the tool axis.
    #[must_use]
    pub fn z_local(&self, p: Point3) -> f64 {
        match &self.frame {
            Some(f) => f.z_local(p),
            None => p.z,
        }
    }

    /// The tool axis in S: the world's +Z when no tilt exists, the
    /// tilt's commanded axis otherwise — the direction the post's
    /// tilted-move gates read.
    #[must_use]
    pub fn tool_axis(&self) -> [f64; 3] {
        self.frame.map_or([0.0, 0.0, 1.0], |f| f.axis)
    }

    /// The local bounds of S-facets: the box of the FULLY TRANSFORMED
    /// geometry — never a shift of the S box (a Z shift is not
    /// generally correct for a tilted plane).
    #[must_use]
    pub fn aabb_local_of(&self, facets_s: &[Triangle]) -> Option<Aabb> {
        let local = self.local_facets(facets_s);
        bounds(local.iter().flat_map(|t| [t.a, t.b, t.c]))
    }

    /// The solid's bounds in L.
    #[must_use]
    pub fn solid_aabb_local(&self, world: &World) -> Option<Aabb> {
        self.aabb_local_of(&self.parts_facets_in_setup(world))
    }

    /// One entity's bounds in L.
    #[must_use]
    pub fn entity_aabb_local(&self, world: &World, entity: &Entity) -> Option<Aabb> {
        self.facets_in_setup(world, entity)
            .and_then(|f| self.aabb_local_of(&f))
    }

    /// Furniture bounds in L (the stock a tilted cut plane stands in).
    #[must_use]
    pub fn furniture_aabb_local(&self, world: &World, entity: &Entity) -> Option<Aabb> {
        self.furniture_facets(world, entity)
            .and_then(|f| self.aabb_local_of(&f))
    }

    /// A height reference resolved in L, against fully transformed
    /// geometry: Top/BottomOfModel read the local solid bounds,
    /// Top/BottomOfStock the furniture's local bounds, WorkOffset the
    /// WCS origin's [`Self::z_local`], Fixed passes through as spoken
    /// along the tool axis. A Selection reference stays `None` here —
    /// a tilted op refuses picked selections until Phase 3, and a height
    /// is never guessed.
    #[must_use]
    pub fn height_z_local(&self, world: &World, h: Height) -> Option<f64> {
        let base = match h.reference {
            HeightReference::Fixed => return h.offset.is_finite().then_some(h.offset),
            HeightReference::TopOfModel => self.solid_aabb_local(world)?.max.z,
            HeightReference::BottomOfModel => self.solid_aabb_local(world)?.min.z,
            HeightReference::TopOfStock | HeightReference::BottomOfStock => {
                let name = crate::setup::stock_entity_name(world, self.setup.as_deref());
                let entity = model::find_by_name(world, &name)?;
                let b = self.furniture_aabb_local(world, &entity)?;
                if matches!(h.reference, HeightReference::TopOfStock) {
                    b.max.z
                } else {
                    b.min.z
                }
            }
            HeightReference::WorkOffset => {
                let entity = crate::setup::wcs_entity(world, self.setup.as_deref())?;
                let pos = model::transform_of(world, &entity).unwrap_or_default().pos;
                self.z_local(pos)
            }
            HeightReference::TopOfSelection | HeightReference::BottomOfSelection => return None,
        };
        (base + h.offset).is_finite().then_some(base + h.offset)
    }

    /// The CL wrap, L -> S, over the whole stream — the tilt's own
    /// contract (every record, arcs chorded when tilted, the pose
    /// stamped); a clone when no tilt exists.
    #[must_use]
    pub fn world_path(&self, local: &Toolpath, chord_tol: f64) -> Toolpath {
        match &self.frame {
            Some(f) => f.world_path(local, chord_tol),
            None => local.clone(),
        }
    }
}

/// The box of points (the `setup_align::bounds` law, mirrored here for
/// the seam's local reads — that one is private to its module).
fn bounds(points: impl Iterator<Item = Point3>) -> Option<Aabb> {
    let mut it = points;
    let first = it.next()?;
    let (mut min, mut max) = (first, first);
    for p in it {
        min.x = min.x.min(p.x);
        min.y = min.y.min(p.y);
        min.z = min.z.min(p.z);
        max.x = max.x.max(p.x);
        max.y = max.y.max(p.y);
        max.z = max.z.max(p.z);
    }
    Some(Aabb { min, max })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIGHT: f64 = 1e-12;

    fn tri(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Triangle {
        Triangle::new(
            Point3::new(a[0], a[1], a[2]),
            Point3::new(b[0], b[1], b[2]),
            Point3::new(c[0], c[1], c[2]),
        )
    }

    /// A 45-degree X index: axis = Rx(45)z = (0, -s, c); the Gram-Schmidt
    /// seed is X (axis has no X component), so u = X and v = axis x u =
    /// (0, c, s) — hand math, pinned against the frame the seam builds.
    fn t45x() -> Tilt {
        Tilt {
            basis_version: 1,
            a_deg: 45.0,
            b_deg: 0.0,
            origin: [0.0, 0.0, 0.0],
        }
    }

    #[test]
    fn identity_is_a_passthrough() {
        let f = OpFrame::new(None, None);
        assert!(f.checked_frame().expect("no tilt to refuse").is_none());
        let facets = vec![tri([1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0])];
        let local = f.local_facets(&facets);
        assert_eq!((local[0].a.x, local[0].a.y, local[0].a.z), (1.0, 2.0, 3.0));
        assert_eq!((local[0].b.x, local[0].b.y, local[0].b.z), (4.0, 5.0, 6.0));
        assert_eq!((local[0].c.x, local[0].c.y, local[0].c.z), (7.0, 8.0, 9.0));
        assert_eq!(f.tool_axis(), [0.0, 0.0, 1.0]);
        assert!((f.z_local(Point3::new(3.0, -4.0, 12.5)) - 12.5).abs() < TIGHT);
        let b = f.aabb_local_of(&facets).expect("a box");
        assert!((b.min.x - 1.0).abs() < TIGHT && (b.max.z - 9.0).abs() < TIGHT);
    }

    #[test]
    fn local_facets_compose_against_hand_math() {
        let f = OpFrame::new(None, Some(t45x()));
        let frame = f.checked_frame().expect("finite letters").expect("a frame");
        let (s, c) = (f64::to_radians(45.0).sin(), f64::to_radians(45.0).cos());
        let axis = [0.0, -s, c];
        let u = [1.0, 0.0, 0.0];
        let v = [0.0, c, s];
        for (got, want) in [(frame.axis, axis), (frame.u, u), (frame.v, v)] {
            for i in 0..3 {
                assert!(
                    (got[i] - want[i]).abs() < TIGHT,
                    "basis [{i}]: {got:?} vs {want:?}"
                );
            }
        }
        // p_L = ((p - o) . u, (p - o) . v, (p - o) . axis), by hand.
        let facets = vec![tri([3.0, -2.0, 7.0], [0.0, 0.0, 0.0], [10.0, 4.0, -1.0])];
        let local = f.local_facets(&facets);
        for (got, p) in [
            (local[0].a, [3.0, -2.0, 7.0]),
            (local[0].c, [10.0, 4.0, -1.0]),
        ] {
            let want = [
                p[0] * u[0] + p[1] * u[1] + p[2] * u[2],
                p[0] * v[0] + p[1] * v[1] + p[2] * v[2],
                p[0] * axis[0] + p[1] * axis[1] + p[2] * axis[2],
            ];
            assert!(
                (got.x - want[0]).abs() < TIGHT,
                "x: {} vs {}",
                got.x,
                want[0]
            );
            assert!(
                (got.y - want[1]).abs() < TIGHT,
                "y: {} vs {}",
                got.y,
                want[1]
            );
            assert!(
                (got.z - want[2]).abs() < TIGHT,
                "z: {} vs {}",
                got.z,
                want[2]
            );
        }
    }

    #[test]
    fn aabb_local_is_the_tight_box_of_transformed_geometry() {
        let f = OpFrame::new(None, Some(t45x()));
        let (s, c) = (f64::to_radians(45.0).sin(), f64::to_radians(45.0).cos());
        // A wedge in the XZ plane: (0,0,0), (10,0,0), (0,0,10).
        let facets = vec![tri([0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 0.0, 10.0])];
        let b = f.aabb_local_of(&facets).expect("a box");
        // Hand: u keeps x; v = (0, c, s) folds z into y; axis = (0, -s, c).
        // (0,0,0) -> (0,0,0); (10,0,0) -> (10,0,0); (0,0,10) -> (0, 10s, 10c).
        assert!((b.min.x - 0.0).abs() < TIGHT && (b.max.x - 10.0).abs() < TIGHT);
        assert!((b.max.y - 10.0 * s).abs() < TIGHT, "max.y: {}", b.max.y);
        assert!((b.max.z - 10.0 * c).abs() < TIGHT, "max.z: {}", b.max.z);
        // The L1 shift of the S box would read max.z = 10 - origin.z = 10:
        // the seam does not do that (the condition-2 law).
        assert!(
            (b.max.z - 10.0).abs() > 1.0,
            "the local box is transformed, not z-shifted"
        );
    }

    #[test]
    fn tool_axis_and_z_local_read_the_commanded_letters() {
        let f = OpFrame::new(
            None,
            Some(Tilt {
                basis_version: 1,
                a_deg: 30.0,
                b_deg: 0.0,
                origin: [5.0, -1.0, 2.0],
            }),
        );
        let axis = f.tool_axis();
        let want = [0.0, -0.5, 0.75_f64.sqrt()];
        for i in 0..3 {
            assert!((axis[i] - want[i]).abs() < TIGHT, "axis [{i}]: {}", axis[i]);
        }
        let o = Point3::new(5.0, -1.0, 2.0);
        assert!(f.z_local(o).abs() < TIGHT, "the origin's local height is 0");
        let q = Point3::new(
            5.0 + 3.0 * axis[0],
            -1.0 + 3.0 * axis[1],
            2.0 + 3.0 * axis[2],
        );
        assert!(
            (f.z_local(q) - 3.0).abs() < TIGHT,
            "3mm along the axis reads 3"
        );
    }

    #[test]
    fn letters_without_a_direction_refuse_by_name() {
        let f = OpFrame::new(
            None,
            Some(Tilt {
                basis_version: 1,
                a_deg: f64::NAN,
                b_deg: 0.0,
                origin: [0.0; 3],
            }),
        );
        let err = f.checked_frame().expect_err("non-finite letters refuse");
        assert!(
            err.to_string()
                .contains("the 3+2 index commands no direction"),
            "{err}"
        );
    }

    #[test]
    fn zero_tilt_is_the_exact_identity_basis() {
        let f = OpFrame::new(
            None,
            Some(Tilt {
                basis_version: 1,
                a_deg: 0.0,
                b_deg: 0.0,
                origin: [0.0; 3],
            }),
        );
        let frame = f.checked_frame().expect("finite letters").expect("a frame");
        assert_eq!(frame.u, [1.0, 0.0, 0.0]);
        assert_eq!(frame.v, [0.0, 1.0, 0.0]);
        assert_eq!(frame.axis, [0.0, 0.0, 1.0]);
        assert!(frame.is_upright());
    }
}
