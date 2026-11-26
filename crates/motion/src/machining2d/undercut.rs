//! Slot undercut (#1444): side-entry passes for T-slot, keyseat, dovetail and lollipop cutters along a straight or circular slot - the shop rule that an undercutting cutter enters and leaves sideways at depth and never moves in Z under material (general shop practice; cutter forms per Machinery's Handbook, T-slot, Woodruff keyseat and dovetail cutters); the pass layout is our own. No vendor source consulted.
//!
//! The law this generator keeps by construction: every Z move stands at
//! ONE XY station, the entry station `a - run_in * tangent`, which the
//! user places outside the wall. The head goes down there, feeds
//! sideways into the slot, cuts every radial pass at one depth, feeds
//! back out the way it came and only then lifts. No link, lead or
//! retract changes Z anywhere else, so none can lift the head under the
//! roof of the undercut. The verifier proves the station itself against
//! the stock (`verify`'s undercut law); this module cannot see stock.
//!
//! Passes: the centreline first, then pairs of offsets that widen the
//! slot by `stepover` up to `h = width / 2 - contact radius`, where the
//! contact radius is the widest point of the cutter's own profile
//! ([`Undercut::contact`]), not its tip. Each offset pass keeps the wall
//! it cuts on the side `climb` asks for. The far end links offsets with
//! a straight lateral feed, so the cutter centre never leaves the band
//! within `h` of the centreline: the undercut is never wider than asked.

use camcraft_tools::undercut::Undercut;

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::face::{GenError, band};

/// The largest centreline bulge: a half circle. A slot that sweeps more
/// is refused by name (its entry tangent would re-enter the slot).
pub const MAX_BULGE: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UndercutParams {
    /// The cutter, read by its profile.
    pub cutter: Undercut,
    /// Width of the finished undercut at the cutter's widest point.
    pub width: f64,
    /// The centreline's bulge from `a` to `b`, DXF style: the tangent of
    /// a quarter of the swept angle, positive counter-clockwise, 0 for a
    /// straight slot.
    pub bulge: f64,
    /// Distance from `a` back along the entry tangent to the station
    /// where the cutter goes down and comes up, outside the wall.
    pub run_in: f64,
    /// Width of the slot cut earlier for the neck, above the undercut;
    /// 0 states that nothing stands above the neck's path.
    pub neck_slot: f64,
    pub clearance_z: f64,
    /// Where the rapid down ends and the feed down starts; `None` feeds
    /// from the clearance plane.
    pub retract_z: Option<f64>,
    /// The cutter tip's plane: the floor of the undercut.
    pub bottom_z: f64,
    pub stepover: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub climb: bool,
}

/// The centreline: a segment or a circular arc from `a` to `b`.
#[derive(Clone, Copy, Debug)]
enum Line {
    Straight { a: Point2, u: Point2, len: f64 },
    Arc { c: Point2, r: f64, a: Point2, b: Point2, ccw: bool, sweep: f64 },
}

impl Line {
    fn new(a: Point2, b: Point2, bulge: f64) -> Result<Self, GenError> {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = dx.hypot(dy);
        if !(len.is_finite() && len > 1e-9) {
            return Err(GenError::Invalid("slot endpoints must be distinct".into()));
        }
        let u = Point2::new(dx / len, dy / len);
        if bulge.abs() < 1e-12 {
            return Ok(Line::Straight { a, u, len });
        }
        // Half the swept angle, signed: the centre stands left of the
        // chord for a counter-clockwise arc.
        let half = 2.0 * bulge.atan();
        let off = len * 0.5 / half.tan();
        let c = Point2::new((a.x + b.x) * 0.5 - u.y * off, (a.y + b.y) * 0.5 + u.x * off);
        let r = (len * 0.5 / half.sin()).abs();
        Ok(Line::Arc { c, r, a, b, ccw: bulge > 0.0, sweep: 2.0 * half })
    }

    /// The end (`far` = at `b`) shifted `d` to the left of travel a -> b.
    fn end(&self, far: bool, d: f64) -> Point2 {
        match *self {
            Line::Straight { a, u, len } => {
                let t = if far { len } else { 0.0 };
                Point2::new(a.x + u.x * t - u.y * d, a.y + u.y * t + u.x * d)
            }
            Line::Arc { c, r, a, b, ccw, .. } => {
                let p = if far { b } else { a };
                let k = (if ccw { r - d } else { r + d }) / r;
                Point2::new(c.x + (p.x - c.x) * k, c.y + (p.y - c.y) * k)
            }
        }
    }

    /// The point at fraction `t` of the way a -> b, shifted `d` to the
    /// left of travel.
    fn at(&self, t: f64, d: f64) -> Point2 {
        match *self {
            Line::Straight { a, u, len } => Point2::new(a.x + u.x * len * t - u.y * d, a.y + u.y * len * t + u.x * d),
            Line::Arc { c, r, a, ccw, sweep, .. } => {
                let ang = (a.y - c.y).atan2(a.x - c.x) + sweep * t;
                let k = if ccw { r - d } else { r + d };
                Point2::new(c.x + k * ang.cos(), c.y + k * ang.sin())
            }
        }
    }

    /// The unit direction of travel at `a`.
    fn tangent(&self) -> Point2 {
        match *self {
            Line::Straight { u, .. } => u,
            Line::Arc { c, r, a, ccw, .. } => {
                let (ex, ey) = ((a.x - c.x) / r, (a.y - c.y) / r);
                if ccw { Point2::new(-ey, ex) } else { Point2::new(ey, -ex) }
            }
        }
    }
}

impl UndercutParams {
    /// How far the passes shift the cutter centre off the centreline.
    fn shift(&self) -> f64 {
        self.width * 0.5 - self.cutter.contact().0
    }

    fn validate(&self) -> Result<(), GenError> {
        let c = &self.cutter;
        let nums = [
            c.r, c.height, c.neck_r, self.width, self.bulge, self.run_in, self.neck_slot, self.clearance_z,
            self.retract_z.unwrap_or(self.clearance_z), self.bottom_z, self.stepover, self.feed, self.plunge_feed,
        ];
        let bad = |why: String| Err(GenError::Invalid(why));
        if nums.iter().any(|v| !v.is_finite()) {
            return bad("parameters must be finite".into());
        }
        if !(c.neck_r > 0.0 && c.neck_r < c.r && c.height > 0.0) {
            return bad("slot undercut needs a cutter whose head is wider than its neck".into());
        }
        if !(self.feed > 0.0 && self.plunge_feed > 0.0) {
            return bad("feeds must be positive".into());
        }
        if self.stepover <= 0.0 {
            return bad("stepover must be positive".into());
        }
        if self.width <= 0.0 {
            return bad("undercut width must be positive".into());
        }
        let h = self.shift();
        if h < -1e-9 {
            return bad(format!("undercut width {} mm is narrower than the {} mm cutter head", self.width, 2.0 * c.r));
        }
        if self.run_in <= 0.0 {
            return bad("entry run-in must be positive: the cutter goes down outside the wall".into());
        }
        if self.bulge.abs() > MAX_BULGE + 1e-12 {
            return bad(format!("slot bulge {} sweeps more than a half circle", self.bulge));
        }
        if self.neck_slot < 0.0 {
            return bad("entry slot width must not be negative".into());
        }
        let need = 2.0 * (c.neck_r + h.max(0.0));
        if self.neck_slot > 0.0 && self.neck_slot < need - 1e-9 {
            return bad(format!(
                "entry slot {} mm is too narrow for the {} mm cutter neck: the passes shift the neck {:.3} mm to each side, so it needs {:.3} mm",
                self.neck_slot,
                2.0 * c.neck_r,
                h.max(0.0),
                need
            ));
        }
        if self.clearance_z <= self.bottom_z {
            return bad("clearance plane must be above the undercut floor".into());
        }
        if self.retract_z.is_some_and(|z| z < self.bottom_z || z > self.clearance_z) {
            return bad("retract plane must be between the undercut floor and the clearance plane".into());
        }
        Ok(())
    }
}

/// The XY station where the path changes Z: `run_in` behind `a` on the
/// entry tangent. The same point [`undercut`] builds from.
pub fn undercut_station(a: Point2, b: Point2, p: &UndercutParams) -> Result<Point2, GenError> {
    let line = Line::new(a, b, p.bulge)?;
    let t = line.tangent();
    Ok(Point2::new(a.x - t.x * p.run_in, a.y - t.y * p.run_in))
}

/// The outline of the band `half` to each side of the centreline
/// `a` -> `b` (square ends): the left edge out, the right edge back, as
/// a closed polygon. The footprint the operation layer shows and judges
/// the slot by. An arc is sampled finely enough to stand within 0.01 mm
/// of its circle.
pub fn undercut_band(a: Point2, b: Point2, bulge: f64, half: f64) -> Result<Vec<Point2>, GenError> {
    if !(half.is_finite() && half > 0.0 && bulge.is_finite() && bulge.abs() <= MAX_BULGE + 1e-12) {
        return Err(GenError::Invalid("slot undercut band needs a positive half width and a bulge within a half circle".into()));
    }
    let line = Line::new(a, b, bulge)?;
    let n = match line {
        Line::Straight { .. } => 1usize,
        Line::Arc { r, sweep, .. } => {
            if r - half <= 1e-6 {
                return Err(GenError::Invalid(format!("slot arc radius {r:.3} mm is not larger than its {half:.3} mm half width")));
            }
            // Sagitta 0.01 mm on the outer edge: step = 2 acos(1 - s / R).
            let outer = r + half;
            let step = 2.0 * (1.0 - 0.01 / outer).clamp(-1.0, 1.0).acos();
            ((sweep.abs() / step.max(1e-3)).ceil() as usize).clamp(2, 720)
        }
    };
    let mut pts: Vec<Point2> = (0..=n).map(|k| line.at(k as f64 / n as f64, half)).collect();
    pts.extend((0..=n).rev().map(|k| line.at(k as f64 / n as f64, -half)));
    if pts.iter().any(|q| !(q.x.is_finite() && q.y.is_finite())) {
        return Err(GenError::Invalid("slot undercut band left the finite horizon".into()));
    }
    Ok(pts)
}

/// Cut the undercut of the slot whose centreline runs `a` -> `b`.
pub fn undercut(a: Point2, b: Point2, p: &UndercutParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
        return Err(GenError::Invalid("slot endpoints must be finite".into()));
    }
    let line = Line::new(a, b, p.bulge)?;
    let h = p.shift().max(0.0);
    if let Line::Arc { r, .. } = line
        && r - h <= 1e-6
    {
        return Err(GenError::Invalid(format!(
            "slot arc radius {r:.3} mm is not larger than the {h:.3} mm the passes shift toward its centre"
        )));
    }
    // 0, stepover, 2 stepover .. h: the centre pass, then each widening.
    let shifts = band(0.0, h, 0.0, p.stepover)?;
    let station = undercut_station(a, b, p)?;
    let z = p.bottom_z;
    let at = |q: Point2| Point3::new(q.x, q.y, z);

    let mut bld = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    bld.push(MoveKind::Rapid, Point3::new(station.x, station.y, p.clearance_z), None);
    if let Some(rz) = p.retract_z {
        bld.push(MoveKind::Rapid, Point3::new(station.x, station.y, rz), None);
    }
    bld.push(MoveKind::Plunge, at(station), Some(p.plunge_feed));

    // One pass along the slot at shift `d`, toward `b` or back to `a`.
    let run = |bld: &mut PathBuilder, d: f64, toward_b: bool| {
        let to = at(line.end(toward_b, d));
        match line {
            Line::Straight { .. } => bld.push(MoveKind::Feed, to, Some(p.feed)),
            Line::Arc { c, ccw, .. } => bld.push_arc(MoveKind::Feed, to, Some(p.feed), c, ccw == toward_b),
        }
    };
    let feed_to = |bld: &mut PathBuilder, q: Point2| bld.push(MoveKind::Feed, at(q), Some(p.feed));

    // The lead: straight in along the entry tangent, then the centre pass.
    feed_to(&mut bld, line.end(false, 0.0));
    run(&mut bld, 0.0, true);
    // Travelling b -> a, the left-of-(a -> b) wall is on the tool's
    // right: climb for a clockwise spindle. Conventional swaps sides.
    let side = if p.climb { 1.0 } else { -1.0 };
    for &m in shifts.iter().filter(|m| **m > 1e-9) {
        feed_to(&mut bld, line.end(true, side * m));
        run(&mut bld, side * m, false);
        feed_to(&mut bld, line.end(false, -side * m));
        run(&mut bld, -side * m, true);
    }
    // Out the way it came: back to the centreline, along it, to the station.
    feed_to(&mut bld, line.end(true, 0.0));
    run(&mut bld, 0.0, false);
    feed_to(&mut bld, station);
    bld.push(MoveKind::Rapid, Point3::new(station.x, station.y, p.clearance_z), None);

    let path = bld.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    // The construction law, checked on what was built: a Z move off the
    // station is a refusal, never a posted lift under the roof.
    let off_station = |m: &crate::cl::Move| {
        m.from.z != m.to.z
            && [m.from, m.to].iter().any(|q| (q.x - station.x).hypot(q.y - station.y) > 1e-9)
    };
    if path.moves.iter().any(off_station) {
        return Err(GenError::Invalid("slot undercut built a Z move away from its entry station".into()).with_ghost(&path));
    }
    Ok(path)
}

#[cfg(test)]
#[path = "undercut_tests.rs"]
mod tests;
