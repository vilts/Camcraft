//! Axial cone footprint at the finished wall top, from machine-tool handbook point geometry and issues #3020/#3451.

use crate::cl::{Move, Toolpath};
use crate::verify::probe::{Atom, Prof};
use crate::verify::{Cutter, Mode, VerifyContext};

/// Static hole walls end at the part top when supplied, else the stock top.
/// For an ordinary axial cone,
/// only its radius below that plane can cut those walls. Model sections,
/// other paths and untrusted cutter shapes retain the nominal radius.
pub(super) struct Radius {
    nominal: f64,
    top: f64,
    cone: Option<Atom>,
}

impl Radius {
    pub(super) fn new(path: &Toolpath, cx: &VerifyContext) -> Self {
        let cone = if cx.model.is_none()
            && cx.mode == Mode::Boundary
            && !cx.rims.is_empty()
            // An explicit axis is modal: even a later None can retain
            // a prior tilt. Keep the existing law for the whole path.
            && path.moves.iter().all(|m| m.axis.is_none())
        {
            Cutter::judged(cx.tool, cx.cutter)
                .atoms()
                .into_iter()
                .next()
                .filter(|atom| matches!(atom.prof, Prof::Cone { .. }))
        } else {
            None
        };
        Self { nominal: cx.tool.radius, top: cx.rim_top_z.unwrap_or(cx.stock_top_z), cone }
    }

    pub(super) fn at(&self, m: &Move) -> f64 {
        let Some(cone) = self.cone else { return self.nominal };
        if m.arc.is_some() || m.from.x != m.to.x || m.from.y != m.to.y {
            return self.nominal;
        }
        // The deepest endpoint bounds an axial linear move, in either
        // direction. At or above the cone's top, keep the full cylinder.
        let depth = self.top - m.from.z.min(m.to.z);
        if !depth.is_finite() || depth >= cone.z1 {
            return self.nominal;
        }
        cone.rho(depth.max(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::Radius;
    use crate::cl::{Move, MoveKind, Toolpath};
    use crate::geo2d::{Point2, Region};
    use crate::geometry::{Point3, Triangle};
    use crate::verify::{Cutter, Mode, ToolGeom, VerifyContext};

    fn radius_at_depth_one(flute_len: f64, model: Option<&[Triangle]>, stock_top_z: f64) -> f64 {
        let geometry = ToolGeom {
            radius: 3.0,
            nose_radius: 0.0,
            flute_len,
            shank_radius: 3.0,
            shank_len: 40.0,
            holder_radius: 10.0,
            holder_len: 60.0,
        };
        let cutter = Cutter { tip_angle: 120.0, ..Cutter::flat(&geometry) };
        let movement = Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 12.0), Point3::new(0.0, 0.0, 11.0), Some(80.0));
        let path = Toolpath { moves: vec![movement], ..Default::default() };
        let region = Region::default();
        let rims = [(Point2::new(0.0, 0.0), 2.5)];
        let context = VerifyContext {
            region: &region,
            model,
            tool: &geometry,
            cutter: Some(cutter),
            mode: Mode::Boundary,
            stock_top_z,
            rims: &rims,
            ..Default::default()
        };
        Radius::new(&path, &context).at(&path.moves[0])
    }

    #[test]
    fn model_sections_keep_the_nominal_radius() {
        assert!(radius_at_depth_one(30.0, None, 12.0) < 2.0, "the matching cone clears the pilot without a model");
        assert_eq!(radius_at_depth_one(30.0, Some(&[]), 12.0), 3.0, "model sections retain their existing radius law");
    }

    #[test]
    fn a_short_flute_cone_keeps_the_nominal_radius_at_the_shank() {
        assert_eq!(radius_at_depth_one(0.5, None, 12.0), 3.0, "the matching shank enters before the programmed depth");
    }

    #[test]
    fn nonfinite_stock_top_keeps_the_nominal_radius() {
        for top in [f64::NAN, f64::INFINITY] {
            assert_eq!(radius_at_depth_one(30.0, None, top), 3.0, "top {top} cannot prove a reduced cone radius");
        }
    }
}
