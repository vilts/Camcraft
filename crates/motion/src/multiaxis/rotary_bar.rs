//! The round bar a rotary rough turns (#16): the setup stock revolved
//! about the work's fitted axis - the solid the centre-cutting audit
//! judges Rotary Offset's posed path against. Its radius is the
//! [`super::rotary_offset_in`] ladder's own top (the stock's farthest
//! vertex from the axis, never less than the part's skin), its length
//! the stations the stock and the part span. A block turning about the
//! axis sweeps the same cylinder through its corner, so for a stock
//! that is not round the bar is a superset: it errs toward solid.
//!
//! Provenance: the revolved stock envelope of 4-axis / mill-turn
//! practice (a bar's material is everything within its radius between
//! its faces); our own design. No vendor material consulted.

use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::plunge_law::Bar;

use super::axis;

/// The revolved bar of `stock` (the part's own skin when `None`) about
/// the axis fitted to the part's `facets` - the fit
/// [`super::rotary_offset_in`] wraps about, so the bar stands in the
/// path's frame. A fit that fails, or no finite extent, is the error.
pub fn revolved_bar(facets: &[Triangle], stock: Option<&[Triangle]>) -> Result<Bar, GenError> {
    let ax = axis::fit(facets)?;
    revolved_bar_about(facets, stock, ax)
}

/// [`revolved_bar`] about a stated axis the caller has judged (#2772
/// parts 1-2, `axis_stated::judged_any`): the bar the centre-cutting
/// audit reads must be the same line the kernel wrapped about.
pub fn revolved_bar_stated(
    facets: &[Triangle],
    stock: Option<&[Triangle]>,
    ax: axis::Axis,
) -> Result<Bar, GenError> {
    revolved_bar_about(facets, stock, ax)
}

/// The bar about `ax`, the plain entry's own body.
fn revolved_bar_about(
    facets: &[Triangle],
    stock: Option<&[Triangle]>,
    ax: axis::Axis,
) -> Result<Bar, GenError> {
    let u = ax.dir;
    let (mut radius, mut lo, mut hi) = (0.0f64, f64::INFINITY, f64::NEG_INFINITY);
    for q in facets.iter().chain(stock.unwrap_or(&[])).flat_map(|t| [t.a, t.b, t.c]) {
        let d = q - ax.origin;
        let v = d.dot(u);
        let w = Point3::new(d.x - u.x * v, d.y - u.y * v, d.z - u.z * v);
        let rho = w.dot(w).sqrt();
        if rho.is_finite() && v.is_finite() {
            radius = radius.max(rho);
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    if !(radius > 0.0 && lo.is_finite() && hi.is_finite() && lo < hi) {
        return Err(GenError::Invalid("the bar has no radius or length to read as stock".into()));
    }
    Ok(Bar { origin: ax.origin, dir: u, radius, lo, hi })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16-sided prism of radius `r` from z `z0` to `z1` about world Z.
    fn prism(r: f64, z0: f64, z1: f64) -> Vec<Triangle> {
        let n = 16usize;
        let pt = |j: usize, z: f64| {
            let th = std::f64::consts::TAU * j as f64 / n as f64;
            Point3::new(r * th.cos(), r * th.sin(), z)
        };
        let mut f = Vec::new();
        for j in 0..n {
            let k = (j + 1) % n;
            f.push(Triangle::new(pt(j, z0), pt(k, z0), pt(k, z1)));
            f.push(Triangle::new(pt(j, z0), pt(k, z1), pt(j, z1)));
            f.push(Triangle::new(Point3::new(0.0, 0.0, z0), pt(k, z0), pt(j, z0)));
            f.push(Triangle::new(Point3::new(0.0, 0.0, z1), pt(j, z1), pt(k, z1)));
        }
        f
    }

    #[test]
    fn the_bar_is_the_stock_cylinder_about_the_part_axis() {
        let part = prism(6.0, -16.0, 16.0);
        let stock = prism(10.0, -16.0, 16.0);
        let bar = revolved_bar(&part, Some(&stock)).expect("bar");
        assert!((bar.radius - 10.0).abs() < 1e-6, "{bar:?}");
        assert!(bar.holds(Point3::new(9.0, 0.0, 0.0)), "inside the radius");
        assert!(!bar.holds(Point3::new(10.5, 0.0, 0.0)), "air outside the radius");
        assert!(!bar.holds(Point3::new(7.5, 7.5, 0.0)), "the box corner is air");
        assert!(!bar.holds(Point3::new(0.0, 0.0, 17.0)), "past the bar's end");
        let none = revolved_bar(&part, None).expect("bar of the skin");
        assert!((none.radius - 6.0).abs() < 1e-6, "{none:?}");
    }
}
