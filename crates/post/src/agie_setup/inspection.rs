//! Zero-step die inspection on the Classic 2S: our own implementation over CamCraft's sketch predicates.

mod piece;

use camcraft_geometry::dxf::{LayeredPrimitive, Primitive};
use camcraft_geometry::geo2d::Point2;
use camcraft_geometry::sketch_chains::{
    ChainClearance, ChainOptions, MIN_JOIN_TOLERANCE, chain, chain_bounds,
};
use std::f64::consts::TAU;
use std::fmt::Write;
use std::path::{Path, PathBuf};

pub const INSPECTION_RECEIPT: &str = "0 working steps; inspection only; holding, leads and erosion settings unverified. Physical stock XY unknown and unchecked (native length and width 0). The perpendicular controller lead default, physical stock Z and installed travel without stated limits are unverified. ISO coordinates rounded by at most 0.000005 mm per axis. Import into a new piece and confirm [0] working steps.";

const SCALE: f64 = 100_000.0;
const LIMIT: f64 = 350.0;
const NOISE: f64 = 1e-8;
/// The offline control recognized an R2 arc with this endpoint radius difference on import.
const MAX_RADIUS_RESIDUAL: f64 = 0.000005038;

type Grid = [i64; 2];

/// An oriented source span in setup XY, before any output rounding.
pub struct InspectionSegment {
    pub start: Point2,
    pub end: Point2,
    /// Circular centre and signed sweep in radians. None denotes a straight line.
    pub arc: Option<(Point2, f64)>,
}

fn grid(p: Point2) -> Result<Grid, String> {
    let scalar = |v: f64| {
        if v.is_finite() && v.abs() <= LIMIT {
            Ok((v * SCALE).round() as i64)
        } else {
            Err("Agie inspection coordinate exceeds the finite native XY span".to_owned())
        }
    };
    Ok([scalar(p.x)?, scalar(p.y)?])
}

fn xy(g: Grid) -> Point2 {
    Point2::new(g[0] as f64 / SCALE, g[1] as f64 / SCALE)
}

fn close(p: Point2, q: Point2) -> bool {
    (p.x - q.x).hypot(p.y - q.y) <= NOISE
}

fn radius_squared(p: Grid, c: Grid) -> i64 {
    // grid bounds both coordinates to ±35,000,000 before subtraction.
    (p[0] - c[0]).pow(2) + (p[1] - c[1]).pow(2)
}

struct Block {
    start: Grid,
    end: Grid,
    curve: Primitive,
    center: Option<Grid>,
}

impl Block {
    fn new(s: &InspectionSegment) -> Result<Self, String> {
        let start = grid(s.start)?;
        let end = grid(s.end)?;
        if start == end {
            return Err("a nominal inspection span collapses on the native ISO grid".into());
        }
        let a = xy(start);
        let b = xy(end);
        let Some((center, sweep)) = s.arc else {
            return Ok(Self {
                start,
                end,
                curve: Primitive::Line { start: a, end: b },
                center: None,
            });
        };
        let c = grid(center)?;
        let (vx, vy) = (s.start.x - center.x, s.start.y - center.y);
        let (sn, cs) = sweep.sin_cos();
        let expected = Point2::new(center.x + cs * vx - sn * vy, center.y + sn * vx + cs * vy);
        if !close(expected, s.end) {
            return Err(
                "the inspection arc endpoint disagrees with its stated centre and sweep".into(),
            );
        }
        let r2 = radius_squared(start, c);
        let raw_center = xy(c);
        let radius_start = (r2 as f64).sqrt() / SCALE;
        let radius_end = (radius_squared(end, c) as f64).sqrt() / SCALE;
        if radius_start == 0.0
            || radius_end == 0.0
            || (radius_start - radius_end).abs() > MAX_RADIUS_RESIDUAL
        {
            return Err("the two native inspection arc radii differ beyond the offline-checked rounding bound".into());
        }
        // The import record retains the rounded I/J centre and both rounded endpoints.
        // For topology, use the nearest exact circle through BOTH endpoints. Reject an ill-
        // conditioned chord whose centre would move by more than one native grid unit.
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let chord = dx.hypot(dy);
        let shift = (radius_end - radius_start) * (radius_end + radius_start) / (2.0 * chord);
        if shift.abs() > 1.0 / SCALE {
            return Err("the inspection arc cannot be bounded after native rounding".into());
        }
        let cxy = Point2::new(
            raw_center.x + shift * dx / chord,
            raw_center.y + shift * dy / chord,
        );
        let radius = (a.x - cxy.x).hypot(a.y - cxy.y);
        let start_angle = (a.y - cxy.y).atan2(a.x - cxy.x);
        let end_angle = (b.y - cxy.y).atan2(b.x - cxy.x);
        let sweep_angle = if sweep > 0.0 {
            (end_angle - start_angle).rem_euclid(TAU)
        } else {
            -(start_angle - end_angle).rem_euclid(TAU)
        };
        if !sweep.is_finite()
            || sweep == 0.0
            || sweep.abs() >= TAU
            || (sweep - sweep_angle).abs() * radius > 4.0 / SCALE
        {
            return Err(
                "the signed inspection arc sweep cannot be preserved on the native grid".into(),
            );
        }
        Ok(Self {
            start,
            end,
            curve: Primitive::Arc {
                center: cxy,
                radius,
                start_angle,
                sweep_angle,
            },
            center: Some(c),
        })
    }
}

/// Rounded, simple CCW profile and the checked 3 mm threading disk.
pub struct InspectionProfile {
    blocks: Vec<Block>,
    clearance: ChainClearance,
    bounds: (Point2, Point2),
    hole: Grid,
}

impl InspectionProfile {
    pub fn new(spans: &[InspectionSegment], hole: Point2) -> Result<Self, String> {
        if spans.is_empty() || spans.len() > 9997 {
            return Err("Agie inspection needs 1 to 9997 connected nominal spans".into());
        }
        let blocks = spans
            .iter()
            .map(Block::new)
            .collect::<Result<Vec<_>, _>>()?;
        let mut area = 0.0;
        let mut items = Vec::with_capacity(blocks.len());
        for (i, b) in blocks.iter().enumerate() {
            let prev = if i == 0 { blocks.len() - 1 } else { i - 1 };
            if blocks[prev].end != b.start || !close(spans[prev].end, spans[i].start) {
                return Err("the inspection opening has a joint gap; no bridge was added".into());
            }
            let (a, z) = (xy(b.start), xy(b.end));
            area += a.x * z.y - z.x * a.y;
            if let Primitive::Arc {
                radius,
                sweep_angle,
                ..
            } = b.curve
            {
                area += radius * radius * (sweep_angle - sweep_angle.sin());
            }
            items.push(LayeredPrimitive {
                layer: "opening".into(),
                primitive: b.curve.clone(),
            });
        }
        let rings = chain(
            &items,
            ChainOptions {
                join_tolerance: MIN_JOIN_TOLERANCE,
            },
        )
        .and_then(|r| r.strict())
        .map_err(|e| format!("native inspection topology: {e}"))?;
        let [ring] = rings.closed.as_slice() else {
            return Err("native inspection geometry does not form one closed loop".into());
        };
        if area <= 0.0 || !area.is_finite() || ring.segments.len() != blocks.len() {
            return Err("native inspection geometry needs one counterclockwise contour".into());
        }
        let clearance = ChainClearance::new(ring).map_err(|e| e.to_string())?;
        let hole = grid(hole)?;
        if !clearance.contains_disk(xy(hole), 1.5 + 4.0 / SCALE) {
            return Err(
                "the entire 3 mm threading hole must be strictly inside the selected opening"
                    .into(),
            );
        }
        let (lo, hi) =
            chain_bounds(&ring.segments).ok_or("the inspection opening has no bounds")?;
        // Include source extrema as well as the native curve. The encodable circle with two
        // slightly unequal radii uses a bounded topology centre adjustment (see Block::new).
        let original: Vec<_> = spans
            .iter()
            .map(|s| {
                let primitive = match s.arc {
                    None => Primitive::Line {
                        start: s.start,
                        end: s.end,
                    },
                    Some((center, sweep_angle)) => {
                        let x = s.start.x - center.x;
                        let y = s.start.y - center.y;
                        Primitive::Arc {
                            center,
                            radius: x.hypot(y),
                            start_angle: y.atan2(x),
                            sweep_angle,
                        }
                    }
                };
                LayeredPrimitive {
                    layer: String::new(),
                    primitive,
                }
            })
            .collect();
        let (source_lo, source_hi) = camcraft_geometry::sketch_data::bounds(&original)
            .ok_or("the source inspection opening has no bounds")?;
        let adjusted = blocks.iter().any(|b| {
            b.center
                .is_some_and(|c| radius_squared(b.start, c) != radius_squared(b.end, c))
        });
        let margin = if adjusted { 4.0 / SCALE } else { 0.0 };
        let bounds = (
            Point2::new(
                (lo.x - margin).min(source_lo.x),
                (lo.y - margin).min(source_lo.y),
            ),
            Point2::new(
                (hi.x + margin).max(source_hi.x),
                (hi.y + margin).max(source_hi.y),
            ),
        );
        Ok(Self {
            blocks,
            clearance,
            bounds,
            hole,
        })
    }

    /// Reject unrelated drawing geometry in the opening. The tolerance includes native rounding.
    pub fn check_other(&mut self, index: usize, item: &Primitive) -> Result<(), String> {
        if self
            .clearance
            .obstructs(index, item, 4.0 / SCALE)
            .map_err(|e| e.to_string())?
        {
            return Err(format!(
                "unselected DXF primitive {} touches or enters the inspection opening",
                index + 1
            ));
        }
        Ok(())
    }

    /// Compare nominal curve extrema in the declared WCS with the mounted table's program travel.
    pub fn check_machine(&self, machine: &crate::machine::Machine) -> Result<(), String> {
        if let Some(axis) = machine.axes.iter().find(|a| !a.direction.is_standard()) {
            return Err(format!(
                "Agie inspection cannot encode the inverted {} axis in its native geometry",
                axis.name
            ));
        }
        let (lo, hi) = self.bounds;
        for (k, (first, last)) in [(lo.x, hi.x), (lo.y, hi.y)].into_iter().enumerate() {
            let name = ["X", "Y"][k];
            let axis = machine
                .axis(k)
                .filter(|a| !a.rotary)
                .ok_or_else(|| format!("Agie inspection requires a mounted linear {name} axis"))?;
            if let Some((min, max)) = axis.program_travel()
                && (!min.is_finite() || !max.is_finite() || min >= max || first < min || last > max)
            {
                return Err(format!(
                    "the nominal inspection contour reaches {first:.5}..{last:.5} in {name}; mounted travel is {min:.5}..{max:.5}"
                ));
            }
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    fn iso(&self, stem: &str) -> String {
        let mut iso = format!("( {stem}.ISO )\r\n");
        for (i, b) in self.blocks.iter().enumerate() {
            let (a, z) = (xy(b.start), xy(b.end));
            if i == 0 {
                let _ = write!(iso, "N0001 G00 X{:.5} Y{:.5} ;\r\n", a.x, a.y);
            }
            let no = i + 2;
            if let (Some(c), Primitive::Arc { sweep_angle, .. }) = (b.center, &b.curve) {
                let offset = xy([c[0] - b.start[0], c[1] - b.start[1]]);
                let motion = if *sweep_angle > 0.0 { "G03" } else { "G02" };
                let _ = write!(
                    iso,
                    "N{no:04} {motion} X{:.5} Y{:.5} I{:.5} J{:.5} ;\r\n",
                    z.x, z.y, offset.x, offset.y
                );
            } else {
                let _ = write!(iso, "N{no:04} G01 X{:.5} Y{:.5} ;\r\n", z.x, z.y);
            }
        }
        let _ = write!(iso, "N{:04} M02 ;\r\n", self.blocks.len() + 2);
        iso
    }
}

impl InspectionProfile {
    /// Publish a reference-only native work. Physical stock XY is unknown.
    pub fn export(&self, dest: &Path) -> Result<Vec<PathBuf>, String> {
        let stem = dest
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| super::valid_stem(s))
            .ok_or_else(|| super::SetupError::InvalidStem.to_string())?;
        let (lo, hi) = self.bounds;
        for (first, last, travel) in [(lo.x, hi.x, 350.0), (lo.y, hi.y, 250.0)] {
            if last.max(0.0) - first.min(0.0) > travel {
                return Err("the nominal inspection opening exceeds the native XY span".into());
            }
        }
        let files = [
            (format!("{stem}.ISO"), self.iso(stem)),
            (
                format!("{stem}.SBR"),
                format!("AGIE.USING_{stem}.ISO IMPORT {stem}.ISO;\r\n"),
            ),
            (format!("{stem}.SBL"), piece::encode(stem, self.hole)),
        ];
        super::publish::publish(dest, &files)
    }
}
