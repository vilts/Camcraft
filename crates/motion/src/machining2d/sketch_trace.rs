//! Analytic centerline marking from directed sketch curves. Provenance: planar circle
//! geometry, RS-274 XY arcs, and CamCraft's existing planar depth and entry laws.

use camcraft_geometry::dxf::Primitive;
use camcraft_geometry::sketch_chains::SketchChainRef;

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::links::Links;
use crate::ring_zig::MIN_ARC_CHORD;
use std::f64::consts::PI;

use super::levels::{Motion, cut_levels_under, run_level_walks};
use super::{GenError, Seg, TraceParams};

/// Directed, disconnected centerlines. Each walk has a separate plunge and lift.
#[derive(Default)]
pub struct TraceStrokes {
    passes: Vec<Vec<Point2>>,
    walks: Vec<Vec<Seg>>,
}

impl TraceStrokes {
    /// Read canonical chain direction for climb. Reversing a chain flips every arc
    /// as well as the order of the curves. The placement must be a rigid XY transform.
    pub fn add_chain(
        &mut self,
        chain: SketchChainRef<'_>,
        climb: bool,
        place: impl Fn(Point2) -> Point2,
    ) -> Result<(), GenError> {
        let place = |p| {
            let q = place(p);
            if q.x.is_finite() && q.y.is_finite() {
                Ok(q)
            } else {
                Err(GenError::Invalid(
                    "the sketch placement gives a non-finite coordinate".into(),
                ))
            }
        };
        let segments = chain.segments();
        let mut points: Vec<Point2> = Vec::new();
        let mut walk = Vec::new();
        for i in 0..segments.len() {
            let s = &segments[if climb { i } else { segments.len() - 1 - i }];
            let (start, end) = if climb {
                (s.start, s.end)
            } else {
                (s.end, s.start)
            };
            let (start, end) = (place(start)?, place(end)?);
            if start == end && matches!(s.curve, Primitive::Line { .. }) {
                return Err(GenError::Invalid(
                    "a sketch stroke collapses at its placed coordinates".into(),
                ));
            }
            if let Some(prev) = points.last()
                && (prev.x - start.x).hypot(prev.y - start.y) > 1e-9
            {
                // A tolerance-joined sketch can have a real gap. Lift across it
                // instead of changing the next arc's radius or machining a bridge.
                self.passes.push(std::mem::take(&mut points));
                self.walks.push(std::mem::take(&mut walk));
            }
            if points.is_empty() {
                points.push(start);
            }
            match &s.curve {
                Primitive::Line { .. } => walk.push(Seg::Line(vec![end])),
                Primitive::Arc {
                    center,
                    radius,
                    start_angle,
                    sweep_angle,
                } => {
                    let ccw = (*sweep_angle > 0.0) == climb;
                    let world_center = place(*center)?;
                    // Splitting a major arc at its angular midpoint gives two equal
                    // sweeps <= pi, even when the original endpoints nearly coincide.
                    // The midpoint is the same in either traversal direction.
                    if sweep_angle.abs() > PI {
                        let (sin, cos) = (start_angle + 0.5 * sweep_angle).sin_cos();
                        let mid = place(Point2::new(
                            center.x + radius * cos,
                            center.y + radius * sin,
                        ))?;
                        points.push(mid);
                        walk.push(Seg::Arc {
                            center: world_center,
                            ccw,
                            to: mid,
                        });
                    }
                    walk.push(Seg::Arc {
                        center: world_center,
                        ccw,
                        to: end,
                    });
                }
                Primitive::Circle { center, radius } => {
                    let opposite = place(Point2::new(center.x - radius, center.y))?;
                    let center = place(*center)?;
                    points.push(opposite);
                    walk.push(Seg::Arc {
                        center,
                        ccw: climb,
                        to: opposite,
                    });
                    walk.push(Seg::Arc {
                        center,
                        ccw: climb,
                        to: end,
                    });
                }
            }
            points.push(end);
        }
        if walk.is_empty() {
            return Err(GenError::Empty(
                "the selected sketch chain has no strokes".into(),
            ));
        }
        self.passes.push(points);
        self.walks.push(walk);
        Ok(())
    }

    /// Carry all vertices and arc centres together through a rigid XY frame change.
    pub fn transform(&mut self, place: impl Fn(Point2) -> Point2) {
        for p in self.passes.iter_mut().flatten() {
            *p = place(*p);
        }
        for seg in self.walks.iter_mut().flatten() {
            match seg {
                Seg::Line(pts) => {
                    for p in pts {
                        *p = place(*p);
                    }
                }
                Seg::Arc { center, to, .. } => {
                    *center = place(*center);
                    *to = place(*to);
                }
            }
        }
    }

    pub fn trace(&self, p: &TraceParams, links: Links) -> Result<Toolpath, GenError> {
        p.validate()?;
        if self.walks.is_empty() {
            return Err(GenError::Empty(
                "the selected sketch has no strokes to mark".into(),
            ));
        }
        // Check the actual chord after placement, WCS rotation, and any joined
        // endpoints. G2/G3 with coincident posted endpoints can command a full turn.
        for (points, walk) in self.passes.iter().zip(&self.walks) {
            for (ends, seg) in points.windows(2).zip(walk) {
                if matches!(seg, Seg::Arc { .. }) {
                    let chord = (ends[1].x - ends[0].x).hypot(ends[1].y - ends[0].y);
                    if !chord.is_finite() || chord < MIN_ARC_CHORD {
                        return Err(GenError::Invalid(format!(
                            "the sketch arc is too short to post safely: its chord must be at least {MIN_ARC_CHORD} mm"
                        )));
                    }
                }
            }
        }
        let zs = cut_levels_under(p.top_z, p.stock_top, p.bottom_z, p.stepdown)?;
        // Bound the cross-product of the imported drawing and requested depth levels.
        let per_level = self
            .passes
            .iter()
            .map(|v| v.len().saturating_add(3))
            .sum::<usize>();
        if per_level.saturating_mul(zs.len()) > camcraft_geometry::sketch_chains::MAX_PRIMITIVES {
            return Err(GenError::Invalid(
                "the sketch mark needs too many moves - select fewer strokes or increase stepdown"
                    .into(),
            ));
        }
        let top = p.stock_top.unwrap_or(p.top_z).max(p.top_z);
        let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
        let motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed)
            .with_top(top)
            .with_links(links, 0.0);
        // A mark is verified against the actual stock surface. Approach each level
        // from that surface: an earlier pass does not clear all the space between
        // two centerlines. The motion ladder handles the separate strokes at that level.
        for z in &zs {
            run_level_walks(
                &mut b,
                &self.passes,
                std::slice::from_ref(z),
                &motion,
                Some(&self.walks),
                None,
                None,
            );
            let at = b.at();
            b.push(
                MoveKind::Rapid,
                Point3::new(at.x, at.y, p.clearance_z),
                None,
            );
        }
        let path = b.finish();
        if let Some(why) = crate::approach::refusal(&path) {
            return Err(GenError::Invalid(why).with_ghost(&path));
        }
        if !path.all_finite() {
            return Err(GenError::Invalid(
                "the sketch mark has a non-finite coordinate".into(),
            ));
        }
        Ok(path)
    }
}
