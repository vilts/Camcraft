//! Metered STEP surfaces and edge curves, so a subdivision whose
//! tessellation never converges ends in a refusal by name instead of a
//! hang (#1536, #1545).
//!
//! Provenance: our own design (an evaluation budget on a parametric
//! surface); the subdivision it meters is truck's published
//! `parameter_division` (ricosjp, Apache-2.0) for most surface kinds, and
//! `step_division` for B-spline and NURBS faces (#1602). Nothing from any
//! CAM vendor.
//!
//! Why it is needed: truck's surface subdivision splits every parameter
//! cell whose midpoint stands off the bilinear patch of its corners by more
//! than the tolerance, and recurses until no cell splits. A B-spline that is
//! not continuous - an interior knot repeated past its degree - keeps one
//! cell "far" at every depth, so the recursion never ends (it outlives the
//! float resolution: a split at an unchanged midpoint still grows the
//! list). So does a range that runs far off the knot domain; the one known
//! source of that, a boundary point on a collapsed patch side, is settled
//! in `step_pole` (#1548) before it reaches the subdivision. It runs inside a rayon task we cannot
//! interrupt, and no facet is ever produced, so the facet budget never
//! fires.
//!
//! How it is bounded: each face's surface is wrapped in [`Metered`], which
//! counts every point truck evaluates on it during one tessellation pass.
//! B-spline and NURBS faces are subdivided by `step_division` called on the
//! wrapper, so every probe is counted (a division still splitting at its
//! sweep cap spends the whole budget at once); other surface kinds keep
//! their specialised division. The wrapper also stretches a B-spline or
//! NURBS face's parameters (`step_stretch`), and its parameter searches
//! settle poles and retry misses (`step_nearest`, #1602). Past the cap the wrapper answers every
//! evaluation with one fixed point: no cell is then "far", the subdivision
//! ends within one more sweep, and the importer refuses the model naming
//! the face - unless the trip came on a finer pass over a face proven
//! continuous, which keeps the coarser mesh (#1568). No thread is spawned
//! and none is left behind.

use std::ops::Bound;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use truck_stepio::r#in::alias::{
    ParameterDivision2D, ParametricSurface, ParametricSurface3D, ParameterRange, Point3, Surface, Vector3,
};
use truck_topology::compress::{CompressedFace, CompressedShell};

pub(super) use super::step_meter::Meter;
use super::step_meter::Pass;
use super::step_nearest::Fault;
use super::step_stretch::Stretch;
use super::ImportError;

/// Surface evaluations one face may spend in one tessellation pass.
///
/// Measured (#1568; M5's 75-file corpus of testdata and users' models,
/// release): a healthy face needs far more than its facet count suggests -
/// a trimmed NURBS face is divided over its whole untrimmed range. Peaks on
/// one face in one pass: 32050.STEP 23.1 M at the default facet budget,
/// socket-head screws 16-19 M, and 10208.STEP's face #1073 28.07 M at the
/// 60 M budget (`step_budget::CEILING`), where it converges - the old 25 M
/// cap refused it. 150 M is 5.3x that heaviest healthy face, and ends a
/// runaway face in 10-15 s (the #1536 fixture 10 s, sc_logo.step's face
/// #836 15 s). It is fixed, not scaled with the facet budget: the budget
/// does not bound one face's evaluations, a low budget must not lower the
/// cap, and a broken face's time must not grow with a user setting.
/// Headroom is not the only line: a REFINEMENT pass that trips on a face
/// proven continuous keeps the coarser pass's mesh (`step_continuity`), so
/// such a face is never refused once a pass has met.
pub const FACE_EVAL_CAP: u64 = 150_000_000;

/// A STEP surface whose evaluations are counted against a [`Meter`].
#[derive(Clone, Debug)]
pub(super) struct Metered {
    pub(super) inner: Surface,
    pub(super) meter: Arc<Meter>,
    /// The mesher sees `inner`'s parameters multiplied by this (#1602,
    /// `step_stretch`); every evaluation divides it back out.
    pub(super) stretch: Stretch,
}

impl Metered {
    #[cfg(test)]
    pub(super) fn new(inner: Surface, cap: u64) -> Self {
        Self::in_pass(inner, cap, Arc::default())
    }
    /// A face whose meter shares `pass` with its shell (#1599).
    pub(super) fn in_pass(inner: Surface, cap: u64, pass: Arc<Pass>) -> Self {
        let stretch = Stretch::of(&inner);
        Self { inner, meter: Arc::new(Meter::in_pass(cap, pass)), stretch }
    }
    /// The surface's own parameters for a point of the mesher's plane.
    pub(super) fn own(&self, u: f64, v: f64) -> (f64, f64) {
        self.stretch.inner((u, v))
    }
}

/// The surface kind, for a refusal a user can act on.
pub(super) fn kind(s: &Surface) -> &'static str {
    match s {
        Surface::ElementarySurface(_) => "elementary surface",
        Surface::SweptCurve(_) => "swept surface",
        Surface::BSplineSurface(_) => "B-spline surface",
        Surface::NurbsSurface(_) => "NURBS surface",
    }
}

impl ParametricSurface for Metered {
    type Point = Point3;
    type Vector = Vector3;
    fn subs(&self, u: f64, v: f64) -> Point3 {
        // Past the cap every point is the same point: the subdivision then
        // sees no cell off its patch and stops. The pass is refused after.
        if self.meter.spend() {
            let (u, v) = self.own(u, v);
            self.inner.subs(u, v)
        } else {
            Point3::new(0.0, 0.0, 0.0)
        }
    }
    // A stretched parameter moves the point 1/stretch as fast (chain rule).
    fn uder(&self, u: f64, v: f64) -> Vector3 {
        let (s, t) = self.own(u, v);
        self.inner.uder(s, t) / self.stretch.u
    }
    fn vder(&self, u: f64, v: f64) -> Vector3 {
        let (s, t) = self.own(u, v);
        self.inner.vder(s, t) / self.stretch.v
    }
    fn uuder(&self, u: f64, v: f64) -> Vector3 {
        let (s, t) = self.own(u, v);
        self.inner.uuder(s, t) / (self.stretch.u * self.stretch.u)
    }
    fn uvder(&self, u: f64, v: f64) -> Vector3 {
        let (s, t) = self.own(u, v);
        self.inner.uvder(s, t) / (self.stretch.u * self.stretch.v)
    }
    fn vvder(&self, u: f64, v: f64) -> Vector3 {
        let (s, t) = self.own(u, v);
        self.inner.vvder(s, t) / (self.stretch.v * self.stretch.v)
    }
    fn parameter_range(&self) -> (ParameterRange, ParameterRange) {
        let (ur, vr) = self.inner.parameter_range();
        (scaled(ur, self.stretch.u), scaled(vr, self.stretch.v))
    }
    fn u_period(&self) -> Option<f64> {
        self.inner.u_period().map(|p| p * self.stretch.u)
    }
    fn v_period(&self) -> Option<f64> {
        self.inner.v_period().map(|p| p * self.stretch.v)
    }
}

/// A parameter range multiplied by a positive `k`.
fn scaled((lo, hi): ParameterRange, k: f64) -> ParameterRange {
    let by = |b: Bound<f64>| match b {
        Bound::Included(x) => Bound::Included(x * k),
        Bound::Excluded(x) => Bound::Excluded(x * k),
        Bound::Unbounded => Bound::Unbounded,
    };
    (by(lo), by(hi))
}

impl ParametricSurface3D for Metered {
    fn normal(&self, u: f64, v: f64) -> Vector3 {
        // A positive stretch keeps the normal's direction.
        let (u, v) = self.own(u, v);
        self.inner.normal(u, v)
    }
}

impl ParameterDivision2D for Metered {
    fn parameter_division(&self, range: ((f64, f64), (f64, f64)), tol: f64) -> (Vec<f64>, Vec<f64>) {
        match &self.inner {
            // B-spline and NURBS faces divide per direction (#1602,
            // `step_division`); running it on the wrapper routes every
            // probe through the meter.
            Surface::BSplineSurface(_) | Surface::NurbsSurface(_) => {
                let (ur, vr) = self.parameter_range();
                if !within(range.0, ur) || !within(range.1, vr) {
                    self.meter.outside.store(true, Ordering::Relaxed);
                }
                let division = super::step_division::parameter_division(self, range, tol);
                // A division still splitting at its sweep cap (a jump) is
                // refused like one that ran out of evaluations.
                if division.is_none() {
                    self.meter.exhaust();
                }
                // A tripped face is refused after the pass; handing truck
                // the grid it grew (millions of points down the runaway
                // seam) would only make it triangulate them first.
                match division {
                    Some(division) if !self.meter.refused() => division,
                    _ => {
                        let ((u0, u1), (v0, v1)) = range;
                        (vec![u0, u1], vec![v0, v1])
                    }
                }
            }
            Surface::SweptCurve(s) => super::step_division::swept_division(s, &self.meter, range, tol),
            // Other kinds are never stretched (`Stretch::of`).
            other => other.parameter_division(range, tol),
        }
    }
}

/// `asked` lies inside `own` (a surface's parameter range), give or take
/// a millionth of its span for a trimming curve's round-off.
fn within((lo, hi): (f64, f64), own: ParameterRange) -> bool {
    let slack = |a: f64, b: f64| 1e-6 * (b - a).abs().max(1.0);
    let (lo_ok, hi_ok) = match own {
        (Bound::Included(a) | Bound::Excluded(a), Bound::Included(b) | Bound::Excluded(b)) => {
            (lo >= a - slack(a, b), hi <= b + slack(a, b))
        }
        (Bound::Included(a) | Bound::Excluded(a), Bound::Unbounded) => (lo >= a - slack(a, a), true),
        (Bound::Unbounded, Bound::Included(b) | Bound::Excluded(b)) => (true, hi <= b + slack(b, b)),
        (Bound::Unbounded, Bound::Unbounded) => (true, true),
    };
    lo.is_finite() && hi.is_finite() && lo_ok && hi_ok
}

type StepShell<S> = CompressedShell<Point3, truck_stepio::r#in::alias::Curve3D, S>;

/// A shell with every face metered, and the meters in face order.
pub(super) struct Guarded {
    pub shell: CompressedShell<Point3, super::step_division::CurveGuard, Metered>,
    meters: Vec<Arc<Meter>>,
    kinds: Vec<&'static str>,
    continuous: Vec<bool>,
    /// The budget every face and edge of a pass shares (#1599).
    pass: Arc<Pass>,
}

/// A face that ran past the cap in a pass, or whose boundary search was
/// refused (`Guarded::fault` then says why).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Trip {
    /// The face's index in the shell (0-based).
    pub index: usize,
    pub kind: &'static str,
    /// Its surface is proven continuous (`step_continuity`): the
    /// subdivision would have ended, it was only expensive.
    pub continuous: bool,
}

impl Guarded {
    pub(super) fn new(shell: StepShell<Surface>, cap: u64) -> Self {
        let mut meters = Vec::with_capacity(shell.faces.len());
        let mut kinds = Vec::with_capacity(shell.faces.len());
        let mut continuous = Vec::with_capacity(shell.faces.len());
        let pass = Arc::<Pass>::default();
        let faces = shell
            .faces
            .into_iter()
            .map(|f| {
                kinds.push(kind(&f.surface));
                continuous.push(super::step_continuity::proven_continuous(&f.surface));
                let surface = Metered::in_pass(f.surface, cap, Arc::clone(&pass));
                meters.push(Arc::clone(&surface.meter));
                CompressedFace { boundaries: f.boundaries, orientation: f.orientation, surface }
            })
            .collect();
        let edges = shell.edges.into_iter().map(|e| truck_topology::compress::CompressedEdge {
            vertices: e.vertices, curve: super::step_division::CurveGuard::in_pass(e.curve, cap, Arc::clone(&pass)),
        }).collect();
        Self { shell: CompressedShell { vertices: shell.vertices, edges, faces }, meters, kinds, continuous, pass }
    }

    /// Start a new pass: every face gets its whole budget again.
    pub(super) fn reset(&self) {
        self.pass.reset();
        self.meters.iter().for_each(|m| m.reset());
        self.shell.edges.iter().for_each(|e| e.curve.meter.reset());
    }

    /// The pass stopped after [`super::step_meter::PASS_TRIPS`] faces or
    /// edges ran out (#1599): the faces it cut short were never judged, so
    /// the pass is refused and no coarser mesh is kept.
    pub(super) fn stopped(&self) -> bool {
        self.pass.stopped()
    }

    /// The face meters, in face order.
    #[cfg(test)]
    pub(super) fn meters(&self) -> &[Arc<Meter>] {
        &self.meters
    }

    /// The face to report for a pass in which faces ran past the cap. The
    /// pass counts as continuous only if EVERY tripped face is proven and
    /// was never divided outside its range; otherwise the first face that
    /// is not is reported, so the refusal names the face that may jump
    /// (#1568 review: a proven face tripping beside one that may jump must
    /// not let the fallback keep the jump's smear).
    pub(super) fn tripped(&self) -> Option<Trip> {
        let mut trips = self.meters.iter().enumerate().filter(|(_, m)| m.refused()).map(|(i, _)| self.trip(i));
        let first = trips.next()?;
        Some(if first.continuous { trips.find(|t| !t.continuous).unwrap_or(first) } else { first })
    }

    fn trip(&self, index: usize) -> Trip {
        Trip {
            index,
            kind: self.kinds.get(index).copied().unwrap_or("surface"),
            // Proven on the surface's own range only: a face any pass
            // divided outside that range is not covered by the proof.
            // A stopped pass cut other faces short: one of them may jump.
            continuous: !self.stopped()
                && self.continuous.get(index).copied().unwrap_or(false)
                && self.meters.get(index).is_some_and(|m| !m.outside.load(Ordering::Relaxed) && m.fault().is_none()),
        }
    }

    /// The boundary search fault of face `index` in this pass, if any. A
    /// faulted face is never continuous: no coarser mesh of it is kept.
    pub(super) fn fault(&self, index: usize) -> Option<Fault> {
        self.meters.get(index).and_then(|m| m.fault())
    }
}

/// The STEP entity number of each face of `shell`, in the order truck
/// converts them - or `None` when the counts differ (truck drops a face it
/// cannot convert, and the numbering would then name the wrong face).
pub(super) fn face_entities(shell: &truck_stepio::r#in::ShellHolder, faces: usize) -> Option<Vec<u64>> {
    use truck_stepio::r#in::ruststep::{ast::Name, tables::PlaceHolder};
    let ids: Option<Vec<u64>> = shell
        .cfs_faces
        .iter()
        .map(|f| match f {
            PlaceHolder::Ref(Name::Entity(id)) => Some(*id),
            _ => None,
        })
        .collect();
    ids.filter(|ids| ids.len() == faces)
}

/// The refusal for the tripped face of a shell of `faces` faces, naming
/// its STEP entity when the file's face list maps one-to-one. A face
/// proven continuous is only reached here from the first, coarsest pass
/// (a finer one falls back to the coarser mesh), so its text says it is
/// too dense, not broken.
pub(super) fn refusal(trip: Trip, faces: usize, entity: Option<u64>, shell: Option<u64>, cap: u64) -> ImportError {
    let face = face_name(trip, faces, entity, shell);
    let why = if trip.continuous {
        "did not finish within {cap} evaluations at the coarsest tolerance (a continuous surface, too dense to divide)"
    } else {
        "did not converge within {cap} evaluations (a discontinuous or degenerate surface)"
    };
    let why = why.replace("{cap}", &cap.to_string());
    ImportError::Step(format!(
        "{face} could not be tessellated: its surface subdivision {why} - re-export the model, or import an STL"
    ))
}

/// How a refusal names a face: its STEP entity, kind, place and shell.
pub(super) fn face_name(trip: Trip, faces: usize, entity: Option<u64>, shell: Option<u64>) -> String {
    let Trip { index, kind, .. } = trip;
    let shell = shell.map(|s| format!(" in shell #{s}")).unwrap_or_default();
    match entity {
        Some(id) => format!("face #{id} ({kind}, face {} of {faces}{shell})", index + 1),
        None => format!("face {} of {faces} ({kind}{shell})", index + 1),
    }
}

#[cfg(test)]
#[path = "step_guard_tests.rs"]
mod tests;
