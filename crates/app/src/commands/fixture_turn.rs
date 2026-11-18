//! `TurnFixture`: turn a fixture about a world axis through its own
//! origin (#2174) - the Fixtures page's quarter-turn buttons and the MCP
//! `turn_fixture` tool, one undo step each. The write is a
//! [`MoveFixture`], so plate riders, the plate-mount marker and the
//! interpenetration warning ride along exactly as for a move.
//!
//! Provenance: our own design; quaternion composition is published rigid
//! transform algebra. No vendor material.
//!
//! ## The laws here
//!
//! - **The turn premultiplies.** The new orientation is `turn ⊗ old`:
//!   a turn about world Z stays about world Z whatever the fixture's
//!   tilt, the Model page's quarter-turn law. The origin stays put.
//! - **No rotation dust (#2177).** A quarter turn lands on exact
//!   quaternion components ([`settle_rot`]); a turn that leaves the
//!   orientation as it was is refused, so it can write nothing.
//! - **Held bodies do not follow; they are named.** A part or the stock
//!   that the fixture held before the turn stays where it is. The status
//!   line names each one, so it is never left floating in silence
//!   (`docs/decisions/2174-fixture-turns.md`).
//! - **The checks read the turned steel.** Fixtures are live gate
//!   inputs (`fixture`'s live-gate law): the post gate judges every
//!   toolpath against the fixture as it stands after the turn.

use flecs_ecs::prelude::World;

use super::fixture_move::MoveFixture;
use crate::commands::{Command, CommandError};
use crate::geometry::{quat_from_axis_angle, quat_mul};
use crate::model;
use crate::schema::Location;

/// Turn a fixture about world X, Y or Z by a number of degrees.
pub struct TurnFixture {
    write: MoveFixture,
    warning: Option<String>,
}

impl TurnFixture {
    /// `axis` is 0, 1 or 2 (X, Y, Z); `degrees` follows the right-hand
    /// rule (+ is counter-clockwise looking down the axis).
    pub fn new(world: &World, name: &str, axis: usize, degrees: f64) -> Result<Self, CommandError> {
        if axis > 2 {
            return Err(CommandError::Needs("turn the fixture about X, Y or Z".into()));
        }
        if !degrees.is_finite() {
            return Err(CommandError::Needs("the turn angle must be a number of degrees".into()));
        }
        let entity = super::fixture::require_writable(world, name)?;
        let prev = model::transform_of(world, &entity).unwrap_or_default();
        let turn = quat_from_axis_angle(axis, degrees.rem_euclid(360.0).to_radians());
        let rot = settle_rot(prev.rot, quat_mul(turn, prev.rot));
        if rot == prev.rot {
            return Err(CommandError::Needs(format!(
                "a turn of {degrees} degrees leaves '{name}' as it stands - nothing to do"
            )));
        }
        let next = Location { pos: prev.pos, rot };
        let write = MoveFixture::locating(world, name, next)?;
        // The move's own checks: the turned steel against the parts and
        // the stock, and each carried rider where it lands.
        let mut warnings: Vec<String> = crate::fixture_clash::after_fixture_move(world, name, next).into_iter().collect();
        warnings.extend(write.rider_warnings(world));
        let held = crate::fixture_clash::held_by(world, name, prev);
        if !held.is_empty() {
            warnings.push(held_message(name, &held));
        }
        Ok(TurnFixture { write, warning: crate::fixture_clash::join(warnings) })
    }

    /// The interpenetration and held-body warnings, if any.
    #[must_use]
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }
}

impl Command for TurnFixture {
    fn apply(&self, world: &World) {
        self.write.apply(world);
    }

    fn invert(&self, world: &World) {
        self.write.invert(world);
    }
}

/// The by-name line for the bodies a turned fixture held.
#[must_use]
pub fn held_message(fixture: &str, held: &[String]) -> String {
    let names = held.iter().map(|n| format!("'{n}'")).collect::<Vec<_>>().join(", ");
    format!("{names} did not turn with the fixture '{fixture}' - locate them on it again")
}

/// A component within this of an exact quarter-turn value snaps to it.
const SNAP: f64 = 1e-12;
/// Two orientations whose unit dot product is within this of 1 are the
/// same: about 3e-7 radians, far below any setup tolerance, far above
/// the dust of a degrees round trip.
const SAME: f64 = 1e-14;

/// The orientation to write for `next` when the fixture stood at `prev`
/// (#2177): `prev` itself when `next` is the same rotation (no dust), the
/// exact quarter-turn quaternion when every component of `next` lies
/// within float dust of one, else `next`. Signs are canonical (w >= 0).
#[must_use]
pub fn settle_rot(prev: [f64; 4], next: [f64; 4]) -> [f64; 4] {
    let dot: f64 = (0..4).map(|i| prev[i] * next[i]).sum();
    if (1.0 - dot.abs()).abs() < SAME {
        return prev;
    }
    let exact = [0.0, 0.5, std::f64::consts::FRAC_1_SQRT_2, 1.0];
    let snap = |v: f64| exact.iter().find(|e| (v.abs() - **e).abs() < SNAP).map(|e| e.copysign(v));
    let mut q = next;
    if let (Some(a), Some(b), Some(c), Some(d)) = (snap(next[0]), snap(next[1]), snap(next[2]), snap(next[3])) {
        q = [a, b, c, d];
    }
    if q[3] < 0.0 {
        q = [-q[0], -q[1], -q[2], -q[3]];
    }
    q.map(|v| if v == 0.0 { 0.0 } else { v })
}
