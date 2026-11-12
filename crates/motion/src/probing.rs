//! Probing cycles (#108): face, edge, outside corner and bore / boss
//! centre finding with a spindle touch probe, as a neutral motion
//! stream - each touch a fast seek, a back-off and a slow measuring
//! re-probe, the touches marked as [`ProbeTouch`]es the post writes as
//! probing blocks. Provenance: NIST RS274NGC (NISTIR 6556) G38.2 probe
//! semantics and general touch-probe practice from machine-tool
//! handbooks (seek fast, back off, measure slow; a bore centre is the
//! midpoint of opposite wall touches). Our own design and names; no
//! vendor material consulted.

use crate::cl::{Move, MoveKind, ProbeTouch, Toolpath};
use crate::geometry::Point3;
use crate::machining2d::GenError;

/// The direction the stylus travels toward the surface it touches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeDir {
    XPos,
    XNeg,
    YPos,
    YNeg,
    ZNeg,
}

impl ProbeDir {
    #[must_use]
    pub fn unit(self) -> Point3 {
        match self {
            ProbeDir::XPos => Point3::new(1.0, 0.0, 0.0),
            ProbeDir::XNeg => Point3::new(-1.0, 0.0, 0.0),
            ProbeDir::YPos => Point3::new(0.0, 1.0, 0.0),
            ProbeDir::YNeg => Point3::new(0.0, -1.0, 0.0),
            ProbeDir::ZNeg => Point3::new(0.0, 0.0, -1.0),
        }
    }

    #[must_use]
    pub fn is_x(self) -> bool {
        matches!(self, ProbeDir::XPos | ProbeDir::XNeg)
    }

    #[must_use]
    pub fn is_y(self) -> bool {
        matches!(self, ProbeDir::YPos | ProbeDir::YNeg)
    }
}

/// What the cycle finds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeMode {
    /// One touch on a face along `dir` (X, Y or -Z).
    Face,
    /// One touch on the side face under a picked straight edge, `depth`
    /// below the edge (X or Y travel).
    Edge,
    /// An outside corner: an X touch and a Y touch, each `inset` along
    /// its edge from the picked vertex and `depth` below it.
    Corner,
    /// A bore's centre: four touches outward from the centre.
    Bore,
    /// A boss's centre: four touches inward from outside the boss.
    Boss,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeParams {
    pub mode: ProbeMode,
    /// Face / edge: the travel direction. Corner: the X touch's travel.
    pub dir: ProbeDir,
    /// Corner only: the Y touch's travel.
    pub dir_y: ProbeDir,
    /// Face: the contact point on the face. Edge / corner: the picked
    /// edge point / vertex (the top of the side faces). Bore / boss:
    /// the circle centre at its top rim.
    pub target: Point3,
    /// Bore / boss nominal radius.
    pub radius: f64,
    pub ball_diameter: f64,
    /// Stylus ball surface to part, before the seek starts.
    pub approach: f64,
    /// How far past the expected contact the control may probe before
    /// it reports no contact.
    pub overtravel: f64,
    pub backoff: f64,
    pub fast_feed: f64,
    pub slow_feed: f64,
    /// Edge / corner / boss / bore: stylus centre depth below `target`.
    pub depth: f64,
    /// Corner: distance of each touch from the vertex along its edge.
    pub inset: f64,
    /// The safe plane the probe travels at between touches.
    pub clearance_z: f64,
}

impl ProbeParams {
    fn validate(&self) -> Result<(), GenError> {
        let nums = [
            self.target.x,
            self.target.y,
            self.target.z,
            self.radius,
            self.ball_diameter,
            self.approach,
            self.overtravel,
            self.backoff,
            self.fast_feed,
            self.slow_feed,
            self.depth,
            self.inset,
            self.clearance_z,
        ];
        if !nums.iter().all(|v| v.is_finite()) {
            return Err(GenError::Invalid("probe parameters must be finite".into()));
        }
        if self.ball_diameter <= 0.0 {
            return Err(GenError::Invalid("the stylus ball diameter must be positive".into()));
        }
        if self.approach <= 0.0 || self.overtravel <= 0.0 || self.backoff <= 0.0 {
            return Err(GenError::Invalid(
                "approach clearance, probe distance and back-off must be positive".into(),
            ));
        }
        if self.backoff > self.approach {
            return Err(GenError::Invalid(
                "the back-off must not exceed the approach clearance".into(),
            ));
        }
        if self.fast_feed <= 0.0 || self.slow_feed <= 0.0 || self.slow_feed > self.fast_feed {
            return Err(GenError::Invalid(
                "probe feeds must be positive and the measuring feed no faster than the seek".into(),
            ));
        }
        if self.depth < 0.0 || self.inset < 0.0 {
            return Err(GenError::Invalid("probe depth and inset must not be negative".into()));
        }
        let horizontal = |d: ProbeDir| d.is_x() || d.is_y();
        match self.mode {
            ProbeMode::Face => {}
            ProbeMode::Edge if !horizontal(self.dir) => {
                return Err(GenError::Invalid("an edge is probed along X or Y, not Z".into()));
            }
            ProbeMode::Corner if !(self.dir.is_x() && self.dir_y.is_y()) => {
                return Err(GenError::Invalid(
                    "a corner takes one X touch and one Y touch".into(),
                ));
            }
            ProbeMode::Bore if self.radius <= self.ball_diameter * 0.5 + self.backoff => {
                return Err(GenError::Invalid(format!(
                    "a bore of radius {:.3} mm has no room for a {:.3} mm stylus ball and its back-off",
                    self.radius, self.ball_diameter
                )));
            }
            ProbeMode::Boss if self.radius <= 0.0 => {
                return Err(GenError::Invalid("a boss radius must be positive".into()));
            }
            _ => {}
        }
        Ok(())
    }
}

/// One touch: the ball contact point on the surface and the travel.
struct Contact {
    at: Point3,
    dir: Point3,
}

fn add(a: Point3, d: Point3, s: f64) -> Point3 {
    Point3::new(a.x + d.x * s, a.y + d.y * s, a.z + d.z * s)
}

fn contacts(p: &ProbeParams) -> Vec<Contact> {
    let z = p.target.z - p.depth;
    let t = p.target;
    match p.mode {
        ProbeMode::Face => vec![Contact { at: t, dir: p.dir.unit() }],
        ProbeMode::Edge => vec![Contact { at: Point3::new(t.x, t.y, z), dir: p.dir.unit() }],
        ProbeMode::Corner => {
            // The part lies on the far side of each touch: an X touch
            // travelling +X meets a part that extends +X, so the Y
            // touch sits `inset` that way along the X edge - and the
            // same for the X touch along the Y edge.
            let dx = p.dir.unit();
            let dy = p.dir_y.unit();
            vec![
                Contact { at: Point3::new(t.x, t.y + dy.y * p.inset, z), dir: dx },
                Contact { at: Point3::new(t.x + dx.x * p.inset, t.y, z), dir: dy },
            ]
        }
        ProbeMode::Bore | ProbeMode::Boss => [ProbeDir::XPos, ProbeDir::XNeg, ProbeDir::YPos, ProbeDir::YNeg]
            .into_iter()
            .map(|d| {
                let u = d.unit();
                let on_wall = Point3::new(t.x + u.x * p.radius, t.y + u.y * p.radius, z);
                // A bore is touched outward, a boss inward.
                let dir = if p.mode == ProbeMode::Bore { u } else { Point3::new(-u.x, -u.y, 0.0) };
                Contact { at: on_wall, dir }
            })
            .collect(),
    }
}

/// Build the probing path. Every touch: travel at the safe plane, feed
/// down to the approach point (never a rapid below the safe plane, #570), seek fast to the expected trip, back
/// off, re-probe slow, return to the approach point and rise. A bore
/// is entered once at its centre and left from there.
pub fn probe_cycle(p: &ProbeParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    let r = p.ball_diameter * 0.5;
    let cs = contacts(p);
    let mut moves: Vec<Move> = Vec::new();
    let mut probes: Vec<ProbeTouch> = Vec::new();
    let first_start = cs
        .first()
        .map(|c| add(add(c.at, c.dir, -r), c.dir, -p.approach))
        .ok_or_else(|| GenError::Empty("no touch to make".into()))?;
    let bore = p.mode == ProbeMode::Bore;
    let centre = Point3::new(p.target.x, p.target.y, p.target.z - p.depth);
    // The plan runs on the ball CENTRE (contact - r along the travel);
    // the path states the TIP, as every ball tool's does (the pivot
    // law: centre = tip + r along the axis, here +Z). A spindle probe's
    // length register is gauged to the ball's bottom like a ball
    // mill's, so the posted G31 Z for a -Z face is the face itself -
    // programming the centre would read every face r high, and a set
    // offset would move the work zero r into the part (review of
    // 0beeec4). The safe plane is a tip height, as for every tool.
    let home = |q: Point3| Point3::new(q.x, q.y, p.clearance_z + r);
    let tip = |c: Point3| Point3::new(c.x, c.y, c.z - r);
    let entry = if bore { centre } else { first_start };
    // The approach plane: the ball the approach clearance over the probed
    // feature's top. The safe plane must stand above it. Below the safe
    // plane a probe never rapids (#570, the protected move of touch-probe
    // practice): the path opens at the safe plane over its first start
    // and FEEDS down to the approach plane at the seek feed, so a stylus
    // meeting something on the way down trips at a feed the probe can
    // take, never at full rapid. The posts position X Y, then Z down to
    // the safe plane, before a first feed (tormach.cps `startXY`).
    let rapid_z = p.target.z + r + p.approach;
    if p.clearance_z <= rapid_z {
        return Err(GenError::Invalid(format!(
            "the safe plane Z {:.3} does not clear the probed feature (top {:.3}) by the stylus ball and the approach",
            p.clearance_z, p.target.z
        )));
    }
    let mut at = home(entry);
    // A move that runs nowhere (the first rapid when the path already
    // stands above its start) is not laid down: the chain stays gapless.
    let mut go = |kind: MoveKind, to: Point3, feed: Option<f64>, moves: &mut Vec<Move>| {
        if to != at {
            moves.push(Move::new(kind, tip(at), tip(to), feed));
            at = to;
        }
    };
    // The positioning block every post writes first: a rapid that ends
    // at the safe plane over the first start (the path's own start, so it
    // runs nowhere in the path's frame - what it positions FROM is the
    // tool change, which no path knows). Then the protected feed down.
    moves.push(Move::new(MoveKind::Rapid, tip(home(entry)), tip(home(entry)), None));
    go(MoveKind::Feed, Point3::new(entry.x, entry.y, rapid_z), Some(p.fast_feed), &mut moves);
    if bore {
        go(MoveKind::Feed, centre, Some(p.fast_feed), &mut moves);
    }
    for (k, c) in cs.iter().enumerate() {
        let expect = add(c.at, c.dir, -r);
        let start = if bore { centre } else { add(expect, c.dir, -p.approach) };
        let back = add(expect, c.dir, -p.backoff);
        let target = add(expect, c.dir, p.overtravel);
        if !bore {
            // The first touch is already over its start at the approach
            // plane; later ones travel over at the safe plane.
            if k > 0 {
                go(MoveKind::Rapid, home(start), None, &mut moves);
            }
            go(MoveKind::Feed, start, Some(p.fast_feed), &mut moves);
        }
        probes.push(ProbeTouch { index: moves.len(), measure: None, target: tip(target) });
        go(MoveKind::Feed, expect, Some(p.fast_feed), &mut moves);
        go(MoveKind::Feed, back, Some(p.fast_feed), &mut moves);
        probes.push(ProbeTouch {
            index: moves.len(),
            measure: Some(u32::try_from(k).unwrap_or(u32::MAX)),
            target: tip(target),
        });
        go(MoveKind::Feed, expect, Some(p.slow_feed), &mut moves);
        go(MoveKind::Feed, start, Some(p.fast_feed), &mut moves);
        if !bore {
            go(MoveKind::Feed, home(start), Some(p.fast_feed), &mut moves);
        }
    }
    if bore {
        go(MoveKind::Feed, home(centre), Some(p.fast_feed), &mut moves);
    }
    let mut path = Toolpath::new(moves);
    path.probes = probes;
    if !path.probes.iter().all(|t| t.is_sane(&path.moves)) {
        // #871: the built cycle rides the refusal (display-only).
        return Err(GenError::Invalid("a probing touch does not run toward its target".into()).with_ghost(&path));
    }
    if !path.all_finite() || !path.is_chained() {
        return Err(GenError::Invalid("the probing path is not a finite chain".into()));
    }
    Ok(path)
}
