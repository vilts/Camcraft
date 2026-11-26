//! The world exit of the Rotary Flow kernel (#2824): the ball-center
//! passes the pass module chorded, posed and emitted as the stream's
//! records. Every point is a TOOL TIP (the pivot law, `retip`: a
//! pose turned about the tip carries the ball off the skin it was
//! planned against), every cut move is posed by the lean law on the
//! radial at its own ball center (`tilt`, the rotary twins' own
//! law), rapids travel under the radial they inherit (the post keeps
//! the tilt modal), and the links are rebuilt radially like the
//! rotary twins' (`rotary_links`): a feed retract along the radial to
//! the hop cylinder, an index chorded on that cylinder the short way
//! round, a plunge along the next pass's own radial. The program
//! opens on the clearance pose above its first cut (M13.8) and exits
//! the same way.
//!
//! The cylinders are named in BALL-CENTER radii about the stated
//! axis (the hop and clearance of the params): every link point is
//! a center pushed straight out along its own radial, so the tool
//! never swings sideways through the band it just cut.
//!
//! Provenance: radial retract / angular index / radial infeed
//! linking of 4-axis rotary milling from general machining practice
//! (`rotary_links` states the star-shaped argument); our own
//! composition. No vendor material consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::Point3;
use crate::machining2d::{GenError, ARC_FIT_TOL};

use super::axis::Axis;
use super::retip::{radial_at, tip_of};
use super::rotary_flow_pass::{cyl_of, station};
use super::rotary_flow_unwrap::{Basis, TAU};
use super::tilt::{command_axis, AxisControl};

/// Chord cap of one cylinder index (`rotary_links`' own clamp).
const INDEX_CHORDS: usize = 8192;
/// Half of the short-way unwrapping constant, spelled once.
const PI: f64 = std::f64::consts::PI;

/// The kernel's builder state: one program, opened once, linked
/// through every band.
pub(super) struct Emitter<'a> {
    ax: &'a Axis,
    basis: &'a Basis,
    c: &'a AxisControl,
    r: f64,
    /// Ball-center radius of the clearance cylinder.
    clear: f64,
    feed: f64,
    plunge_feed: f64,
    builder: Option<PathBuilder>,
    held: Option<[f64; 3]>,
}

impl<'a> Emitter<'a> {
    #[allow(clippy::too_many_arguments)] // one emitter's flat read
    pub(super) fn new(
        ax: &'a Axis,
        basis: &'a Basis,
        c: &'a AxisControl,
        r: f64,
        clear: f64,
        feed: f64,
        plunge_feed: f64,
    ) -> Self {
        Emitter {
            ax,
            basis,
            c,
            r,
            clear,
            feed,
            plunge_feed,
            builder: None,
            held: None,
        }
    }

    /// The builder, once opened - a private invariant the kernel
    /// keeps by calling [`open`](Self::open) before anything else.
    fn b(&mut self) -> Result<&mut PathBuilder, GenError> {
        self.builder
            .as_mut()
            .ok_or_else(|| GenError::Invalid("Rotary Flow: the emitter was not opened".into()))
    }

    /// The cut pose at ball center `w` travelling `t`: the radial
    /// through the lean law (rotary.rs's own cut_pose, refused by
    /// name where no pose can be framed, never guessed).
    fn cut_pose(&self, w: Point3, t: Point3) -> Result<[f64; 3], GenError> {
        let Some(n3) = radial_at(self.ax, w) else {
            return Err(GenError::invalid_at(
                "Rotary Flow: a cut station stands on the axis line - no radial to pose",
                w,
            ));
        };
        let n = Point3::new(n3[0], n3[1], n3[2]);
        let l = t.dot(t).sqrt();
        if !(l.is_finite() && l > 1e-12) {
            return Ok(n3); // nowhere to lead: hold the radial
        }
        let a = command_axis(n, Point3::new(t.x / l, t.y / l, t.z / l), self.c).ok_or_else(|| {
            GenError::invalid_at(
                "Rotary Flow: the lean law cannot frame a pose at this station - the axis \
                 control's angles are not usable",
                w,
            )
        })?;
        Ok([a.x, a.y, a.z])
    }

    /// A rapid's pose: the radial at its own ball center, held where
    /// none exists (`retip`'s law, said never silent).
    fn air_pose(&mut self, w: Point3) -> Option<[f64; 3]> {
        let p = radial_at(self.ax, w).or(self.held);
        if p.is_none() {
            tracing::warn!("multiaxis: a Rotary Flow rapid at {w:?} stands on the axis; it holds the last pose");
        }
        p
    }

    /// The program's opening (M13.8): the clearance pose over the
    /// first cut - the pose-stating record first, then the plunge
    /// into the first station along its own radial.
    pub(super) fn open(&mut self, first: Point3) -> Result<(), GenError> {
        let (v, th, _) = cyl_of(self.ax, self.basis, first);
        let top = station(self.ax, self.basis, th, v, self.clear);
        let Some(rad) = radial_at(self.ax, top) else {
            return Err(GenError::invalid_at(
                "Rotary Flow: the opening station stands on the axis line - no radial to hold",
                top,
            ));
        };
        let t0 = tip_of(top, Some(rad), self.r);
        let mut b = PathBuilder::new(t0);
        b.set_axis(Some(rad));
        b.push(MoveKind::Rapid, t0, None);
        self.builder = Some(b);
        self.held = Some(rad);
        self.plunge_from(self.clear, first)
    }

    /// Plunge from the `cyl` cylinder above `into`, along `into`'s
    /// own radial, at plunge feed.
    fn plunge_from(&mut self, cyl: f64, into: Point3) -> Result<(), GenError> {
        let (v, th, _) = cyl_of(self.ax, self.basis, into);
        let above = station(self.ax, self.basis, th, v, cyl);
        let pose = self.cut_pose(into, into - above)?;
        let tip = tip_of(into, Some(pose), self.r);
        let feed = self.plunge_feed;
        {
            let b = self.b()?;
            b.set_axis(Some(pose));
            b.push(MoveKind::Plunge, tip, Some(feed));
        }
        self.held = Some(pose);
        Ok(())
    }

    /// One pass's cut moves: ball centers in, tips out, every move
    /// posed by the lean law on its own travel.
    pub(super) fn pass(&mut self, centers: &[Point3]) -> Result<(), GenError> {
        if centers.len() < 2 {
            return Err(GenError::Invalid(
                "Rotary Flow: a flow pass holds fewer than two stations".into(),
            ));
        }
        let feed = self.feed;
        for k in 1..centers.len() {
            let travel = centers[k] - centers[k - 1];
            let pose = self.cut_pose(centers[k], travel)?;
            let tip = tip_of(centers[k], Some(pose), self.r);
            {
                let b = self.b()?;
                b.set_axis(Some(pose));
                b.push(MoveKind::Feed, tip, Some(feed));
            }
            self.held = Some(pose);
        }
        Ok(())
    }

    /// Leave one pass's last center: a feed retract along its radial
    /// to the `cyl` cylinder (`rotary_links`' cut-feed exit off a
    /// pass, on this kernel's ball-center radius).
    pub(super) fn retract(&mut self, last: Point3, cyl: f64) -> Result<(), GenError> {
        let (v, th, _) = cyl_of(self.ax, self.basis, last);
        let top = station(self.ax, self.basis, th, v, cyl);
        let pose = self.cut_pose(top, top - last)?;
        let tip = tip_of(top, Some(pose), self.r);
        let feed = self.feed;
        {
            let b = self.b()?;
            b.set_axis(Some(pose));
            b.push(MoveKind::Feed, tip, Some(feed));
        }
        self.held = Some(pose);
        Ok(())
    }

    /// The index onto the `cyl` cylinder over `next`, from wherever
    /// the same cylinder already holds the tool (`rotary_links`' law:
    /// short way round, chorded at `ARC_FIT_TOL`), then the plunge
    /// into `next` along its own radial.
    pub(super) fn link(&mut self, last: Point3, next: Point3, cyl: f64) -> Result<(), GenError> {
        let (v0, th0, _) = cyl_of(self.ax, self.basis, last);
        let (v1, th1, _) = cyl_of(self.ax, self.basis, next);
        let dphi = (th1 - th0 + PI).rem_euclid(TAU) - PI;
        let cap = 2.0 * (1.0 - (ARC_FIT_TOL / cyl).min(2.0)).clamp(-1.0, 1.0).acos();
        let n = if cap > 1e-12 {
            (dphi.abs() / cap).ceil() as usize
        } else {
            1
        };
        let n = n.clamp(1, INDEX_CHORDS);
        let mut at = station(self.ax, self.basis, th0, v0, cyl);
        for j in 1..=n {
            let t = j as f64 / n as f64;
            let to = if j == n {
                station(self.ax, self.basis, th1, v1, cyl)
            } else {
                let (x, ph) = (v0 + (v1 - v0) * t, th0 + dphi * t);
                station(self.ax, self.basis, ph, x, cyl)
            };
            if to != at {
                let pose = self.air_pose(to);
                let tip = tip_of(to, pose, self.r);
                {
                    let b = self.b()?;
                    b.set_axis(pose);
                    b.push(MoveKind::Rapid, tip, None);
                }
                self.held = pose.or(self.held);
                at = to;
            }
        }
        self.plunge_from(cyl, next)
    }

    /// The program's exit off a band: rapid out along the last radial
    /// to the clearance cylinder (`rotary_links`' exit law).
    pub(super) fn exit(&mut self, last: Point3) -> Result<(), GenError> {
        let (v, th, _) = cyl_of(self.ax, self.basis, last);
        let top = station(self.ax, self.basis, th, v, self.clear);
        let pose = self.air_pose(top);
        let tip = tip_of(top, pose, self.r);
        {
            let b = self.b()?;
            b.set_axis(pose);
            b.push(MoveKind::Rapid, tip, None);
        }
        self.held = pose.or(self.held);
        Ok(())
    }

    /// The finished program; `notes` ride the path to the status
    /// line, the kernel's own statements of what it did.
    pub(super) fn finish(mut self, notes: Vec<String>) -> Result<Toolpath, GenError> {
        let b = self
            .builder
            .take()
            .ok_or_else(|| GenError::Empty("Rotary Flow planned no moves - no band survived the picks".into()))?;
        let mut out = b.finish();
        if out.moves.is_empty() {
            return Err(GenError::Empty(
                "Rotary Flow planned no moves - no band survived the picks".into(),
            ));
        }
        if !out.all_finite() {
            return Err(GenError::Invalid(
                "a Rotary Flow path left the finite horizon".into(),
            ));
        }
        out.notes = notes;
        Ok(out)
    }
}
