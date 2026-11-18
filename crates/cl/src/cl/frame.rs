//! The tilted work frame: 3+2 indexed machining's coordinate system.
//!
//! Provenance: 3-axis machines machining on a tilted plane with the
//! rotarys indexed is standard shop practice (a "3+2" setup); running
//! an ordinary planar program in the plane is general machining
//! knowledge, machine-brand free. The frame's basis CHOICE rule below
//! is ours, stated once here so the generators, the verifier and the
//! preview all compute the same (u, v) from the same axis — never
//! private copies that disagree by a rotation.
//!
//! The contract: [`TiltedFrame`] is a rigid, right-handed, orthonormal
//! frame (`u × v = axis`). Everything a planar generator computes
//! "in XY at height z" happens in LOCAL coordinates here; the walk to
//! the machine's world is exactly one function, [`TiltedFrame::world_path`],
//! and every law that compares against world metal reads the same
//! frame. Rotation-invariance does the rest: a pocket in a tilted
//! plane is the flat pocket's path, rotated — no new geometry math.

use crate::geo2d::{GeoError, Loop, Point2, Region};
use crate::geometry::{Point3, Triangle};

use super::moves::Move;
use super::path::Toolpath;
use super::pose::{angles_to_axis, is_vertical, normalize_axis};

/// A rigid frame on the work: origin plus the right-handed orthonormal
/// basis `(u, v, axis)`. `axis` is the [`Move::axis`] the whole path
/// holds — the tool stands along it — and the plane spanned by `u` and
/// `v` is the "XY" the planar generators run in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TiltedFrame {
    pub origin: Point3,
    pub u: [f64; 3],
    pub v: [f64; 3],
    pub axis: [f64; 3],
}

/// Componentwise dot and cross of `[f64; 3]` vectors — the frame's
/// whole linear algebra, next to the frame it builds.
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

impl TiltedFrame {
    /// The frame holding `axis` (normalized here, [`normalize_axis`]'s
    /// refusal law) about `origin`. The in-plane basis follows ONE
    /// rule: take the world axis whose component in `axis` is smallest
    /// in magnitude — the most independent of the three — and
    /// Gram-Schmidt it against `axis`. The seed is then never closer
    /// than ~35° to perpendicular (the largest component of a unit
    /// vector is ≥ 1/√3, so the smallest is ≤ 1/√3), which puts the
    /// projection's length ≥ √(2/3) — the normalization cannot fail,
    /// and no axis direction near the seeds can tilt the frame
    /// discontinuously. The vertical axis takes the EXACT identity
    /// basis, bits included: a zero-tilt op must reproduce the
    /// prismatic path exactly, goldens and all.
    #[must_use]
    pub fn new(origin: Point3, axis: [f64; 3]) -> Option<Self> {
        Self::new_versioned(origin, axis, 1)
    }

    fn new_versioned(origin: Point3, axis: [f64; 3], basis_version: u8) -> Option<Self> {
        if basis_version > 1 {
            return None;
        }
        let a = normalize_axis(axis)?;
        if is_vertical(&a) {
            return Some(TiltedFrame {
                origin,
                u: [1.0, 0.0, 0.0],
                v: [0.0, 1.0, 0.0],
                axis: a,
            });
        }
        let abs = [a[0].abs(), a[1].abs(), a[2].abs()];
        // Ignore trigonometric float dust (such as sin(180 deg) ~ 1.22e-16)
        // when breaking ties for the smallest component, so an axis in the YZ
        // plane such as A0 B180 keeps plane X along machine +X.
        let eff = abs.map(|x| if basis_version == 1 && x < 1e-12 { 0.0 } else { x });
        let seed_idx = if eff[0] <= eff[1] && eff[0] <= eff[2] {
            0
        } else if eff[1] <= eff[2] {
            1
        } else {
            2
        };
        let mut seed = [0.0; 3];
        seed[seed_idx] = 1.0;
        let sd = dot(seed, a);
        let raw = [seed[0] - sd * a[0], seed[1] - sd * a[1], seed[2] - sd * a[2]];
        // Unreachable by the bound above; normalize_axis's refusal is
        // kept honest rather than unwrapped around it.
        let u = normalize_axis(raw)?;
        let v = cross(a, u);
        Some(TiltedFrame { origin, u, v, axis: a })
    }

    /// The frame the RS-274 letters command: `axis = Rz(0)·Ry(B)·Rx(A)·ẑ`
    /// — [`angles_to_axis`] under the pose convention, C left to the
    /// machine (a 3+2 index spins C into position; the cut plane only
    /// cares about A and B). None on a non-finite angle.
    #[must_use]
    pub fn from_angles(origin: Point3, a_deg: f64, b_deg: f64) -> Option<Self> {
        Self::from_angles_versioned(origin, a_deg, b_deg, 1)
    }

    /// Read the persisted basis version. Version 0 preserves the original
    /// smallest-component rule, including its trigonometric roundoff.
    #[must_use]
    pub fn from_angles_versioned(origin: Point3, a_deg: f64, b_deg: f64, basis_version: u8) -> Option<Self> {
        let axis = angles_to_axis([a_deg, b_deg, 0.0])?;
        Self::new_versioned(origin, axis, basis_version)
    }

    /// True when this frame IS the world's XY: the exact-vertical test
    /// the generators' fast paths branch on (same law as
    /// [`is_vertical`] — the frame adds no new notion of vertical).
    #[must_use]
    pub fn is_upright(&self) -> bool {
        is_vertical(&self.axis)
    }

    /// World → local.
    #[must_use]
    pub fn to_local(&self, p: Point3) -> Point3 {
        let d = [p.x - self.origin.x, p.y - self.origin.y, p.z - self.origin.z];
        Point3::new(dot(d, self.u), dot(d, self.v), dot(d, self.axis))
    }

    /// Local → world.
    #[must_use]
    pub fn to_world(&self, p: Point3) -> Point3 {
        Point3::new(
            self.origin.x + p.x * self.u[0] + p.y * self.v[0] + p.z * self.axis[0],
            self.origin.y + p.x * self.u[1] + p.y * self.v[1] + p.z * self.axis[1],
            self.origin.z + p.x * self.u[2] + p.y * self.v[2] + p.z * self.axis[2],
        )
    }

    /// A DIRECTION (no origin) world → local: facet normals and the
    /// like. The stored `Triangle::normal` field is zero from every
    /// importer and orientation comes from the recomputed
    /// [`Triangle::normal`], which is a cross of edges — equivariant
    /// under this proper rotation — so rotating vertices alone carries
    /// the facet's whole orientation law.
    #[must_use]
    pub fn dir_to_local(&self, n: Point3) -> Point3 {
        Point3::new(dot([n.x, n.y, n.z], self.u), dot([n.x, n.y, n.z], self.v), dot([n.x, n.y, n.z], self.axis))
    }

    /// The frame's own height coordinate of a world point: what the
    /// verifier's Z-plane laws compare against once the work is
    /// indexed off XY (a "clearance plane" is the plane `z_local =
    /// clearance` in this frame, not `z = clearance` in the world).
    #[must_use]
    pub fn z_local(&self, p: Point3) -> f64 {
        dot(
            [p.x - self.origin.x, p.y - self.origin.y, p.z - self.origin.z],
            self.axis,
        )
    }

    /// Material seen from the frame: the same facets, rotated into
    /// local coordinates (vertices only — see
    /// [`dir_to_local`](Self::dir_to_local)'s equivariance note). The
    /// upright case is the identity map with the bits intact.
    #[must_use]
    pub fn local_facets(&self, facets: &[Triangle]) -> Vec<Triangle> {
        facets
            .iter()
            .map(|t| Triangle::new(self.to_local(t.a), self.to_local(t.b), self.to_local(t.c)))
            .collect()
    }

    /// A local path in world coordinates, with the commanded pose on
    /// every move. Arcs: the upright frame maps a local circle to a
    /// world circle (one translation — the `ArcMove` survives verbatim,
    /// center and all); a TILTED frame's circle projects to an ellipse
    /// in the world XY the `G2/G3` contract cannot state, so its
    /// chords at `chord_tol` (the caller's [`crate::cl`] sagitta
    /// tolerance — the same number the verifier and the golden dump
    /// measure arcs by) replace it, motion-for-motion. Interpolated
    /// Z rides linearly between the endpoints (a helix in the plane
    /// posts as its chord polyline — an honest coarser truth, and the
    /// exact endpoints keep their bits). `cycles` pass through:
    /// drilling refuses tilt at the parameter door, so a tilted path
    /// with a cycle in it is a contract violation upstream, not a
    /// thing this function silently re-expresses. A stated spindle C
    /// (`Move::rotary`) rides every move it maps, chords included
    /// (#295): the part's turn is not the frame's to drop. No tilted kind
    /// is C-stamped today - the rotary kinds take no tilt
    /// (`OpParams::tilt`) - so this keeps the angle rather than losing it.
    #[must_use]
    pub fn world_path(&self, local: &Toolpath, chord_tol: f64) -> Toolpath {
        let stamp = if self.is_upright() { None } else { Some(self.axis) };
        let mut moves = Vec::with_capacity(local.moves.len());
        // Where each local move starts in the world list: a flattened
        // arc becomes several chords, so a probe touch's index (#108)
        // is re-pointed through this, never copied.
        let mut starts = Vec::with_capacity(local.moves.len());
        for m in &local.moves {
            starts.push(moves.len());
            match &m.arc {
                // Upright, vertical plane (M11.2 A): the identity
                // basis keeps the plane, the center rides the
                // translation in its own word pair.
                Some(arc) if self.is_upright() && !arc.plane.is_xy() => {
                    let (cu, cv) = arc.plane.center_uv(arc.center);
                    let c3 = self.to_world(arc.plane.point(cu, cv, 0.0));
                    moves.push(Move {
                        rotary: m.rotary,
                        kind: m.kind,
                        from: self.to_world(m.from),
                        to: self.to_world(m.to),
                        feed: m.feed,
                        arc: Some(super::ArcMove { center: arc.plane.words(c3), ..*arc }),
                        axis: stamp,
                    });
                }
                // Tilted, vertical plane: its own 3-D chords, mapped.
                Some(arc) if !arc.plane.is_xy() => {
                    for (a, b) in m.segments3(chord_tol) {
                        let (a, b) = (self.to_world(a), self.to_world(b));
                        if a == b {
                            continue; // a collapsed chord is no motion
                        }
                        moves.push(Move {
                            rotary: m.rotary,
                            kind: m.kind,
                            from: a,
                            to: b,
                            feed: m.feed,
                            arc: None,
                            axis: stamp,
                        });
                    }
                }
                // Upright: an arc is still an arc — translate and go.
                Some(arc) if self.is_upright() => {
                    moves.push(Move {
                        rotary: m.rotary,
                        kind: m.kind,
                        from: self.to_world(m.from),
                        to: self.to_world(m.to),
                        feed: m.feed,
                        arc: Some(*arc),
                        axis: stamp,
                    });
                }
                Some(_) => {
                    // Tilted: flatten. `chords` speaks XY at the
                    // LOCAL z's it is given, so rebuild the local
                    // points along the run (endpoints exact, Z
                    // linear) and map each through the frame.
                    let ch = m.chords(chord_tol);
                    let n = ch.len() as f64;
                    let mut pts = Vec::with_capacity(ch.len() + 1);
                    for (i, (a, b)) in ch.iter().enumerate() {
                        let za = if i == 0 {
                            m.from.z
                        } else {
                            let t = i as f64 / n;
                            m.from.z + (m.to.z - m.from.z) * t
                        };
                        pts.push(self.to_world(Point3::new(a.x, a.y, za)));
                        let last = i + 1 == ch.len();
                        let zb = if last {
                            m.to.z
                        } else {
                            let t = (i + 1) as f64 / n;
                            m.from.z + (m.to.z - m.from.z) * t
                        };
                        pts.push(self.to_world(Point3::new(b.x, b.y, zb)));
                        if last {
                            break;
                        }
                    }
                    // Consecutive chords share endpoints BY
                    // CONSTRUCTION (same (x, y) in, same arithmetic),
                    // and the windowed rebuild keeps the chain exact.
                    for w in pts.windows(2) {
                        let zero = w[0].x == w[1].x && w[0].y == w[1].y && w[0].z == w[1].z;
                        if zero {
                            continue; // a collapsed chord is no motion
                        }
                        moves.push(Move {
                            rotary: m.rotary,
                            kind: m.kind,
                            from: w[0],
                            to: w[1],
                            feed: m.feed,
                            arc: None,
                            axis: stamp,
                        });
                    }
                }
                None => {
                    moves.push(Move {
                        rotary: m.rotary,
                        kind: m.kind,
                        from: self.to_world(m.from),
                        to: self.to_world(m.to),
                        feed: m.feed,
                        arc: None,
                        axis: stamp,
                    });
                }
            }
        }
        // The control-compensated spans (M13.11) ride the same map
        // upright - their programmed moves are straight or XY arcs,
        // the translation case above. A tilted frame drops them and
        // says so: the centre moves then post as a centre path, a
        // correct program with the compensation taken in computer.
        let mut notes = local.notes.clone();
        let comps = if self.is_upright() {
            local
                .comps
                .iter()
                .map(|c| super::CompSpan {
                    programmed: c
                        .programmed
                        .iter()
                        .map(|m| Move {
                            rotary: m.rotary,
                            kind: m.kind,
                            from: self.to_world(m.from),
                            to: self.to_world(m.to),
                            feed: m.feed,
                            arc: m.arc.map(|arc| {
                                let c3 = self.to_world(arc.plane.point(arc.center.x, arc.center.y, 0.0));
                                super::ArcMove { center: arc.plane.words(c3), ..arc }
                            }),
                            axis: stamp,
                        })
                        .collect(),
                    ..c.clone()
                })
                .collect()
        } else {
            if !local.comps.is_empty() {
                notes.push(
                    "compensation: control compensation does not ride a tilted frame - this path posts its cutter-centre moves (in computer)"
                        .into(),
                );
            }
            Vec::new()
        };
        // Threading records (issue #34) are turning intent - a lathe
        // never runs a tilted work plane; a tilted frame drops them
        // and says so, and the passes post as themselves (G32 form).
        if !local.threads.is_empty() {
            notes.push("threading: a thread cycle does not ride a tilted frame - this path posts its explicit passes".into());
        }
        // Probe touches (#108) ride the map: a touch names a straight
        // feed, which maps to one world move, and its target maps with it.
        let probes = local
            .probes
            .iter()
            .map(|t| {
                // A touch naming no local move keeps an index naming
                // no world move: `is_sane` then refuses it at the post,
                // rather than the touch vanishing into a plain feed.
                let index = starts.get(t.index).copied().unwrap_or(usize::MAX);
                super::ProbeTouch { index, target: self.to_world(t.target), ..*t }
            })
            .collect();
        // Framed caps rotate as volumes; legacy vertical cones still
        // declare nothing in a tilted frame.
        let breaks = local.breaks.iter()
            .filter(|b| self.is_upright() || b.volume.is_some())
            .map(|b| {
                let mut b = super::EdgeBreak { edge: b.edge.iter().map(|p| self.to_world(*p)).collect(), ..b.clone() };
                if let Some(v) = &mut b.volume {
                    v.rotate(self);
                }
                b
            }).collect();
        Toolpath { moves, cycles: local.cycles.clone(), notes, comps, threads: Vec::new(), probes, breaks, plasma: local.plasma.clone() }
    }

    /// A local planar region (the cut's footprint at local height
    /// `z`) in world XY: what the preview and the XY-domain laws
    /// (fixture overlap, stock footprint) read. Points on the plane
    /// `z_local = z` project one-to-one into world XY (the frame is
    /// never edge-on: `axis` always has a positive Z, |axis.z| =
    /// cos of the tilt < 90° by the parameter refusal), so a loop
    /// survives as a loop.
    pub fn world_region(&self, local: &Region, z: f64) -> Result<Region, GeoError> {
        let mut loops = Vec::with_capacity(local.loops().len());
        for l in local.loops() {
            let mut pts = Vec::with_capacity(l.points().len());
            for p in l.points() {
                let w = self.to_world(Point3::new(p.x, p.y, z));
                pts.push(Point2::new(w.x, w.y));
            }
            loops.push(Loop::new(pts)?);
        }
        Region::new(loops)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::moves::MoveKind;
    use crate::cl::path::PathBuilder;
    use crate::cl::pose::axis_to_angles;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn the_vertical_axis_takes_the_exact_identity_basis() {
        let f = TiltedFrame::new(p(7.0, -3.0, 11.0), [0.0, 0.0, 1.0]).unwrap();
        assert_eq!(f.u, [1.0, 0.0, 0.0]);
        assert_eq!(f.v, [0.0, 1.0, 0.0]);
        assert!(f.is_upright());
        // At a zero origin the translation round trip keeps its bits;
        // away from it, subtraction and re-addition trade the last
        // bits like any two translations would — the law is the
        // micron, not the bit, once the origin is off-zero. (The
        // goldens' bit-exactness never rides this: a None tilt skips
        // the frame entirely upstream.)
        let g = TiltedFrame::new(Point3::ZERO, [0.0, 0.0, 1.0]).unwrap();
        let q = p(1.25, -6.5, 0.000_1);
        assert_eq!(g.to_world(g.to_local(q)), q);
        let back = f.to_world(f.to_local(q));
        let e = back - q;
        assert!(e.x.abs().max(e.y.abs().max(e.z.abs())) < 1e-9, "{back:?}");
    }

    #[test]
    fn from_angles_zero_is_the_identity_too() {
        let f = TiltedFrame::from_angles(Point3::ZERO, 0.0, 0.0).unwrap();
        assert_eq!(f.axis, [0.0, 0.0, 1.0]);
        assert!(f.is_upright());
    }

    #[test]
    fn the_basis_is_right_handed_and_orthonormal_everywhere() {
        // A sweep of poses: single-axis tips, combined tips, the
        // sign-flipped and near-degenerate ones (an axis along +X
        // makes the Z seed degenerate — the seed rule must shrug it
        // off — and along +Z is the identity case).
        let axes = [
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [std::f64::consts::FRAC_1_SQRT_2, 0.0, std::f64::consts::FRAC_1_SQRT_2],
            [0.0, -0.5, 0.866_025_403_784_438_7],
            [-0.3, -0.6, 0.741_619_848_724_46],
            [0.999_999_9, 0.001_414_213_5, 0.001_414_213_5],
        ];
        for a in axes {
            let f = TiltedFrame::new(p(1.0, 2.0, 3.0), a).expect("a unit axis frames");
            let u = f.u;
            let v = f.v;
            for (name, b) in [("u", u), ("v", v), ("axis", f.axis)] {
                let len = dot(b, b).sqrt();
                assert!((len - 1.0).abs() < 1e-12, "{name} not unit: {len}");
            }
            assert!(dot(u, v).abs() < 1e-12, "u·v for {a:?}");
            assert!(dot(u, f.axis).abs() < 1e-12, "u·axis for {a:?}");
            assert!(dot(v, f.axis).abs() < 1e-12, "v·axis for {a:?}");
            let c = cross(u, v);
            for (ci, ai) in c.iter().zip(f.axis.iter()) {
                assert!((ci - ai).abs() < 1e-12, "u×v != axis for {a:?}");
            }
        }
    }

    #[test]
    fn local_and_world_round_trip_within_a_micron() {
        // The canonical-letter route is what stage B commands, so
        // round the pose through its own angles as the UI would.
        for (a, b) in [(0.0, 0.0), (30.0, -45.0), (-90.0, 1e-9), (89.999, 60.0)] {
            let f = TiltedFrame::from_angles(p(2.0, 2.0, 2.0), a, b).unwrap();
            for q in [p(-9.0, 3.25, 0.001), p(0.0, 0.0, 0.0), p(123.5, -78.25, 41.0)] {
                let back = f.to_world(f.to_local(q));
                let e = back - q;
                assert!(
                    e.x.abs().max(e.y.abs().max(e.z.abs())) < 1e-9,
                    "round trip at {a},{b}: {q:?} -> {back:?}"
                );
            }
        }
    }

    #[test]
    fn the_frame_reads_its_own_angles_back() {
        // Pose convention is one law: the frame built FROM letters
        // answers with those same letters (canonical form, C = 0).
        let f = TiltedFrame::from_angles(Point3::ZERO, 25.0, -40.0).unwrap();
        let [a, b, c] = axis_to_angles(f.axis);
        assert!((a - 25.0).abs() < 1e-9 && (b + 40.0).abs() < 1e-9 && c == 0.0);
    }

    #[test]
    fn a_tilted_circle_walks_its_chords_on_the_plane() {
        // A half ring at local z = 0 in a 30°-tilted frame: after
        // world_path, every vertex must sit on the plane (z_local ==
        // 0) and at the circle's radius FROM the arc's centre — the
        // chord flattening may coarsen the shape, never lift it off
        // the plane.
        let f = TiltedFrame::from_angles(p(1.0, 0.0, 5.0), 30.0, 0.0).unwrap();
        let r = 8.0;
        let mut b = PathBuilder::new(p(r, 0.0, 0.0));
        b.push_arc(
            MoveKind::Feed,
            p(-r, 0.0, 0.0),
            Some(500.0),
            Point2::new(0.0, 0.0),
            true,
        );
        let local = b.finish();
        assert_eq!(local.len(), 1, "one half-circle move");
        let world = f.world_path(&local, 0.001);
        assert!(world.len() > 2, "the arc became chords: {}/1", world.len());
        assert!(world.is_chained(), "chord split must keep the chain");
        assert!(!world.is_prismatic(), "a tilted path states its pose");
        for m in &world.moves {
            assert_eq!(m.arc, None, "a tilted arc is chords");
            assert_eq!(m.axis, Some(f.axis));
            for pt in [m.from, m.to] {
                assert!(f.z_local(pt).abs() < 1e-9, "chord vertex off plane");
                let l = f.to_local(pt);
                let d = (l.x * l.x + l.y * l.y).sqrt();
                assert!((d - r).abs() < 0.002, "chord vertex off radius");
            }
        }
    }

    #[test]
    fn the_upright_world_path_is_the_prismatic_path_translated() {
        // Exact-structure law, arcs intact, no pose stamped anywhere:
        // an indexed op at zero tilt must be indistinguishable from
        // the flat op moved — the goldens' sibling law.
        let f = TiltedFrame::new(p(10.0, 0.0, 1.0), [0.0, 0.0, 1.0]).unwrap();
        let mut b = PathBuilder::new(Point3::ZERO);
        b.push(MoveKind::Rapid, p(5.0, 0.0, 0.0), None);
        b.push_arc(
            MoveKind::Feed,
            p(5.0, 2.0, -1.0),
            Some(800.0),
            Point2::new(5.0, 1.0),
            true,
        );
        b.push(MoveKind::Plunge, p(5.0, 2.0, -3.0), Some(200.0));
        let local = b.finish();
        let world = f.world_path(&local, 0.001);
        assert_eq!(world.moves.len(), local.moves.len());
        assert!(world.is_prismatic());
        for (l, w) in local.moves.iter().zip(&world.moves) {
            assert_eq!(w.kind, l.kind);
            assert_eq!(w.feed, l.feed);
            assert_eq!(w.axis, None);
            assert_eq!(w.arc, l.arc, "an upright arc keeps its circle");
            assert_eq!(w.from, f.to_world(l.from));
            assert_eq!(w.to, f.to_world(l.to));
        }
    }

    #[test]
    fn degenerate_axes_are_refused_not_framed() {
        assert_eq!(TiltedFrame::new(Point3::ZERO, [0.0, 0.0, 0.0]), None);
        assert_eq!(
            TiltedFrame::new(Point3::ZERO, [f64::NAN, 0.0, 1.0]),
            None
        );
        assert_eq!(TiltedFrame::from_angles(Point3::ZERO, f64::INFINITY, 0.0), None);
        // The tool end-over-end is a frame (a 180° B), not a refusal:
        // reachable or not is the machine checker's law, not this one.
        let f = TiltedFrame::new(Point3::ZERO, [0.0, 0.0, -1.0]);
        assert!(f.is_some());
    }

    #[test]
    fn a_tilted_world_region_projects_to_world_xy() {
        // A 20 mm square in the frame at local z = 0, -45° about A:
        // its world corners are the square's corners rotated and
        // shifted; area in the plane is preserved (a rotation is an
        // isometry), and the XY shadow is the square squeezed by
        // cos(45°) along one axis — the preview's honest shape.
        let f = TiltedFrame::from_angles(Point3::ZERO, 45.0, 0.0).unwrap();
        let sq = Region::new(vec![Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(20.0, 0.0),
            Point2::new(20.0, 20.0),
            Point2::new(0.0, 20.0),
        ])
        .unwrap()])
        .unwrap();
        let w = f.world_region(&sq, 0.0).unwrap();
        assert_eq!(w.loops().len(), 1, "one square, one loop");
        // Match by position, not index: Region::new fuses, and where
        // a fused ring starts is the fuser's business, not the law's.
        let truth: [Point3; 4] = [
            f.to_world(Point3::new(0.0, 0.0, 0.0)),
            f.to_world(Point3::new(20.0, 0.0, 0.0)),
            f.to_world(Point3::new(20.0, 20.0, 0.0)),
            f.to_world(Point3::new(0.0, 20.0, 0.0)),
        ];
        for q in w.loops()[0].points() {
            // Region::new fuses through the micron integer lattice
            // (geo2d SCALE), so the shadows are that close to exact,
            // not bit close — same law every other sketched region
            // pays.
            let hit = truth
                .iter()
                .find(|t| (t.x - q.x).abs() < 1e-5 && (t.y - q.y).abs() < 1e-5)
                .expect("shadow corner off the frame's square");
            assert!(f.z_local(*hit).abs() < 1e-9, "corner off the plane");
        }
        // Edges keep their 20 mm length IN THE PLANE (a rotation is
        // an isometry) even as the XY shadow squeezes by cos(45°).
        for pair in truth.windows(2) {
            let d = pair[1] - pair[0];
            let len3 = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt();
            assert!((len3 - 20.0).abs() < 1e-9, "edge stretched: {len3}");
        }
    }
}
