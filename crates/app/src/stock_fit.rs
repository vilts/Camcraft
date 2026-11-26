//! "Fit stock to models" (#1538): the box, cylinder and per-side fits
//! measure the setup's PARTS in its frame - never the vises, accessories,
//! plates or tagged fixtures around them, which the collision laws judge
//! instead. One rule for the GUI's Fit button and the MCP `fit_stock`
//! tool, so both build the same undoable `SetStock`.
//! Provenance: our own design; stock allowance from general machining
//! knowledge (the fits are `camcraft_geometry::stock`'s).

use flecs_ecs::prelude::World;

use crate::commands::SetStock;
use crate::geometry::{Aabb, Point3, quat_conjugate, quat_rotate_point};
use crate::schema::Location;
use crate::stock::{self, StockShape};
use crate::{model, setup_align};

/// Which fit, with its allowance in mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FitKind {
    /// A box, `margin` on every side.
    Box(f64),
    /// A cylinder on Z, snug about the parts' largest radius plus `margin`.
    Cylinder(f64),
    /// A box with its own allowance per side: -X, +X, -Y, +Y, -Z, +Z.
    Sides([f64; 6]),
}

/// The parts' bounds in a setup's frame - the box every fit wraps and the
/// Stock page's "Models:" readout shows. Fixtures are left out.
#[must_use]
pub fn part_bounds(world: &World, declared: Option<&str>) -> Option<Aabb> {
    setup_align::part_aabb_in(world, declared)
}

/// The parts' largest horizontal distance from the vertical line through
/// `c`, in a setup's frame - the radius twin of [`part_bounds`] (#230's
/// snug cylinder fit). Fixtures are left out.
#[must_use]
pub fn part_radius_about_in(world: &World, declared: Option<&str>, c: [f64; 2]) -> Option<f64> {
    model::model_entities(world)
        .into_iter()
        .filter(|(_, e)| !model::is_fixture(world, e))
        .filter_map(|(_, e)| setup_align::facets_in(world, &e, declared))
        .flatten()
        .flat_map(|t| [t.a, t.b, t.c])
        .map(|p| (p.x - c[0]).hypot(p.y - c[1]))
        .filter(|r| r.is_finite())
        .reduce(f64::max)
}

/// The fitted stock shape and its position for the setup `declared`, or
/// why there is none (refused by name).
pub fn fitted(world: &World, declared: Option<&str>, kind: FitKind) -> Result<(StockShape, Point3), String> {
    check_kind(kind)?;
    let bounds = part_bounds(world, declared).ok_or_else(|| nothing_to_fit(world))?;
    fit_shape(&bounds, kind, |c| part_radius_about_in(world, declared, c))
}

/// [`fitted`] for a stock that keeps the turn `rot` (#2576): the parts are
/// measured in the stock's own frame (each vertex turned back by `rot`),
/// the fit is made there, and its base point is turned onto the setup's
/// frame. A stock laid along X or Y (or tilted) thus wraps the parts on
/// its own axes, never a world-frame size put on a turned solid.
pub fn fitted_turned(world: &World, declared: Option<&str>, kind: FitKind, rot: [f64; 4]) -> Result<(StockShape, Point3), String> {
    if is_upright(rot) {
        return fitted(world, declared, kind);
    }
    check_kind(kind)?;
    let back = quat_conjugate(rot);
    let points: Vec<Point3> = model::model_entities(world)
        .into_iter()
        .filter(|(_, e)| !model::is_fixture(world, e))
        .filter_map(|(_, e)| setup_align::facets_in(world, &e, declared))
        .flatten()
        .flat_map(|t| [t.a, t.b, t.c])
        .map(|p| quat_rotate_point(back, p))
        .collect();
    let bounds = setup_align::bounds(points.iter().copied()).ok_or_else(|| nothing_to_fit(world))?;
    let radius = |c: [f64; 2]| {
        points.iter().map(|p| (p.x - c[0]).hypot(p.y - c[1])).filter(|r| r.is_finite()).reduce(f64::max)
    };
    let (shape, base) = fit_shape(&bounds, kind, radius)?;
    Ok((shape, quat_rotate_point(rot, base)))
}

/// True when `rot` is no turn at all (q or -q of identity).
fn is_upright(rot: [f64; 4]) -> bool {
    rot[..3].iter().all(|v| v.abs() < 1e-12)
}

/// The fit itself, from the parts' bounds and their radius about a centre.
fn fit_shape(bounds: &Aabb, kind: FitKind, radius: impl Fn([f64; 2]) -> Option<f64>) -> Result<(StockShape, Point3), String> {
    let fit = match kind {
        FitKind::Box(margin) => stock::fit_box(bounds, margin),
        FitKind::Cylinder(margin) => {
            let c = [(bounds.min.x + bounds.max.x) * 0.5, (bounds.min.y + bounds.max.y) * 0.5];
            radius(c).and_then(|r| stock::fit_cylinder_about(bounds, r, margin))
        }
        FitKind::Sides(sides) => stock::fit_box_sides(bounds, sides),
    };
    fit.ok_or_else(|| "stock fit refused: models have no usable extent".into())
}

/// Refuse a negative or non-finite allowance by name.
fn check_kind(kind: FitKind) -> Result<(), String> {
    if let FitKind::Sides(sides) = kind
        && !sides.iter().all(|s| s.is_finite() && *s >= 0.0)
    {
        return Err("stock fit refused: every side's allowance must be zero or more".into());
    }
    if let FitKind::Box(margin) | FitKind::Cylinder(margin) = kind
        && !(margin.is_finite() && margin >= 0.0)
    {
        return Err(format!("stock fit refused: the margin must be zero or more (got {margin} mm)"));
    }
    Ok(())
}

/// Why there is nothing to fit: no models at all, or only fixtures -
/// which are never measured as the part.
fn nothing_to_fit(world: &World) -> String {
    if model::model_entities(world).iter().any(|(_, e)| model::is_fixture(world, e)) {
        "nothing to fit: no part models - fixtures are not measured".into()
    } else {
        "nothing to fit: no models in the document".into()
    }
}

/// The undoable command that fits the ACTIVE setup's stock: the fitted
/// size and placement, the stock's deliberate rotation kept and the fit
/// made in that rotation's frame (#2576) - with the fitted shape, for the
/// caller's report.
pub fn fit_command(world: &World, kind: FitKind) -> Result<(SetStock, StockShape), String> {
    let active = crate::setup::active(world);
    let stock_name = crate::setup::stock_entity_name(world, active.as_deref());
    let rot = model::find_by_name(world, &stock_name)
        .and_then(|e| model::transform_of(world, &e))
        .map_or_else(|| Location::default().rot, |loc| loc.rot);
    let (shape, pos) = fitted_turned(world, active.as_deref(), kind, rot)?;
    SetStock::new(world, shape.clone(), Location { pos, rot })
        .map(|cmd| (cmd, shape))
        .map_err(|e| format!("stock not fitted: {e}"))
}
