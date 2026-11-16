//! The bench's bound on truck's mesher (#1697): every face's surface is
//! metered, so a face whose subdivision never ends is refused by name, and
//! the bench's cancel token stops it, instead of a hang the test thread can
//! never get back from.
//! Provenance: our own design, the bench's copy of the evaluation meter
//! crates/app puts on its STEP faces (#1536, `step_guard`); the subdivision
//! it meters is truck's published generic `parameter_division` (ricosjp,
//! Apache-2.0). Nothing from any CAM vendor.
//!
//! Why the bench needs its own: truck's subdivision splits a parameter cell
//! while its midpoint stands off the patch of its corners, and on a B-spline
//! that jumps (an interior knot past its degree, testdata's
//! bspline_discontinuous.step) or on a range far off the knot domain (the
//! pole of a corner blend, fillet_corner.step) no cell ever stops splitting.
//! The app meters each face and refuses such a file by name; the bench
//! cannot import the app, and truck itself polls no token. Past the cap, or
//! once the token is cancelled, every evaluation answers one fixed point:
//! no cell is then "far" and the subdivision ends within one sweep.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use solidcore::kernel::CancelToken;
use truck_stepio::r#in::alias::{
    Curve3D, D2, ParameterDivision2D, ParameterRange, ParametricSurface, ParametricSurface3D,
    Point3, SPHint2D, SearchNearestParameter, SearchParameter, Surface, Vector3, algo,
};
use truck_topology::compress::{CompressedFace, CompressedShell};

/// Surface evaluations one face may spend in one tessellation. The app's
/// first cap (#1536); every bench fixture that converges stays far under it.
pub const FACE_EVAL_CAP: u64 = 25_000_000;

/// One face's evaluation count, and the bench's token.
#[derive(Debug)]
pub struct Meter {
    used: AtomicU64,
    cap: u64,
    stop: CancelToken,
}

impl Meter {
    /// Count one evaluation; `false` once the cap is spent or the token fired.
    fn spend(&self) -> bool {
        !self.stop.is_cancelled() && self.used.fetch_add(1, Ordering::Relaxed) < self.cap
    }
    /// Why this face's mesh is not to be used, if it is not.
    fn refused(&self) -> Option<&'static str> {
        if self.used.load(Ordering::Relaxed) > self.cap {
            Some("its subdivision did not converge within the evaluation cap")
        } else if self.stop.is_cancelled() {
            Some("the time bound cancelled its subdivision")
        } else {
            None
        }
    }
}

/// A STEP surface whose evaluations are counted against a [`Meter`].
#[derive(Clone, Debug)]
pub struct Metered {
    inner: Surface,
    meter: Arc<Meter>,
}

fn kind(s: &Surface) -> &'static str {
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
        if self.meter.spend() {
            self.inner.subs(u, v)
        } else {
            Point3::new(0.0, 0.0, 0.0)
        }
    }
    fn uder(&self, u: f64, v: f64) -> Vector3 {
        self.inner.uder(u, v)
    }
    fn vder(&self, u: f64, v: f64) -> Vector3 {
        self.inner.vder(u, v)
    }
    fn uuder(&self, u: f64, v: f64) -> Vector3 {
        self.inner.uuder(u, v)
    }
    fn uvder(&self, u: f64, v: f64) -> Vector3 {
        self.inner.uvder(u, v)
    }
    fn vvder(&self, u: f64, v: f64) -> Vector3 {
        self.inner.vvder(u, v)
    }
    fn parameter_range(&self) -> (ParameterRange, ParameterRange) {
        self.inner.parameter_range()
    }
    fn u_period(&self) -> Option<f64> {
        self.inner.u_period()
    }
    fn v_period(&self) -> Option<f64> {
        self.inner.v_period()
    }
}

impl ParametricSurface3D for Metered {
    fn normal(&self, u: f64, v: f64) -> Vector3 {
        self.inner.normal(u, v)
    }
}

impl ParameterDivision2D for Metered {
    fn parameter_division(
        &self,
        range: ((f64, f64), (f64, f64)),
        tol: f64,
    ) -> (Vec<f64>, Vec<f64>) {
        match &self.inner {
            // truck's B-spline and NURBS surfaces divide with this generic
            // algorithm; run on the wrapper, every probe is metered.
            Surface::BSplineSurface(_) | Surface::NurbsSurface(_) => {
                let division = algo::surface::parameter_division(self, range, tol);
                // A refused face is refused after the pass: the grid it grew
                // down the runaway seam is not handed on to be triangulated.
                if self.meter.refused().is_some() {
                    let ((u0, u1), (v0, v1)) = range;
                    return (vec![u0, u1], vec![v0, v1]);
                }
                division
            }
            other => other.parameter_division(range, tol),
        }
    }
}

impl SearchParameter<D2> for Metered {
    type Point = Point3;
    fn search_parameter<H: Into<SPHint2D>>(
        &self,
        point: Point3,
        hint: H,
        trials: usize,
    ) -> Option<(f64, f64)> {
        self.inner.search_parameter(point, hint, trials)
    }
}

impl SearchNearestParameter<D2> for Metered {
    type Point = Point3;
    fn search_nearest_parameter<H: Into<SPHint2D>>(
        &self,
        point: Point3,
        hint: H,
        trials: usize,
    ) -> Option<(f64, f64)> {
        self.inner.search_nearest_parameter(point, hint, trials)
    }
}

/// A shell with every face metered, and the meters in face order.
pub struct Guarded {
    pub shell: CompressedShell<Point3, Curve3D, Metered>,
    meters: Vec<(Arc<Meter>, &'static str)>,
}

impl Guarded {
    pub fn new(
        shell: CompressedShell<Point3, Curve3D, Surface>,
        cap: u64,
        stop: &CancelToken,
    ) -> Self {
        let mut meters = Vec::with_capacity(shell.faces.len());
        let faces = shell
            .faces
            .into_iter()
            .map(|f| {
                let meter = Arc::new(Meter {
                    used: AtomicU64::new(0),
                    cap,
                    stop: stop.clone(),
                });
                meters.push((Arc::clone(&meter), kind(&f.surface)));
                CompressedFace {
                    boundaries: f.boundaries,
                    orientation: f.orientation,
                    surface: Metered {
                        inner: f.surface,
                        meter,
                    },
                }
            })
            .collect();
        Self {
            shell: CompressedShell {
                vertices: shell.vertices,
                edges: shell.edges,
                faces,
            },
            meters,
        }
    }

    /// The refusal of the first face whose mesh is not to be used, naming
    /// the face by its place in the shell and its kind.
    pub fn refusal(&self) -> Option<String> {
        let n = self.meters.len();
        self.meters.iter().enumerate().find_map(|(i, (m, kind))| {
            m.refused()
                .map(|why| format!("face {} of {n} ({kind}) not meshed: {why}", i + 1))
        })
    }
}
