//! The cutter as the solid law reads it: the tool assembly as a stack of
//! convex solids of revolution (flat band, point cone, toroidal corner,
//! ball) and the tool frame a move is measured in. The tip shapes are
//! the standard machine-tool handbook side views, the same ones
//! [`Tool::profile`] draws; the frame is plain linear algebra. Our own
//! design; no vendor source.

use camcraft_tools::holder_profile::HolderStack;
use camcraft_tools::undercut::{Head, Undercut};

use super::body_bands::{BodyBands, NeckBands};
use crate::geometry::{Point3, Triangle};
use crate::tools::{Tool, ToolGeom, ToolKind};

/// The tool assembly: the cutting atom (radius, corner, point cone)
/// over `flute_len`, then the shank and holder bands - the same stack
/// the reach laws collide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cutter {
    pub radius: f64,
    /// 0 square end, `radius` ball, between bull nose.
    pub corner_r: f64,
    /// Included point angle in degrees (drill, taper); 0 for none.
    pub tip_angle: f64,
    pub flute_len: f64,
    pub shank_radius: f64,
    pub shank_len: f64,
    pub holder_radius: f64,
    pub holder_len: f64,
    /// A modelled tool body's bands (#835), standing in for the holder
    /// band: the shape the fixture laws sweep and the simulation draws.
    /// `None` reads the holder cylinder, as always.
    pub body: Option<BodyBands>,
    /// A necked tool's drawn shank bands (#852), standing in for the
    /// shank band where they are narrower. `None` reads the stated shank
    /// cylinder, as always.
    pub neck: Option<NeckBands>,
    /// An undercut cutter's head (#1444): a dovetail's frustum or a
    /// lollipop's ball stands in for the square flute atom, and the
    /// undercut law reads the head and neck from it. `None` is every
    /// other tool, as always.
    pub undercut: Option<Undercut>,
}

impl Cutter {
    /// The square-ended stack of a [`ToolGeom`]: never smaller than the
    /// real cutter at the same tip (a ball, bull or point sits inside
    /// the flat cylinder), so a verdict read with it can only over-report.
    pub fn flat(g: &ToolGeom) -> Self {
        Self {
            radius: g.radius,
            corner_r: 0.0,
            tip_angle: 0.0,
            flute_len: g.flute_len,
            shank_radius: g.shank_radius,
            shank_len: g.shank_len,
            holder_radius: g.holder_radius,
            holder_len: g.holder_len,
            body: None,
            neck: None,
            undercut: None,
        }
    }

    /// The same cutter with its holder read as the body's band stack
    /// `stack` (#835) - `tool_body::stack` of the tool `g` describes.
    /// A stack that is the plain holder cylinder, or does not fit `g`
    /// ([`BodyBands::of`]), leaves the cylinder: it can only over-report.
    #[must_use]
    /// Its drawn neck (#852) rides the same way ([`NeckBands::of`]).
    pub fn with_body(self, g: &ToolGeom, stack: &HolderStack) -> Self {
        Self { body: BodyBands::of(g, stack), neck: NeckBands::of(g, stack), ..self }
    }

    /// The shank as `(bottom above the tip, radius, length)` bands,
    /// bottom-up: the drawn neck's bands, or the one stated cylinder.
    #[must_use]
    pub fn shank_bands(&self) -> Vec<(f64, f64, f64)> {
        match &self.neck {
            Some(n) => n.bands().iter().map(|h| (h.above, h.r, h.len)).collect(),
            None => vec![(self.flute_len, self.shank_radius, self.shank_len)],
        }
    }

    /// What stands above the shank as `(bottom above the tip, radius,
    /// length)` bands, bottom-up: the body's bands, or the one holder
    /// cylinder.
    #[must_use]
    pub fn holder_bands(&self) -> Vec<(f64, f64, f64)> {
        match &self.body {
            Some(b) => b.bands().iter().map(|h| (h.above, h.r, h.len)).collect(),
            None => vec![(self.flute_len + self.shank_len, self.holder_radius, self.holder_len)],
        }
    }

    /// The tool's own tip, by kind - exactly the side view
    /// [`Tool::profile`] draws: ball, bull corner, drill/taper point.
    pub fn of(t: &Tool) -> Self {
        let mut c = Self::flat(&t.geom());
        match t.kind {
            ToolKind::BallNose => c.corner_r = c.radius,
            ToolKind::BullNose => c.corner_r = t.corner_r,
            ToolKind::Drill | ToolKind::Taper => c.tip_angle = t.tip_angle,
            _ => {}
        }
        c.undercut = Undercut::of(t);
        c
    }

    /// The stack the model law (b2) judges with (M13.7): `real` when it
    /// is the tip of exactly this `ToolGeom` - its square stack equals
    /// [`Self::flat`]`(g)`, so only the tip differs and the real tip
    /// sits inside the square one - else the square stack. A tip that
    /// disagrees with the geometry the other laws read is never
    /// trusted to clear a move: the fallback can only over-report.
    /// Its body bands (#835) ride along only while they still fit `g`
    /// ([`BodyBands::of`]'s law); else the holder cylinder stands in.
    #[must_use]
    pub fn judged(g: &ToolGeom, real: Option<Self>) -> Self {
        let flat = Self::flat(g);
        match real {
            Some(c) if Self { corner_r: 0.0, tip_angle: 0.0, body: None, neck: None, undercut: None, ..c } == flat => {
                // #1444: a head rides only while it is this geometry's.
                let fits = |u: &Undercut| {
                    (u.r - g.radius).abs() <= 1e-9 && (u.height - g.flute_len).abs() <= 1e-9 && (u.neck_r - g.shank_radius).abs() <= 1e-9
                };
                let c = Self { undercut: c.undercut.filter(fits), ..c };
                let fits = c.body.is_some_and(|b| {
                    let top = g.flute_len + g.shank_len + g.holder_len;
                    let stack = HolderStack { bands: b.bands().to_vec(), top, neck: Vec::new() };
                    BodyBands::of(g, &stack).is_some()
                });
                // #852: a neck rides only while it still fits `g`.
                let neck = c.neck.filter(|n| n.fits(g));
                Self { body: if fits { c.body } else { None }, neck, ..c }
            }
            _ => flat,
        }
    }

    /// Height above the tip where the cutting profile first reaches
    /// its full radius: the tip atom's top (ball/bull corner, point
    /// cone), 0 for a square end or a stack that describes no tool.
    #[must_use]
    pub fn tip_height(&self) -> f64 {
        self.atoms().first().filter(|a| !a.is_flat()).map_or(0.0, |a| a.z1)
    }

    /// Height above the tip at which the cutting surface stands at
    /// radial distance `rho` from the axis (#106): 0 inside a square
    /// end or a bull's flat, the corner arc's rise outside it (a ball
    /// is a bull with no flat), `rho / tan(half angle)` up a point
    /// cone. A `rho` past the tip atom, or a stack that describes no
    /// tool, reads 0 - the tip plane, which can only over-report.
    #[must_use]
    pub fn lift_at(&self, rho: f64) -> f64 {
        if !(rho.is_finite() && rho > 0.0) {
            return 0.0;
        }
        let Some(a) = self.atoms().into_iter().next() else { return 0.0 };
        let s = match a.prof {
            Prof::Flat(_) => 0.0,
            Prof::Bull { flat, cr } => {
                let e = rho - flat;
                if e <= 0.0 { 0.0 } else if e >= cr { a.z1 } else { cr - (cr * cr - e * e).max(0.0).sqrt() }
            }
            Prof::Cone { k, .. } => rho / k,
            // A dovetail's bottom is flat; a ball rises as a sphere.
            Prof::Dove { .. } => 0.0,
            Prof::Ball(r) => if rho >= r { r } else { r - (r * r - rho * rho).max(0.0).sqrt() },
        };
        if s.is_finite() { s.clamp(0.0, a.z1.max(0.0)) } else { 0.0 }
    }

    /// The atoms bottom-up, heights above the tip. Empty when the
    /// numbers describe no tool (the caller fails closed).
    pub(super) fn atoms(&self) -> Vec<Atom> {
        let ok = |v: f64| v.is_finite() && v >= 0.0;
        let r = self.radius;
        if !(r.is_finite() && r > 0.0) || ![self.flute_len, self.shank_len, self.holder_len].into_iter().all(ok) {
            return Vec::new();
        }
        let fl = self.flute_len;
        let mut v = Vec::with_capacity(4);
        let mut z = 0.0;
        let tan_half = (self.tip_angle.to_radians() * 0.5).tan();
        let cr = if self.corner_r.is_finite() { self.corner_r.clamp(0.0, r) } else { 0.0 };
        // #1444: an undercut head is its own atom. The disc is the
        // square flute atom below; the frustum and the ball replace it
        // (the ball whole, over the neck's start: the atoms are a
        // union, and the neck leaves the ball inside it).
        let head = self.undercut.and_then(|u| match u.head {
            Head::Disc => None,
            Head::Dove { cot } => Some(Atom { z0: 0.0, z1: fl, prof: Prof::Dove { r, k: cot }, cap: true }),
            Head::Ball => Some(Atom { z0: 0.0, z1: 2.0 * r, prof: Prof::Ball(r), cap: false }),
        });
        if let Some(a) = head {
            if a.z1 > 0.0 {
                v.push(a);
            }
            z = fl;
        } else if self.tip_angle > 0.0 && tan_half.is_finite() && tan_half > 0.0 {
            let top = (r / tan_half).min(fl);
            if top > 0.0 {
                v.push(Atom { z0: 0.0, z1: top, prof: Prof::Cone { k: tan_half, r }, cap: false });
                z = top;
            }
        } else if cr > 0.0 {
            let top = cr.min(fl);
            if top > 0.0 {
                v.push(Atom { z0: 0.0, z1: top, prof: Prof::Bull { flat: r - cr, cr }, cap: false });
                z = top;
            }
        }
        if fl > z {
            // #1444: a disc head's top face cuts the roof it stands under.
            v.push(Atom { z0: z, z1: fl, prof: Prof::Flat(r), cap: self.undercut.is_some() });
        }
        let mut band = |z0: f64, len: f64, br: f64| {
            if len > 0.0 && br.is_finite() && br > 0.0 {
                v.push(Atom { z0, z1: z0 + len, prof: Prof::Flat(br), cap: false });
            }
        };
        for (above, r, len) in self.shank_bands() {
            band(above, len, r);
        }
        match &self.body {
            Some(b) => {
                for h in b.bands() {
                    band(h.above, h.len, h.r);
                }
            }
            None => band(fl + self.shank_len, self.holder_len, self.holder_radius),
        }
        v
    }
}

/// An atom's radius profile above its own bottom `s = z - z0`.
#[derive(Clone, Copy, Debug)]
pub(super) enum Prof {
    Flat(f64),
    /// Toroidal corner: rho = flat + sqrt(cr^2 - (cr - s)^2); a ball is
    /// `flat = 0`.
    Bull { flat: f64, cr: f64 },
    /// Point cone, capped at the flute radius.
    Cone { k: f64, r: f64 },
    /// Dovetail head (#1444): rho = r - k s, widest at its bottom.
    Dove { r: f64, k: f64 },
    /// Lollipop head (#1444): the whole ball of radius r over [0, 2r].
    Ball(f64),
}

/// One convex solid of revolution about the tool axis over local
/// heights `[z0, z1]`; its profile is concave in z. It is also
/// non-decreasing, but for the undercut heads (#1444): a dovetail falls
/// and a ball rises then falls, so [`Atom::rho_peak`] reads the largest
/// radius of a span, never the span's top.
#[derive(Clone, Copy, Debug)]
pub(super) struct Atom {
    pub z0: f64,
    pub z1: f64,
    pub prof: Prof,
    /// The atom's top is a cutting face that may stand ON the part - an
    /// undercut head under its roof (#1444): the solid law grants it the
    /// allowance the tip gets, so contact is not a gouge. False for
    /// every other atom, whose top is read to its full height.
    pub cap: bool,
}

impl Atom {
    pub(super) fn rho(&self, z: f64) -> f64 {
        let s = (z - self.z0).max(0.0);
        match self.prof {
            Prof::Flat(r) => r,
            Prof::Bull { flat, cr } => {
                let t = (cr - s).clamp(0.0, cr);
                flat + (cr * cr - t * t).max(0.0).sqrt()
            }
            Prof::Cone { k, r } => (k * s).min(r),
            Prof::Dove { r, k } => (r - k * s).max(0.0),
            Prof::Ball(r) => (2.0 * r * s - s * s).max(0.0).sqrt(),
        }
    }

    /// The largest radius over heights `[lo, hi]`: the top's for the
    /// non-decreasing profiles, the bottom's for a dovetail, the
    /// equator's for a ball whose span holds it.
    pub(super) fn rho_peak(&self, lo: f64, hi: f64) -> f64 {
        match self.prof {
            Prof::Flat(_) | Prof::Bull { .. } | Prof::Cone { .. } => self.rho(hi),
            Prof::Dove { .. } => self.rho(lo),
            Prof::Ball(r) => {
                let eq = self.z0 + r;
                if lo <= eq && eq <= hi { r } else { self.rho(lo).max(self.rho(hi)) }
            }
        }
    }

    pub(super) fn rho_max(&self) -> f64 {
        match self.prof {
            Prof::Flat(r) | Prof::Cone { r, .. } | Prof::Dove { r, .. } | Prof::Ball(r) => r,
            Prof::Bull { flat, cr } => flat + cr,
        }
    }

    pub(super) fn is_flat(&self) -> bool {
        matches!(self.prof, Prof::Flat(_))
    }
}

/// The tool frame of a tip and axis: local coordinates of world points
/// (tip at the origin, tool axis +z).
#[derive(Clone, Copy)]
pub(super) struct Frame {
    o: Point3,
    u: [f64; 3],
    v: [f64; 3],
    a: [f64; 3],
}

impl Frame {
    pub(super) fn new(o: Point3, axis: Option<[f64; 3]>) -> Self {
        let Some(a) = axis else {
            return Self { o, u: [1.0, 0.0, 0.0], v: [0.0, 1.0, 0.0], a: [0.0, 0.0, 1.0] };
        };
        let e = if a[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let u = norm(cross(a, e));
        let v = cross(a, u);
        Self { o, u, v, a }
    }

    pub(super) fn local(&self, p: Point3) -> [f64; 3] {
        let d = [p.x - self.o.x, p.y - self.o.y, p.z - self.o.z];
        [dot(d, self.u), dot(d, self.v), dot(d, self.a)]
    }

    pub(super) fn tri(&self, t: &Triangle) -> [[f64; 3]; 3] {
        [self.local(t.a), self.local(t.b), self.local(t.c)]
    }

    pub(super) fn seg(&self, a: Point3, b: Point3) -> [[f64; 3]; 2] {
        [self.local(a), self.local(b)]
    }

    /// The world point `h` up the axis from the tip.
    pub(super) fn up(&self, h: f64) -> Point3 {
        Point3::new(self.o.x + self.a[0] * h, self.o.y + self.a[1] * h, self.o.z + self.a[2] * h)
    }

    /// The world displacement of local displacement `d`.
    pub(super) fn end(&self, d: [f64; 3]) -> Point3 {
        let c = |k: usize| self.u[k] * d[0] + self.v[k] * d[1] + self.a[k] * d[2];
        Point3::new(c(0), c(1), c(2))
    }

    pub(super) fn is_upright(&self) -> bool {
        self.a == [0.0, 0.0, 1.0]
    }

    /// The local XY of the line through `at` along `dir` when it runs
    /// along this frame's axis (within 1e-9 of parallel, either sense);
    /// `None` for any other line, or one that is not finite (#1205).
    pub(super) fn axis_xy(&self, at: Point3, dir: [f64; 3]) -> Option<[f64; 2]> {
        let len = dot(dir, dir).sqrt();
        if !(len.is_finite() && len > 0.0 && at.is_finite()) {
            return None;
        }
        let c = dot(dir, self.a) / len;
        let p = self.local(at);
        (c.abs() >= 1.0 - 1e-9 && p[0].is_finite() && p[1].is_finite()).then_some([p[0], p[1]])
    }
}

fn cross(p: [f64; 3], q: [f64; 3]) -> [f64; 3] {
    [p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0]]
}

fn dot(p: [f64; 3], q: [f64; 3]) -> f64 {
    p[0] * q[0] + p[1] * q[1] + p[2] * q[2]
}

fn norm(p: [f64; 3]) -> [f64; 3] {
    let l = dot(p, p).sqrt();
    if l > 0.0 { [p[0] / l, p[1] / l, p[2] / l] } else { [1.0, 0.0, 0.0] }
}
