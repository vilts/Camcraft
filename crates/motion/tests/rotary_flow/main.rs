//! Issue #2824, the Rotary Flow kernel: flow lines between picked
//! helical guide chains on a rotor, unwrapped about a STATED axis
//! (the #2772 seam - the axis is a parameter, never fitted), aligned
//! by the nearest whole turn, pushed out radially until the
//! closest-point oracle clears, and linked on the retract cylinders.
//!
//! A directory suite: the rotor laws (`rotor`), the swept-ball and
//! approach proofs (`sweep`), and the guide laws - closed laps, the
//! band width at every knot, extreme coordinates (`guides`) - share
//! one binary and one fixture module.
//!
//! Gate map (M4.4): covers camcraft_motion::multiaxis (rotary_flow,
//! the #2824 kernel) on this crate's own fixtures.
//! Lane shape (M4.4): the default tests run in seconds; the rotor
//! envelope scan rides the heavy lane.

mod fixture;
mod guides;
mod rotor;
mod sweep;
