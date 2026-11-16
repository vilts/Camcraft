//! The simulated lathe-held tool (#1038): a turning steel's modelled
//! body (#369, `ToolBody::facets` in the turning tool frame) posed at
//! the playhead in the very frame the holder law reads it in - the tip
//! at the move's point, +x along the path's signed radial, +z along the
//! spindle, +y = z x x - drawn as modelled, not revolved. Specced from
//! the handbook turning-tool convention (X radial, Z along the spindle)
//! and plain linear algebra; our own design, no vendor file or source
//! consulted.
//!
//! Display only: nothing here writes the world or feeds a verdict. The
//! frame is `operation::lathe_body_axes` - the holder law's own fit of
//! the spindle to the op's source and its own signed radial off the
//! op's path - so what is drawn stands where the law judged it. A tool
//! with no lathe body is not this module's (`Ok(None)`): it draws as it
//! did before. A frame that cannot be fitted is refused by name and
//! nothing is drawn, never a guessed pose.

use std::sync::Arc;

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::geometry::{Point3, Triangle};
use crate::schema::{ToolBody, Toolpath};
use crate::tool_body::BodyFrame;
use crate::model;

/// Most a supplied frame's two directions may stand off square (a
/// cosine), the placement's own tolerance.
const SQUARE_TOL: f64 = camcraft_tools::lathe_holder::SQUARE_TOL;

/// The turning tool frame in world: unit `radial` (+x) and `along`
/// (+z, the spindle).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatheAxes {
    pub radial: [f64; 3],
    pub along: [f64; 3],
}

impl LatheAxes {
    /// Both directions made unit and checked square; `None` for a
    /// zero, non-finite or skew pair (nothing is drawn then).
    #[must_use]
    pub fn new(radial: [f64; 3], along: [f64; 3]) -> Option<Self> {
        let (x, z) = (unit(radial)?, unit(along)?);
        (dot(x, z).abs() <= SQUARE_TOL).then_some(Self { radial: x, along: z })
    }

    /// +y = z x x, normal to the working plane.
    #[must_use]
    pub fn normal(&self) -> [f64; 3] {
        cross(self.along, self.radial)
    }
}

/// Turning-tool-frame facets posed with the tip at `tip` (world): a
/// point (x, y, z) lands at `tip + x*radial + y*normal + z*along`.
#[must_use]
pub fn posed(facets: &[Triangle], tip: [f64; 3], axes: &LatheAxes) -> Vec<Triangle> {
    let (x, y, z) = (axes.radial, axes.normal(), axes.along);
    let map = |p: Point3| {
        let w = |k: usize| tip[k] + p.x * x[k] + p.y * y[k] + p.z * z[k];
        Point3::new(w(0), w(1), w(2))
    };
    facets.iter().map(|t| Triangle::new(map(t.a), map(t.b), map(t.c))).collect()
}

/// A lathe body's tool-frame facets with the world frame they are posed in.
pub type Drawn = (Arc<[Triangle]>, LatheAxes);

/// What the cache was built from: the op, its path's signature and the
/// body's facets (by identity).
#[derive(Clone, Debug)]
struct Key {
    op: String,
    sig: u64,
    facets: Arc<[Triangle]>,
}

impl Key {
    fn same(&self, other: &Self) -> bool {
        self.op == other.op && self.sig == other.sig && Arc::ptr_eq(&self.facets, &other.facets)
    }
}

/// The lathe body of the one tool at the playhead and its fitted frame,
/// refitted only when the op, its path or the body changed.
#[derive(Debug, Default)]
pub struct LatheBodyCache {
    built: Option<(Key, LatheAxes)>,
}

impl LatheBodyCache {
    /// `op`'s tool's lathe body with the frame it is judged in:
    /// `Ok(None)` when the tool carries no lathe body or the op has no
    /// path yet (draw as before), `Err` naming why a lathe body cannot
    /// be posed.
    pub fn of(&mut self, world: &World, op: &str) -> Result<Option<Drawn>, String> {
        let Some(e) = model::find_by_name(world, op) else { return self.none() };
        let Some(o) = model::operation_of(world, &e) else { return self.none() };
        let Some(body) = lathe_body(world, &o.tool) else { return self.none() };
        let Some(sig) = e.entity_view(world).try_get::<&Toolpath>(Toolpath::path_sig) else { return self.none() };
        let key = Key { op: op.to_owned(), sig, facets: Arc::clone(&body.facets) };
        if let Some((held, axes)) = &self.built
            && held.same(&key)
        {
            return Ok(Some((key.facets, *axes)));
        }
        self.built = None;
        let path = model::toolpath_of(world, &e).ok_or_else(|| format!("{op}: its path went away"))?;
        let part = crate::operation::source_facets(world, o.setup.as_deref(), &o.src).map_err(|e| format!("{op}: the lathe frame needs the op's source: {e}"))?;
        let (radial, along) = crate::operation::lathe_body_axes(&part, &path.path).map_err(|e| format!("{op}: the lathe body has no frame: {e}"))?;
        let axes = LatheAxes::new(radial, along).ok_or_else(|| format!("{op}: the lathe body's frame is not square"))?;
        self.built = Some((key.clone(), axes));
        Ok(Some((key.facets, axes)))
    }

    fn none(&mut self) -> Result<Option<Drawn>, String> {
        self.built = None;
        Ok(None)
    }

    /// Drop the frame (the transport closed).
    pub fn clear(&mut self) {
        self.built = None;
    }

    /// Does the cache hold no frame?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.built.is_none()
    }
}

/// The tool `tool`'s body, when it is a lathe-frame one.
#[must_use]
pub fn lathe_body(world: &World, tool: &str) -> Option<ToolBody> {
    let te = model::find_by_name(world, tool)?;
    crate::tool_body::body_of(world, &te).filter(|b| matches!(b.frame, BodyFrame::Lathe { .. }))
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = dot(a, a).sqrt();
    (n.is_finite() && n > 1e-12).then(|| [a[0] / n, a[1] / n, a[2] / n])
}
